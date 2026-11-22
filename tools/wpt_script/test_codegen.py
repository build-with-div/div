"""Tests for the RFC 0005 code generator (tools/wpt_script/codegen.py)."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import codegen  # noqa: E402

ACORN = os.environ.get("CHROMIUM_ACORN")


def parse(source: str) -> dict:
    """The pinned Acorn's AST of one inline script."""
    payload = [{"file": "p.html", "title": None, "scripts": [{"source": source, "offset": 0}],
                "helpers": []}]
    result = subprocess.run(
        ["node", str(codegen.TOOL / "translate.mjs"), ACORN, "--ast"],
        input=json.dumps(payload), capture_output=True, text=True, encoding="utf-8",
        check=True)
    return json.loads(result.stdout)[0]["programs"][0]


def compile_function(source: str) -> str:
    """The Rust for the first statement of `source`, a function expression."""
    program = parse(source)
    page = codegen.Scope(None)
    page.collect(program["body"], program)
    return codegen.Compiler().expression(program["body"][0]["expression"], page)


class LiteralTest(unittest.TestCase):
    def test_strings_escape_for_rust(self):
        self.assertEqual(codegen.rust_string('a"b\\c\n'), '"a\\"b\\\\c\\n"')
        self.assertEqual(codegen.rust_string("é"), '"\\u{e9}"')

    def test_a_lone_surrogate_has_no_rust_string(self):
        with self.assertRaises(codegen.LoneSurrogate):
            codegen.rust_string("\ud800")

    def test_numbers_keep_their_value(self):
        self.assertEqual(codegen.rust_number(0), "0.0")
        self.assertEqual(codegen.rust_number(1e21), "1e+21")
        self.assertEqual(codegen.rust_number(float("inf")), "f64::INFINITY")


class MarkupTest(unittest.TestCase):
    def events(self, text):
        markup = codegen.Markup()
        markup.feed(text)
        markup.close()
        return markup.events

    def test_elements_text_and_scripts_keep_document_order(self):
        events = self.events(
            '<!doctype html><title>T</title><script src="/resources/testharness.js"></script>'
            '<div id="a"><span>x</span><br></div><script>1</script><p></p>')
        self.assertEqual(events, [
            ("element", 1, "div", [("id", "a")], None),
            ("element", 2, "span", [], 1),
            ("text", "x", 2),
            ("element", 3, "br", [], 1),
            ("script", 0),
            ("element", 4, "p", [], None),
        ])

    def test_style_and_helpers_are_reported(self):
        events = self.events('<style>div { color: red }</style><script src="/x.js"></script>')
        self.assertEqual(events, [("unsupported", "markup:<style>"),
                                  ("unsupported", "intrinsic:/x.js")])


