"""The porter's <dialog> and top layer support (RFC 0015 3b-porter)."""

import base64
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "wpt"))
sys.path.insert(0, str(ROOT / "tools" / "accountability"))
import port_coverage_area as COVERAGE  # noqa: E402
import port_wpt as PORT_WPT  # noqa: E402
import run_all_pixel_comparisons as RUNNER  # noqa: E402


def by_id(identifier):
    return {
        "kind": "dom-query",
        "method": "getElementById",
        "argument": identifier,
        "constant": True,
    }


def ir(*operations):
    return {
        "lowerable": True,
        "operations": [
            {**operation, "order": order} for order, operation in enumerate(operations)
        ],
    }


def bound(name):
    return {"kind": "binding", "name": name}


def bind(name, identifier):
    return {"kind": "bind-target", "binding": name, "target": by_id(identifier)}


def show_modal(identifier):
    return {"kind": "dialog-show-modal", "target": by_id(identifier)}


def show(identifier):
    return {"kind": "dialog-show", "target": by_id(identifier)}


def close(identifier):
    return {"kind": "dialog-close", "target": by_id(identifier), "return_value": None}


class DialogPorterTests(unittest.TestCase):
    def setUp(self):
        PORT_WPT.set_dialog_elements_enabled(True)
        self.addCleanup(PORT_WPT.set_dialog_elements_enabled, False)

    def parse(self, markup, mutation_ir=None):
        return PORT_WPT._parse_wpt_markup(markup, str(ROOT), mutation_ir=mutation_ir)

    def port(self, markup, mutation_ir=None):
        """(rust, template) through the coverage area's own entry point."""
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "case.html"
            path.write_text(markup, encoding="utf-8")
            result = COVERAGE.port_one(
                path, "demo", "case", dialog_elements=True, mutation_ir=mutation_ir
            )
        self.assertIsInstance(result, tuple, result)
        return result

    # Blink 147's HTMLDialogElement, as the lowered script runs it.

    def test_show_modal_orders_the_top_layer_by_call(self):
        parser = self.parse(
            "<body><dialog id=a></dialog><dialog id=b></dialog><dialog id=c></dialog></body>",
            ir(show_modal("a"), show_modal("b"), close("a"), show_modal("a"),
               show_modal("c"), show_modal("c")),
        )
        a, b, c = parser.root.children
        self.assertEqual(parser.top_layer, [b, a, c])
        self.assertTrue(all("open" in dialog.attrs for dialog in (a, b, c)))

    def test_calls_that_throw_are_unsupported_with_their_reason(self):
        cases = [
            ([show_modal("d"), show("d")], "show() on a modal dialog throws"),
            ([show("d"), show_modal("d")], "showModal() on an open non-modal dialog throws"),
            (
                [bind("d", "d"),
                 {"kind": "remove", "target": bound("d"), "parent": by_id("p")},
                 {"kind": "dialog-show-modal", "target": bound("d")}],
                "showModal() on a disconnected dialog throws",
            ),
            (
                [show_modal("d"),
                 {"kind": "remove-attribute", "target": by_id("d"), "name": "open"}],
                "open removed from a modal dialog",
            ),
        ]
        for operations, reason in cases:
            with self.subTest(reason=reason):
                with self.assertRaisesRegex(ValueError, re.escape(reason)):
                    self.parse(
                        "<body><div id=p><dialog id=d></dialog></div></body>",
                        ir(*operations),
                    )

    def test_a_removed_dialog_leaves_the_top_layer_and_stays_open(self):
        parser = self.parse(
            "<body><div id=p><dialog id=d></dialog></div></body>",
            ir(
                bind("d", "d"),
                show_modal("d"),
                {"kind": "remove", "target": bound("d"), "parent": by_id("p")},
                {"kind": "append", "target": by_id("p"), "children": [bound("d")]},
            ),
        )
        dialog = parser.root.children[0].children[0]
        self.assertEqual(parser.top_layer, [])
        self.assertIn("open", dialog.attrs)

    def test_without_the_opt_in_show_modal_and_dialogs_stay_rejected(self):
        PORT_WPT.set_dialog_elements_enabled(False)
        with self.assertRaisesRegex(ValueError, re.escape("top layer (RFC 0015 PR 3)")):
            self.parse("<body><dialog id=d></dialog></body>", ir(show_modal("d")))
        parser = self.parse("<body><dialog open></dialog></body>")
        self.assertEqual(
            PORT_WPT.analyze_portability(parser), (False, "unsupported element: <dialog>")
        )

    # The cascade the porter hands to the engine.

    def test_modal_selectors_match_the_final_top_layer_and_backdrop_rules_drop(self):
        parser = self.parse(
            "<body><style>dialog:modal { width: 10px } dialog:open { height: 5px }"
            " ::backdrop { display: none }</style>"
            "<dialog id=m></dialog><dialog id=o></dialog></body>",
            ir(show_modal("m"), show("o")),
        )
        modal, non_modal = [node for node in parser.root.children if node.tag == "dialog"]
        self.assertEqual(modal.styles.get("width"), "10px")
        self.assertNotIn("width", non_modal.styles)
        self.assertEqual(non_modal.styles.get("height"), "5px")
        self.assertNotIn(PORT_WPT._MODAL_MARKER, modal.attrs)
        self.assertEqual(len(parser.backdrop_rules), 1)
        self.assertTrue(PORT_WPT.analyze_portability(parser)[0])

    def test_shorthands_the_schema_lacks_expand_and_others_are_named(self):
        expand = PORT_WPT._expand_dialog_shorthand
        self.assertEqual(
            expand("outline", "2px solid red"),
            [("outline-width", "2px"), ("outline-style", "solid"), ("outline-color", "red")],
        )
        self.assertEqual(expand("outline", "none"), [("outline-style", "none")])
        self.assertEqual(
            expand("inset", "1px 2px"),
            [("top", "1px"), ("right", "2px"), ("bottom", "1px"), ("left", "2px")],
        )
        self.assertEqual(expand("background", "white"), [("background-color", "white")])
        self.assertIsNone(expand("background", "url(a.png) red"))
        # The engine's border literal takes `none` or all three parts; a
        # value without a style computes as `none` (dialog-popover-overlay).
        self.assertEqual(expand("border", "0"), [("border", "none")])
        self.assertEqual(expand("border-top", "2px red"), [("border-top", "none")])
        self.assertEqual(expand("border", "none"), [("border", "none")])
        self.assertEqual(expand("border", "1px solid red"), [("border", "1px solid red")])
        self.assertIsNone(expand("border", "solid"))
        self.assertIsNone(expand("border", "1px solid rgb(0, 0, 0)"))
        parser = self.parse(
            "<body><style>dialog { margin: inherit }</style><dialog open></dialog></body>"
        )
        portable, reason = PORT_WPT.analyze_portability(parser)
        self.assertFalse(portable)
        self.assertIn("CSS-wide keyword", reason)

    # Engine rule 6: the three cases, in the emitted builder.

    def dialog_calls(self, rust):
        var = re.search(r"let (n\d+) = doc\.create_dialog\(\);", rust).group(1)
        snapshot = re.findall(rf"doc\.set_style\({var}, RendererStyleValue::(\w+)\(", rust)
        author = re.findall(rf'doc\.set_author_literal\({var}, "([^"]+)", "([^"]*)"\);', rust)
        return var, snapshot, author

    def test_author_margin_border_and_position_beat_the_user_agent(self):
        rust, _ = self.port(
            "<body><style>dialog { margin: 0; border: none; position: static }</style>"
            "<dialog open>x</dialog></body>"
        )
        var, _, author = self.dialog_calls(rust)
        for declaration in [("margin", "0"), ("border", "none"), ("position", "static")]:
            self.assertIn(declaration, author)
        # Styles, then attributes (`open` among them), then the author's
        # declarations; the dialog stays open and non-modal.
        last_snapshot = rust.rindex(f"doc.set_style({var},")
        opened = rust.index(f'doc.set_attribute({var}, "open", "");')
        self.assertLess(last_snapshot, opened)
        self.assertLess(opened, rust.index(f"doc.set_author_literal({var},"))
        self.assertNotIn("show_modal", rust)

    def test_author_display_block_shows_a_closed_dialog(self):
        rust, _ = self.port(
            "<body><style>dialog { display: block }</style><dialog>x</dialog></body>"
        )
        var, _, author = self.dialog_calls(rust)
        self.assertIn(("display", "block"), author)
        self.assertNotIn(f'doc.set_attribute({var}, "open"', rust)

    def test_a_modal_dialog_without_author_css_takes_position_from_the_engine(self):
        rust, template = self.port(
            "<body><dialog id=d>x</dialog><script>d.showModal()</script></body>",
            ir(show_modal("d")),
        )
        var, snapshot, author = self.dialog_calls(rust)
        placement = {"position", "top", "right", "bottom", "left", "inset"}
        self.assertFalse(placement & {name for name, _ in author})
        self.assertFalse({"Position", "Top", "Right", "Bottom", "Left"} & set(snapshot))
        # Only the comparison harness reset, which is author CSS in Chromium.
        self.assertEqual(
            author, [("margin", "0"), ("padding", "0"), ("box-sizing", "content-box")]
        )
        # `open` comes from show_modal, after every style and attribute.
        self.assertNotIn(f'doc.set_attribute({var}, "open"', rust)
        self.assertGreater(
            rust.index(f"doc.show_modal({var});"),
            rust.rindex(f"doc.set_author_literal({var},"),
        )
        # Chromium opens it the same way: closed in the markup, then showModal().
        directives = dict(re.findall(r"<!--OPENUI_(\w+):([A-Za-z0-9_=-]+)-->", template))
        modals = json.loads(base64.urlsafe_b64decode(directives["FINAL_MODALS"]))
        self.assertEqual(modals, ["0"])
        self.assertIn('<dialog id="d" data-openui-final-modal="0">', template)
        document = RUNNER.build_html_document(template)
        self.assertIn('dialog[data-openui-final-modal="${m}"]`).showModal()', document)

    def test_a_dialog_closes_an_open_paragraph(self):
        # As in HTML's tree builder: the dialogs are the paragraph's
        # siblings, so pruning the instruction paragraph keeps them.
        rust, template = self.port(
            "<body><p>The test passes if you see green.\n<dialog id=a></dialog>"
            "<dialog id=b></dialog><script>a.showModal(); b.showModal()</script></body>",
            ir(show_modal("a"), show_modal("b")),
        )
        self.assertEqual(rust.count("create_dialog()"), 2)
        self.assertEqual(rust.count("doc.show_modal("), 2)
        self.assertEqual(
            re.findall(r'<dialog id="(\w)" data-openui-final-modal="(\d)">', template),
            [("a", "0"), ("b", "1")],
        )
        self.assertNotIn("test passes", template.lower())

    def test_every_final_modal_rank_marks_one_dialog_in_its_template(self):
        # An author marker that collides with a rank would leave the runner's
        # showModal() on the wrong element, so the porter refuses the template.
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "case.html"
            path.write_text(
                '<body><div data-openui-final-modal="0"></div><dialog id=d>x</dialog>'
                "<script>d.showModal()</script></body>",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "FINAL_MODALS ranks do not each mark"):
                COVERAGE.port_one(
                    path, "demo", "case", dialog_elements=True,
                    mutation_ir=ir(show_modal("d")),
                )

    def test_the_user_agent_color_reaches_children_but_is_not_emitted(self):
        rust, _ = self.port(
            "<body><style>body { color: red }</style><dialog open><span>x</span></dialog></body>"
        )
        _, _, author = self.dialog_calls(rust)
        self.assertNotIn("color", {name for name, _ in author})
        self.assertNotIn("Color::from_rgba8(255, 0, 0", rust.split("create_dialog")[1])

    def test_dropped_backdrop_rules_are_noted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "case.html"
            for css, expected in [("dialog::backdrop { background: red }", True), ("", False)]:
                path.write_text(
                    f"<body><style>{css}</style><dialog id=d>x</dialog>"
                    "<script>d.showModal()</script></body>",
                    encoding="utf-8",
                )
                notes = {}
                result = COVERAGE.port_one(
                    path, "demo", "case", dialog_elements=True,
                    mutation_ir=ir(show_modal("d")), notes=notes,
                )
                self.assertIsInstance(result, tuple, result)
                self.assertEqual(notes.get("backdrop_rules_dropped", False), expected)

    def test_templates_without_a_modal_dialog_keep_their_bytes(self):
        _, template = self.port(
            "<body><dialog id=d>x</dialog><script>d.show()</script></body>", ir(show("d"))
        )
        self.assertNotIn("FINAL_MODALS", template)
        self.assertNotIn("showModal", RUNNER.build_html_document(template))


