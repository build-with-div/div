"""Tests for the RFC 0005 front end, capability classification and ledger."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_chromium_subtests as capture  # noqa: E402
import ledger  # noqa: E402

ACORN = os.environ.get("CHROMIUM_ACORN")


class ExclusionTest(unittest.TestCase):
    def test_pages_that_are_not_run_are_named(self):
        self.assertEqual(capture.exclusion("a/cancel-handle-manual.html"), "manual")
        self.assertEqual(capture.exclusion("a/devicepixel-ref.html"), "reference")
        self.assertEqual(capture.exclusion("a/x.sub.html"), "wptserve-substitution")
        self.assertEqual(capture.exclusion("a/x.https.html"), "https")
        self.assertEqual(capture.exclusion("a/idlharness.window.js"), "wptserve-generated-wrapper")
        self.assertEqual(capture.exclusion("a/idlharness.html"), "idlharness")
        self.assertEqual(capture.exclusion("a/idlharness-2.html"), "idlharness")
        self.assertEqual(capture.exclusion("a/resources/helper.js"), "helper")
        self.assertEqual(capture.exclusion("a/resources/iframe.html"), "helper")
        self.assertEqual(capture.exclusion("a/x-expected.txt"), "helper")
        self.assertIsNone(capture.exclusion("a/observe.html"))
        self.assertIsNone(capture.exclusion("a/svg-with-css-box-002.svg"))


class PageTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        (self.root / "suite/resources").mkdir(parents=True)
        self.helper = self.root / "suite/resources/helper.js"
        self.helper.write_text("function helperMade() {}\n", encoding="utf-8")
        digest = hashlib.sha256(self.helper.read_bytes()).hexdigest()
        self.intrinsics = {"suite/resources/helper.js": {
            "sha256": digest, "provides": {"helperMade": ["document.body"]}}}

    def page(self, name, text):
        (self.root / "suite" / name).write_text(text, encoding="utf-8")
        return ledger.read_page(self.root, f"suite/{name}", self.intrinsics)

    def test_resolve_handles_absolute_and_relative_urls(self):
        self.assertEqual(ledger.resolve("/resources/a.js?x", "s/p.html"), "resources/a.js")
        self.assertEqual(ledger.resolve("./resources/a.js", "s/p.html"), "s/resources/a.js")
        self.assertEqual(ledger.resolve("../common/a.js", "s/t/p.html"), "s/common/a.js")

    def test_scripts_helpers_and_title_are_read(self):
        page = self.page("p.html", """<!doctype html><title> A  &amp; B </title>
<script src="/resources/testharness.js"></script>
<script src="./resources/helper.js"></script>
<script>test(() => {}, "one");</script>""")
        self.assertEqual(page["title"], "A & B")
        self.assertEqual(page["helpers"], ["suite/resources/helper.js"])
        self.assertEqual(len(page["scripts"]), 1)
        self.assertIn('"one"', page["scripts"][0]["source"])
        self.assertEqual(page["problems"], [])

    def test_xhtml_self_closing_scripts_and_cdata_are_read(self):
        page = self.page("p.svg", """<svg xmlns:h="http://www.w3.org/1999/xhtml">
<h:script src="./resources/helper.js"/>
<script><![CDATA[ test(() => {}, "a < b"); ]]></script></svg>""")
        self.assertEqual(page["helpers"], ["suite/resources/helper.js"])
        self.assertEqual(page["scripts"][0]["source"].strip(), 'test(() => {}, "a < b");')

    def test_unknown_and_changed_helpers_are_problems(self):
        page = self.page("p.html", '<script src="/common/other.js"></script>')
        self.assertEqual(page["problems"], ["unrecognised-helper:common/other.js"])
        self.helper.write_text("function helperMade() { changed(); }\n", encoding="utf-8")
        page = self.page("q.html", '<script src="resources/helper.js"></script>')
        self.assertEqual(page["problems"], ["changed-helper:suite/resources/helper.js"])

    def test_markup_surfaces_cover_tags_styles_and_sheets(self):
        page = self.page("p.html", """<html><head><link rel="stylesheet" href="/fonts/ahem.css">
