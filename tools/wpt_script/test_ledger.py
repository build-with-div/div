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
            "markup:<style>", "markup:circle", "markup:div", "markup:span",
            "markup:style-attribute", "markup:stylesheet:fonts/ahem.css", "markup:svg"])


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

    def test_a_parse_error_is_reported(self):
        record = self.analyse("test(() => {, 'x');")
        self.assertIn("parse_error", record)


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