class AreaConfigTests(unittest.TestCase):
    def test_area_json_is_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            original = COVERAGE.COVERAGE_DIR
            COVERAGE.COVERAGE_DIR = pathlib.Path(directory)
            try:
                self.assertIsNone(COVERAGE.area_config("css_grid"))
                (pathlib.Path(directory) / "demo").mkdir()
                config = pathlib.Path(directory) / "demo" / "area.json"
                good = {
                    "schema_version": 1, "root": "wpt", "area": "html/x",
                    "selection": "directory", "dialog_elements": True,
                    "mutation_ir": True,
                }
                config.write_text(json.dumps(good))
                self.assertEqual(COVERAGE.area_config("demo"), good)
                with_fixtures = {**good, "fixtures": ["own.html"]}
                config.write_text(json.dumps(with_fixtures))
                self.assertEqual(COVERAGE.area_config("demo"), with_fixtures)
                for bad in [
                    # Files are never picked by hand: the directory is the rule.
                    {**good, "files": ["a.html"]},
                    {**good, "selection": ["a.html"]},
                    {**{k: v for k, v in good.items() if k != "selection"}},
                    {**good, "fixtures": ["b.html", "a.html"]},
                    {**good, "fixtures": []},
                    {**good, "fixtures": ["sub/own.html"]},
                    {**good, "root": "css"},
                    {**good, "extra": 1},
                ]:
                    config.write_text(json.dumps(bad))
                    with self.assertRaises(SystemExit):
                        COVERAGE.area_config("demo")
            finally:
                COVERAGE.COVERAGE_DIR = original

    def test_the_dialog_user_agent_guard_passes_on_the_engine_sheet(self):
        COVERAGE.dialog_user_agent_guard()

    @staticmethod
    def sheet(*rules):
        """A DIALOG_USER_AGENT_RULES block in dialog.rs's own layout."""
        body = "".join(
            f'    (\n        "{selector}",\n        concat!(\n'
            + "".join(f'            "{piece}",\n' for piece in pieces)
            + "        ),\n    ),\n"
            for selector, pieces in rules
        )
        return f"pub(crate) const DIALOG_USER_AGENT_RULES: &[(&str, &str)] = &[\n{body}];\n"

    def test_the_dialog_user_agent_guard_fails_on_drift(self):
        base = ("dialog", ["position: absolute; padding: 1em; ", "background-color: white; color: black"])
        hidden = ("dialog:not([open])", ["display: none"])
        COVERAGE.dialog_user_agent_guard(self.sheet(base, hidden))
        drifted = {
            "another element": self.sheet(base, hidden, ("div", ["display: block"])),
            "a changed inherited value": self.sheet(
                ("dialog", ["position: absolute; ", "color: red"]), hidden
            ),
            "a new inherited declaration": self.sheet(
                base, ("dialog:modal", ["font-size: 20px"])
            ),
            "no inherited declaration": self.sheet(("dialog", ["padding: 1em"]), hidden),
            "no rules": "pub(crate) const DIALOG_USER_AGENT_RULES: &[(&str, &str)] = &[\n];\n",
            "no block": "fn main() {}\n",
        }
        for case, source in drifted.items():
            with self.subTest(case), self.assertRaises(SystemExit):
                COVERAGE.dialog_user_agent_guard(source)