<style>div { color: red }</style></head><body>
<!-- <section> in a comment --><div style="width: 1px"><span>x</span></div>
<svg><circle r="1"/></svg><script>let s = "<table>";</script></body></html>""")
        self.assertEqual(page["markup"], [
            "css:color", "css:width", "markup:<style>", "markup:circle", "markup:div",
            "markup:span", "markup:style-attribute", "markup:stylesheet:fonts/ahem.css",
            "markup:svg"])

    def test_style_attributes_and_event_handler_attributes_are_page_surfaces(self):
        text = """<body onload="run()"><div style='background: red; Width:1px'
onclick="go(&quot;x&quot;)"></div></body>"""
        self.assertEqual(ledger.markup_surfaces(text, "p.html"), [
            "css:background", "css:width", "markup:div", "markup:onclick", "markup:onload",
            "markup:style-attribute"])
        self.assertEqual(ledger.markup_handlers(text), ["run()", 'go("x")'])


class CssSurfaceTest(unittest.TestCase):
    def test_a_sheet_reports_each_declared_property(self):
        surfaces = ledger.markup_surfaces("""<style>
/* margin: 0 */
a:hover { COLOR: red; background: url(x:y) }
@layer base { b { width: 1px } }
</style>""", "s/p.html")
        self.assertEqual(surfaces, ["css:background", "css:color", "css:width", "markup:<style>"])

    def test_animation_at_rules_are_surfaces(self):
        surfaces = ledger.markup_surfaces("""<style>
/* @starting-style in a comment */
@KEYFRAMES spin { from { width: 1px } }
</style>""", "s/p.html")
        self.assertEqual(surfaces, ["css:width", "markup:<style>", "markup:@keyframes"])


class RealInputOracleTest(unittest.TestCase):
    capture = ledger.capture

    def result(self, *rows, harness="OK", **extra):
        return {"harness": harness, "subtests": [
            {"name": name, "status": status, **({"message": message} if message else {})}
            for name, status, message in rows], **extra}

    def test_each_variant_is_a_run(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        page = directory / "p.html"
        page.write_text("""<meta name="variant" content="?mouse">
<meta name=variant content='?a=b&c'>""")
        self.assertEqual(self.capture.page_variants(page), ["?mouse", "?a=b&c"])
        page.write_text("<title>none</title>")
        self.assertEqual(self.capture.page_variants(page), [""])

    def test_expectations_keep_what_applies_on_linux(self):
        text = """# external/wpt/s/comment.html [ Failure ]