class RecordTest(unittest.TestCase):
    def test_subtests_the_run_did_not_report_are_not_run_or_unsupported(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        (directory / "a.json").write_text(json.dumps({
            "page": "s/a.html", "harness_error": None, "unsupported": None,
            "subtests": [{"name": "one", "status": "PASS"}]}))
        (directory / "b.json").write_text(json.dumps({
            "page": "s/b.html", "harness_error": None, "unsupported": "markup:<style>",
            "subtests": []}))
        ledger = {"rows": [
            {"file": "s/a.html", "name": "one", "status": "translatable"},
            {"file": "s/a.html", "name": "two", "status": "translatable"},
            {"file": "s/a.html", "name": "three", "status": "blocked"},
            {"file": "s/b.html", "name": "four", "status": "translatable"},
        ]}
        original = codegen.ledger.load_json
        codegen.ledger.load_json = lambda path: ledger
        self.addCleanup(setattr, codegen.ledger, "load_json", original)
        self.assertEqual(codegen.record("s", directory, {}), {
            "s/a.html": {"harness_error": None, "subtests": {
                "one": {"status": "PASS"}, "two": {"status": "NOTRUN"}}},
            "s/b.html": {"harness_error": None, "subtests": {
                "four": {"status": "UNSUPPORTED", "message": "markup:<style>"}}},
        })


@unittest.skipUnless(ACORN, "CHROMIUM_ACORN is not set")
class CompilerTest(unittest.TestCase):
    def test_constructs_outside_the_subset_are_reported_not_approximated(self):
        for source, reason in [
            ("(function () { try { f(); } catch (e) {} })", "syntax:TryStatement"),
            ("(function () { return /x/; })", "syntax:regex"),
            ("(function () { return a?.b; })", "syntax:optional-chaining"),
            ("(async function () {})", "syntax:async-or-generator"),
            ("(function () { return [...a]; })", "syntax:array-hole-or-spread"),
            ("(function () { x = 1; })", "global-assignment:x"),
            ('(function () { "use strict"; x = 1; })', "global-assignment:x"),
        ]:
            with self.subTest(source=source):
                self.assertIn(codegen.rust_string(reason), compile_function(source))

    def test_a_block_binding_named_outside_its_block_is_not_hoisted(self):
        rust = compile_function("(function () { if (a) { let x = 1; } return x; })")
        self.assertIn('rt.unsupported("syntax:block-scoped-binding")', rust)

    def test_closures_over_a_loop_binding_are_reported(self):
        rust = compile_function(
            "(function () { for (let i = 0; i < 2; i++) { f(() => i); } })")
        self.assertIn('rt.unsupported("syntax:per-iteration-binding")', rust)

    def test_let_and_const_have_a_dead_zone(self):
        rust = compile_function("(function () { f(x); let x = 1; })")
        self.assertIn("let v_x = binding();", rust)
        self.assertIn('read(&v_x, "x")?', rust)

    def test_closures_share_their_variables(self):
        rust = compile_function("(function () { var n = 0; f(() => { n++; }); })")
        self.assertIn("let v_n = v_n.clone();", rust)
        self.assertIn("store(&v_n,", rust)

    def test_strict_functions_keep_this_and_do_not_alias_arguments(self):
        rust = compile_function(
            '(function (a) { "use strict"; a = 2; g(this, arguments[0]); })')
        self.assertNotIn("sloppy_this", rust)
        self.assertNotIn("mapped-arguments", rust)
        self.assertIn("cell(Value::array(args.clone()))", rust)

    def test_strict_mode_is_inherited_by_nested_functions(self):
        rust = compile_function(
            '(function () { "use strict"; return function () { return this; }; })')
        self.assertNotIn("sloppy_this", rust)

    def test_a_function_expression_name_is_immutable(self):
        sloppy = compile_function("(function f() { f = 1; })")
        self.assertNotIn("Assignment to constant variable", sloppy)
        self.assertNotIn("store(&v_f, t", sloppy)
        strict_mode = compile_function('(function f() { "use strict"; f = 1; })')
        self.assertIn("Assignment to constant variable", strict_mode)

    def test_arguments_aliasing_assigned_parameters_is_reported(self):
        rust = compile_function("(function (a) { a = 2; return arguments[0]; })")
        self.assertIn('rt.unsupported("syntax:mapped-arguments")', rust)
        rust = compile_function("(function (a) { return arguments[0]; })")
        self.assertNotIn("mapped-arguments", rust)

    def test_a_sloppy_function_without_this_gets_the_global_object(self):
        self.assertIn("let this_ = sloppy_this(this_)?;",
                      compile_function("(function () { return this; })"))
        # An arrow keeps its enclosing `this`.
        self.assertNotIn("sloppy_this", compile_function("(() => this)"))

    def test_testharness_assertions_are_globals_of_the_runtime(self):
        rust = compile_function("(function () { assert_equals(1, 1); })")
        self.assertIn('rt.global("assert_equals")?', rust)


if __name__ == "__main__":
    unittest.main()