DIALOG_AREA = "html/semantics/interactive-elements/the-dialog-element"


class AreaGenerationTests(unittest.TestCase):
    def check(self, environment):
        """Run ``--check`` for html_dialog with the given environment."""
        env = {
            key: value for key, value in os.environ.items()
            if key not in ("CHROMIUM_WPT_ROOT", "CHROMIUM_ACORN")
        }
        return subprocess.run(
            [sys.executable, str(ROOT / "tools/wpt/port_coverage_area.py"), "--check",
             DIALOG_AREA, "html_dialog"],
            env={**env, **environment, "PYTHONUTF8": "1"},
            capture_output=True,
            text=True,
        )

    def test_check_fails_without_the_wpt_root(self):
        completed = self.check({})
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("CHROMIUM_WPT_ROOT must name", completed.stdout + completed.stderr)

    def test_check_fails_without_acorn(self):
        with tempfile.TemporaryDirectory() as root:
            (pathlib.Path(root) / DIALOG_AREA).mkdir(parents=True)
            completed = self.check({
                "CHROMIUM_WPT_ROOT": root,
                "CHROMIUM_ACORN": str(pathlib.Path(root) / "missing" / "acorn.mjs"),
            })
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(
            "pinned Chromium Acorn parser is missing", completed.stdout + completed.stderr
        )

    def area(self, root, files):
        base = pathlib.Path(root) / "html" / "x"
        for name, text in files.items():
            (base / name).parent.mkdir(parents=True, exist_ok=True)
            (base / name).write_text(text, encoding="utf-8")
        return base

    def test_every_file_is_ported_or_excluded_with_its_reason(self):
        with tempfile.TemporaryDirectory() as root:
            base = self.area(root, {
                "a.html": "", "b-ref.htm": "", "DIR_METADATA": "",
                "a-expected.txt": "", "c.window.js": "", "notes.css": "",
                "resources/frame.html": "", "support/x.png": "", "tentative/t.html": "",
            })
            ported = COVERAGE.area_files(base)
            self.assertEqual([path.name for path in ported], ["a.html", "b-ref.htm"])
            self.assertEqual(COVERAGE.excluded_files(base, ported), {
                "DIR_METADATA": "directory metadata, not a test",
                "a-expected.txt": "Chromium's expected testharness output, not a test",
                "c.window.js": "a JavaScript-only testharness test: no document to port",
                "notes.css": "not an HTML test document",
                "resources/frame.html": "in a resources/ directory: a support file, not a test",
                "support/x.png": "in a support/ directory: a support file, not a test",
                "tentative/t.html": "in a tentative/ directory: a support file, not a test",
            })

    def test_a_census_audit_test_is_never_lowered_again(self):
        sys.path.insert(0, str(ROOT))
        from tools.qualification import generate_renderer_contract as contract

        if not contract.ACORN.is_file():
            self.skipTest(f"pinned Acorn is not installed: {contract.ACORN}")
        with tempfile.TemporaryDirectory() as root:
            base = self.area(root, {
                "case.html": "<div id=a></div><script>a.remove()</script>",
            })
            audit = pathlib.Path(root) / "audit-v3.json"
            audit.write_text(json.dumps({"entries": [{"test_id": "wpt/demo/case"}]}))
            with mock.patch.dict(os.environ, {"CHROMIUM_WPT_ROOT": root}), \
                    mock.patch.object(PORT_WPT, "WPT_SOURCE_ROOT", pathlib.Path(root)), \
                    mock.patch.object(PORT_WPT, "JAVASCRIPT_MUTATION_AUDIT", audit):
                with self.assertRaisesRegex(SystemExit, "repeats census audit tests: wpt/demo/case"):
                    COVERAGE.area_mutation_ir(base, [base / "case.html"], "demo", "html/x")
                audit.write_text(json.dumps({"entries": [{"test_id": "wpt/other/case"}]}))
                ir = COVERAGE.area_mutation_ir(base, [base / "case.html"], "demo", "html/x")
        self.assertEqual(list(ir["entries"]), ["wpt/demo/case"])



if __name__ == "__main__":
    unittest.main()