crbug.com/1 [ Win ] external/wpt/s/win.html [ Failure ]
[ Mac14 ] external/wpt/s/mac.html [ Timeout ]
[ Linux ] external/wpt/s/linux.html?x [ Failure Pass ]
[ Release ] external/wpt/s/release.html [ Timeout ]
virtual/v/external/wpt/s/virtual.html [ Failure ]
external/wpt/s/pass.html [ Pass ]
external/wpt/other/o.html [ Failure ]
external/wpt/s/dir/* [ Timeout ]
"""
        self.assertEqual(self.capture.parse_expectations(text, "s"), {
            "s/linux.html?x": "Failure", "s/release.html": "Timeout", "s/dir/*": "Timeout"})

    def test_an_expectation_matches_the_run_its_page_or_a_wildcard(self):
        entries = {"s/a.html?x": "Failure", "s/b.html": "Timeout", "s/dir/*": "Crash"}
        find = self.capture.expectation_for
        self.assertEqual(find("s/a.html?x", entries), "Failure")
        self.assertIsNone(find("s/a.html?y", entries))
        self.assertEqual(find("s/b.html?y", entries), "Timeout")
        self.assertEqual(find("s/dir/c.html", entries), "Crash")

    def test_a_baseline_lists_non_passing_subtests_and_harness_errors(self):
        text = """This is a testharness.js-based test.
[FAIL] one
  assert_true
[TIMEOUT] two
Harness: the test ran to completion.
"""
        self.assertEqual(self.capture.parse_baseline(text),
                         ({"one": "FAIL", "two": "TIMEOUT"}, False))
        self.assertTrue(self.capture.parse_baseline(
            "Harness Error. harness_status.status = 1 , harness_status.message = x")[1])

    def test_a_variants_baseline_is_named_after_its_query(self):
        root = Path("wpt")
        self.assertEqual(self.capture.baseline_path(root, "s/p.html?a=b&c"),
                         root / "s/p_a=b_c-expected.txt")
        self.assertEqual(self.capture.baseline_path(root, "s/p.https.html"),
                         root / "s/p.https-expected.txt")

    def test_a_run_agrees_only_when_it_reproduces_the_record(self):
        status = self.capture.oracle_status
        agrees = {"status": "agrees"}
        self.assertEqual(status(self.result(("a", "PASS", None)), None, None), agrees)
        self.assertEqual(status(self.result(("a", "FAIL", "m")), ({"a": "FAIL"}, False), None),
                         agrees)
        self.assertEqual(status(self.result(("a", "PASS", None), harness="ERROR"),
                                ({}, True), None), agrees)
        mismatch = {"status": "disputed", "reason": "baseline-mismatch"}
        # A failure Chromium doesn't record, a pass it records as failing, a
        # recorded subtest that's missing, and a harness error on either side.
        self.assertEqual(status(self.result(("a", "FAIL", "m")), None, None), mismatch)
        self.assertEqual(status(self.result(("a", "PASS", None)), ({"a": "FAIL"}, False), None),
                         mismatch)
        self.assertEqual(status(self.result(("a", "PASS", None)), ({"b": "FAIL"}, False), None),
                         mismatch)
        self.assertEqual(status(self.result(("a", "PASS", None), harness="TIMEOUT"), None, None),
                         mismatch)
        self.assertEqual(status(self.result(("a", "PASS", None)), ({}, True), None), mismatch)

    def test_a_disputed_run_names_its_reason(self):
        status = self.capture.oracle_status
        self.assertEqual(status(self.result(("a", "PASS", None)), None, "Failure")["reason"],
                         "test-expectations:Failure")
        self.assertEqual(status(self.result(("a", "PASS", None), unstable=True), None,
                                None)["reason"], "unstable")
        self.assertEqual(status(self.result(("a", "FAIL", "Error: No eventSender")), None,
                                None)["reason"], "driver:keyboard")

    def test_repeats_compare_names_and_statuses_not_messages(self):
        same = self.capture.same_result
        one = self.result(("a", "FAIL", "at 10"))
        self.assertTrue(same(one, self.result(("a", "FAIL", "at 11"))))
        self.assertFalse(same(one, self.result(("a", "PASS", None))))
        self.assertFalse(same(one, self.result(("a", "FAIL", "at 10"), harness="ERROR")))

    def test_a_name_computed_differently_in_a_repeat_makes_the_run_disputed(self):
        # RFC 0005: a computed name must be the same in all three repeats, or
        # the run is disputed as `unstable` and none of its rows is generated.
        first = self.result(("case at 16ms", "PASS", None))
        renamed = self.result(("case at 17ms", "PASS", None))
        self.assertFalse(self.capture.same_result(first, renamed))
        self.assertEqual(self.capture.oracle_status({**first, "unstable": True}, None, None),
                         {"status": "disputed", "reason": "unstable"})


class CssCapabilityTest(unittest.TestCase):
    def test_the_property_names_come_from_from_css_name(self):
        source = """fn other() { "skipped" => Some(Self::Skipped), }
pub fn from_css_name(name: &str) -> Option<Self> {
    match name {
        "width" => Some(Self::Width),
        "box-sizing" => Some(Self::BoxSizing),
        _ => None,
    }
}
"after" => Some(Self::After),"""
        self.assertEqual(ledger.div_css_properties(source), {"width", "box-sizing"})

    def test_the_real_table_has_the_properties_the_ledger_relies_on(self):
        known = ledger.div_css_properties(ledger.PROPERTY_TABLE.read_text(encoding="utf-8"))
        self.assertIn("width", known)
        self.assertIn("overflow", known)

    def test_a_shorthand_that_lands_fails_until_its_row_is_updated(self):
        capabilities = {"_comment": "x", "css:width": {"feature": "S1", "divui": "Width"},
                        "css:background": {"feature": "gap:background-shorthand", "divui": None}}
        ledger.check_css_capabilities(capabilities, {"width"})
        with self.assertRaisesRegex(SystemExit, "css:background"):
            ledger.check_css_capabilities(capabilities, {"width", "background"})
        with self.assertRaisesRegex(SystemExit, "css:width"):
            ledger.check_css_capabilities(capabilities, set())


class ClassifyTest(unittest.TestCase):
    CAPABILITIES = {
        "harness:test": {"feature": "T3", "divui": "runtime"},
        "requestAnimationFrame()": {"feature": "R1", "divui": "Document::request_animation_frame"},
        "new IntersectionObserver()": {"feature": "O2", "divui": None},
    }

    def test_every_surface_mapped_to_a_call_is_translatable(self):
        self.assertEqual(
            ledger.classify(["harness:test", "requestAnimationFrame()"], self.CAPABILITIES),
            {"status": "translatable"})

    def test_a_missing_call_or_an_unmapped_surface_blocks(self):
        self.assertEqual(
            ledger.classify(["new IntersectionObserver()", "harness:test"], self.CAPABILITIES),
            {"status": "blocked", "features": ["O2"]})
        self.assertEqual(
            ledger.classify(["document.foo()"], self.CAPABILITIES),
            {"status": "blocked", "features": [], "unmapped": ["document.foo()"]})

    def test_helper_methods_are_harness_and_helper_functions_add_their_surfaces(self):
        page = {"helpers": ["h.js"]}
        intrinsics = {"h.js": {"provides": {"Helper": ["new ResizeObserver()"]},
                               "methods": ["start"]}}
        record = {"declared_by": "harness:Helper", "surfaces": [".start()", ".observe()"]}
        self.assertEqual(ledger.subtest_surfaces(record, page, intrinsics),
                         [".observe()", "harness:.start()", "new ResizeObserver()"])

    def test_a_chain_stops_at_the_first_link_that_cannot_run(self):
        capabilities = {**self.CAPABILITIES, "harness:t.done": {"feature": "T3", "divui": "x"}}

        def link(label, surfaces, subtests=(), finishes=(), reasons=()):
            return {"label": label, "surfaces": list(surfaces), "reasons": list(reasons),
                    "subtests": list(subtests), "finishes": list(finishes)}
        chain = [link("a", ["harness:test"], ["a"]),
                 link("b", ["new IntersectionObserver()"], ["b"]),
                 link("c", ["harness:test"], ["c"]),
                 link("link3", ["harness:t.done"], finishes=["guard"])]
        self.assertEqual(
            ledger.chain_stops({"chains": [chain]}, {"helpers": []}, {}, capabilities),
            {"c": "b", "guard": "b"})
        chain[1] = link("b", ["harness:test"], ["b"], reasons=["dynamic-code"])
        self.assertEqual(
            ledger.chain_stops({"chains": [chain]}, {"helpers": []}, {}, capabilities),
            {"c": "b", "guard": "b"})
        self.assertEqual(ledger.chain_stops({}, {"helpers": []}, {}, capabilities), {})

    def test_the_classifier_and_the_recorded_run_must_agree(self):
        ledger.check_reachability("p", "a", "translatable", "PASS")
        ledger.check_reachability("p", "a", "translatable", "FAIL")
        ledger.check_reachability("p", "a", "unreachable", "NOTRUN")
        # A row behind a stopped chain that starts running.
        with self.assertRaisesRegex(SystemExit, "unreachable but its recorded Div result is PASS"):
            ledger.check_reachability("p", "a", "unreachable", "PASS")
        # A row the classifier calls reachable that did not run.
        with self.assertRaisesRegex(SystemExit, "translatable but its recorded"):
            ledger.check_reachability("p", "a", "translatable", "NOTRUN")

    def test_a_computed_name_needs_every_surface_on_its_page(self):
        page = {"helpers": [], "markup": ["markup:div"]}
        capabilities = {**self.CAPABILITIES, "markup:div": {"feature": "DOM", "divui": "x"}}
        dynamic = [{"name": None, "untranslatable": ["runtime-subtest-name"]}]
        whole = {"surfaces": ["harness:test", "requestAnimationFrame()"], "reasons": []}
        self.assertEqual(ledger.classify_computed({"whole_page": whole}, dynamic, page, {},
                                                  capabilities), {"status": "translatable"})
        # One surface anywhere on the page that has no call blocks it.
        whole = {"surfaces": [*whole["surfaces"], "new IntersectionObserver()"], "reasons": []}
        self.assertEqual(ledger.classify_computed({"whole_page": whole}, dynamic, page, {},
                                                  capabilities),
                         {"status": "blocked", "features": ["O2"]})
        # So does the markup.
        self.assertEqual(ledger.classify_computed(
            {"whole_page": {"surfaces": [], "reasons": []}}, dynamic,
            {**page, "markup": ["markup:svg"]}, {}, capabilities)["unmapped"], ["markup:svg"])
        # Anything outside the subset on the page makes it untranslatable.
        self.assertEqual(ledger.classify_computed(
            {"whole_page": {"surfaces": [], "reasons": ["dynamic-code"]}}, dynamic, page, {},
            capabilities), {"status": "untranslatable", "reasons": ["dynamic-code"]})
        # A name no declaration on the page can produce.
        self.assertEqual(ledger.classify_computed({"whole_page": whole}, [], page, {},
                                                  capabilities),
                         {"status": "untranslatable", "reasons": ["undeclared-subtest"]})

    def test_a_computed_name_page_reports_exactly_the_recorded_names(self):
        problem = ledger.name_problem
        self.assertIsNone(problem(["b", "a"], ["a", "b"], False))
        self.assertRegex(problem(["a", "b", "c"], ["a", "b"], False), "did not record: \\['c'\\]")
        self.assertRegex(problem(["a"], ["a", "b"], False), "differ")
        self.assertRegex(problem(["a", "x"], ["a", "b"], False), "did not record")
        self.assertRegex(problem(["a", "a"], ["a"], False), "differ")
        # A run the translation stopped may declare fewer, never another.
        self.assertIsNone(problem(["a"], ["a", "b"], True))
        self.assertRegex(problem(["x"], ["a", "b"], True), "did not record")

    def test_an_unreachable_row_needs_the_runs_stop_reason(self):
        ledger.check_stopped("p", "a", "unreachable", "surface:Image")
        ledger.check_stopped("p", "a", "translatable", None)
        with self.assertRaisesRegex(SystemExit, "names no reason"):
            ledger.check_stopped("p", "a", "unreachable", None)


@unittest.skipUnless(ACORN, "CHROMIUM_ACORN is not set")
class FrontEndTest(unittest.TestCase):
    def analyse(self, source, title="Page title", helpers=()):
        payload = [{"file": "p.html", "title": title,
                    "scripts": [{"source": source, "offset": 0}], "helpers": list(helpers)}]
        result = subprocess.run(["node", str(ledger.TOOL / "translate.mjs"), ACORN],
                                input=json.dumps(payload), capture_output=True, text=True,
                                encoding="utf-8", check=True)
        return json.loads(result.stdout)[0]

    def names(self, record):
        return [(s["name"], s["name_known"]) for s in record["subtests"]]

    def test_harness_declarations_and_their_names(self):
        record = self.analyse("""
test(() => {}, "sync");
async_test(t => t.done(), "async " + "joined");
async_test("named first");
promise_test(async () => {}, `template`);
test(() => {});""")
        self.assertEqual(self.names(record), [
            ("sync", True), ("async joined", True), ("named first", True),
            ("template", True), ("Page title", True)])

    def test_a_page_without_declarations_is_one_subtest_named_by_its_title(self):
        record = self.analyse("requestAnimationFrame(() => done());")
        self.assertEqual(self.names(record), [("Page title", True)])
        self.assertEqual(record["subtests"][0]["declared_by"], "harness:single-page")
        self.assertIn("requestAnimationFrame()", record["subtests"][0]["surfaces"])

    def test_the_whole_page_includes_handlers_and_functions_passed_by_name(self):
        payload = [{"file": "p.html", "title": "t", "helpers": [], "handlers": ["run()"],
                    "scripts": [{"source": """
var t = async_test("named");
function run() { requestAnimationFrame(later); }
function later() { new IntersectionObserver(); t.done(); }
for (const n of [1]) test(() => {}, `case ${n}`);""", "offset": 0}]}]
        result = subprocess.run(["node", str(ledger.TOOL / "translate.mjs"), ACORN],
                                input=json.dumps(payload), capture_output=True, text=True,
                                encoding="utf-8", check=True)
        record = json.loads(result.stdout)[0]
        # `<body onload="run()">` runs `run`, which passes `later` by name.
        self.assertIn("new IntersectionObserver()", record["subtests"][0]["surfaces"])
        self.assertIn("new IntersectionObserver()", record["whole_page"]["surfaces"])
        self.assertEqual(record["whole_page"]["reasons"], [])

    def test_names_built_at_run_time_are_untranslatable(self):
        record = self.analyse("""
for (const n of [1, 2]) test(() => {}, `case ${n}`);
const label = location.hash; test(() => {}, label);""")
        self.assertEqual([s["untranslatable"] for s in record["subtests"]],
                         [["runtime-subtest-name"], ["runtime-subtest-name"]])

    def test_a_local_named_test_does_not_hide_declarations(self):
        record = self.analyse("""
function run() { let test = async_test("inner"); test.step(() => {}); }
function other() { test(() => {}, "outer"); }
run(); other();""")
        self.assertEqual(self.names(record), [("inner", True), ("outer", True)])

    def test_subtests_do_not_take_each_others_surfaces(self):
        record = self.analyse("""
function test0() {
  test(() => { new ResizeObserver(() => {}); }, "zero");
  return Promise.resolve();
}
function test1() {
  test(() => { new IntersectionObserver(() => {}); }, "one");
  return Promise.resolve();
}
function shared() { return document.body; }
test(() => { assert_true(!!window.ResizeObserver); }, "implemented");
test0().then(() => { shared(); return test1(); });""")
        surfaces = {s["name"]: s["surfaces"] for s in record["subtests"]}
        self.assertIn("new ResizeObserver()", surfaces["zero"])
        self.assertNotIn("new IntersectionObserver()", surfaces["zero"])
        self.assertIn("new IntersectionObserver()", surfaces["one"])
        self.assertNotIn("new ResizeObserver()", surfaces["one"])
        # Shared page-level code reaches every subtest; another subtest's body does not.
        self.assertIn("document.body", surfaces["zero"])
        self.assertNotIn("window.ResizeObserver", surfaces["zero"])

    def test_dynamic_code_is_untranslatable(self):
        record = self.analyse('test(() => { eval("1"); }, "e");')
        self.assertEqual(record["subtests"][0]["untranslatable"], ["dynamic-code"])

    def test_helper_declarations_use_the_helper_name_argument(self):
        record = self.analyse("""
function test0() { let h = new ResizeTestHelper("helped", []); return h.start(); }
test0();""", helpers=["ResizeTestHelper"])
        self.assertEqual(self.names(record), [("helped", True)])
        self.assertEqual(record["subtests"][0]["declared_by"], "harness:ResizeTestHelper")

    def test_a_chain_reports_each_link_without_synchronous_test_bodies(self):
        record = self.analyse("""
let guard;
test(() => { guard = async_test("guard"); }, "setup");
function a() {
  test(() => { new IntersectionObserver(() => {}); }, "a");
  return Promise.resolve();
}
function b() { const image = new Image(); return Promise.resolve(); }
function c() { test(() => {}, "c"); return Promise.resolve(); }
a().then(() => b()).then(() => c()).then(() => guard.done());""")
        self.assertEqual(len(record["chains"]), 1)
        chain = record["chains"][0]
        self.assertEqual([link["label"] for link in chain], ["a", "b", "c", "link3"])
        self.assertNotIn("new IntersectionObserver()", chain[0]["surfaces"])
        self.assertEqual(chain[0]["subtests"], ["a"])
        self.assertIn("new Image()", chain[1]["surfaces"])
        self.assertEqual(chain[2]["subtests"], ["c"])
        self.assertEqual(chain[3]["finishes"], ["guard"])

    def test_a_chain_with_a_rejection_handler_is_not_analysed(self):
        record = self.analyse("""
function a() { return Promise.resolve(); }
function b() {}
a().then(() => b(), () => b());""")
        self.assertEqual(record["chains"], [])

    def test_a_parse_error_is_reported(self):
        record = self.analyse("test(() => {, 'x');")
        self.assertIn("parse_error", record)


@unittest.skipUnless(ACORN, "CHROMIUM_ACORN is not set")
class HelperAnalysisTest(unittest.TestCase):
    def expand(self, files, capabilities=None, wrong_hash=()):
        root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        intrinsics = {}
        for name, source in files.items():
            (root / name).write_bytes(source.encode("utf-8"))
            digest = hashlib.sha256(source.encode("utf-8")).hexdigest()
            intrinsics[name] = {"sha256": "0" * 64 if name in wrong_hash else digest,
                                "analyse": True, "provides": {}}
        capabilities = capabilities if capabilities is not None else {
            "getComputedStyle()": {"feature": "gap:computed-style", "divui": None}}
        ledger.expand_intrinsics(intrinsics, capabilities, root, ACORN)
        return intrinsics, capabilities

    def test_a_wrapped_helper_exposes_functions_with_their_named_surfaces(self):
        intrinsics, capabilities = self.expand({"a.js": """(function (root) {
  function hidden() { return getComputedStyle(document.body); }
  root.exposed = function () { hidden(); };
  root.alias = hidden;
  root.declaring = function () { test(() => {}, "made"); };
})(window);"""})
        entry = intrinsics["a.js"]
        self.assertEqual(entry["provides"]["exposed"], ["getComputedStyle()"])
        self.assertEqual(entry["provides"]["alias"], ["getComputedStyle()"])
        self.assertEqual(entry["declares"], ["declaring"])
        self.assertFalse(entry["page_declares"])
        self.assertEqual(entry["page"], ["helper:a.js"])
        self.assertEqual(capabilities["helper:a.js"]["feature"], "T3:helper-intrinsic")
        self.assertIsNone(capabilities["harness:exposed"]["divui"])

    def test_a_call_into_another_helper_takes_its_surfaces(self):
        intrinsics, _ = self.expand({
            "a.js": "function first() { return getComputedStyle(document.body); }",
            "b.js": "function second(el) { first(); return el.privateField; }"})
        self.assertEqual(intrinsics["b.js"]["provides"]["second"], ["getComputedStyle()"])

    def test_a_helper_that_declares_subtests_when_it_loads_counts(self):
        intrinsics, _ = self.expand({"a.js": 'test(() => {}, "on load");'})
        self.assertTrue(intrinsics["a.js"]["page_declares"])
        record = {"whole_page": {"surfaces": []}}
        self.assertTrue(ledger.helper_declared(record, {"helpers": ["a.js"]}, intrinsics))

    def test_a_page_calling_a_declaring_helper_function_counts(self):
        intrinsics, _ = self.expand({"a.js": 'function make() { test(() => {}, "x"); }'})
        page = {"helpers": ["a.js"]}
        self.assertTrue(ledger.helper_declared(
            {"whole_page": {"surfaces": ["harness:make"]}}, page, intrinsics))
        self.assertFalse(ledger.helper_declared(
            {"whole_page": {"surfaces": []}}, page, intrinsics))

    def test_a_helper_page_stays_blocked_on_the_intrinsic(self):
        intrinsics, capabilities = self.expand({"a.js": "function plain() {}"})
        page = {"helpers": ["a.js"]}
        surfaces = ledger.subtest_surfaces(
            {"surfaces": ["harness:plain"], "declared_by": "harness:test"}, page, intrinsics)
        capabilities["harness:test"] = {"feature": "T3", "divui": "runtime"}
        row = ledger.classify(surfaces, capabilities)
        self.assertEqual(row, {"status": "blocked", "features": ["T3:helper-intrinsic"]})

    def test_a_helper_using_dynamic_code_needs_review(self):
        with self.assertRaisesRegex(SystemExit, "review this helper"):
            self.expand({"a.js": "function run() { eval('1'); }"})

    def test_a_helper_with_another_hash_is_not_analysed(self):
        intrinsics, _ = self.expand({"a.js": "function plain() {}"}, wrong_hash={"a.js"})
        self.assertNotIn("page", intrinsics["a.js"])


class RecordingTest(unittest.TestCase):
    def test_a_stale_recording_fails(self):
        root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        (root / "s").mkdir()
        (root / "s/new.html").write_text("<script src=/resources/testharness.js></script>",
                                         encoding="utf-8")
        recordings = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recordings)
        (recordings / "s.json").write_text(json.dumps({
            "chromium": "147", "wpt_revision": "x", "suite": "s", "files": {}}), encoding="utf-8")
        with patch.object(ledger, "CHROMIUM", recordings):
            with self.assertRaisesRegex(SystemExit, "stale"):
                ledger.build("s", root, ACORN or "acorn", {}, {})


if __name__ == "__main__":
    unittest.main()
