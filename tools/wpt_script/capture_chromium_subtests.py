#!/usr/bin/env python3
"""Record every testharness subtest of a WPT suite as pinned Chromium runs it.

RFC 0005 counts results against the upstream subtests. Many subtest names
are computed while the test runs (loops, helper functions, the document
title as the default name), so a static reading cannot list them. This tool
loads each test in the pinned Chromium 147, headless, and records the name,
status and message of every subtest the harness reports. The ledger
(`ledger.py`) uses the recording as each suite's denominator and as
Chromium's own result.

The WPT files come from the pinned Chromium checkout at 09d377d9
(`--wpt-root`, for example a `git archive` of
`third_party/blink/web_tests/external/wpt`). A small local server serves
them, with one change: `/resources/testharnessreport.js` is replaced by a
reporter that stores the harness's completion results on the page, as
wptrunner does. Files that need wptserve features (`.sub.` substitution,
generated `.any.js`/`.window.js` wrappers, `.headers`) or a person
(`-manual`) are recorded as excluded with their reason, not run.

A suite in `ORACLE_SUITES` runs with the switches of Chromium's web test
runner and is checked against Chromium's own pinned record. It also gets the
test driver Chromium's web test server serves (`DRIVER_FILES`, from the same
checkout). Its vendor file sends pointer actions through
`chrome.gpuBenchmarking.pointerActionSequence`, so the browser starts with
`--enable-gpu-benchmarking`. `--wpt-root` must then sit in a tree that keeps
Chromium's paths.

This isn't content_shell, so such a run counts as Chromium's result only
where it reproduces Chromium's own pinned record. Each run gets an `oracle`:
`agrees`, or `disputed` with the reason (a `TestExpectations` entry, results
that change between repeats, a keyboard action the vendor file needs
`eventSender` for, or a mismatch with the `-expected.txt` baseline).

Usage (Linux; the pinned build from `run_all_pixel_comparisons.py`):

    python3 tools/wpt_script/capture_chromium_subtests.py \
        --wpt-root out/wpt-script/09d377d9/third_party/blink/web_tests/external/wpt \
        --suite resize-observer
"""

from __future__ import annotations

import argparse
import functools
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
HARNESS = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
FIXTURES = ROOT / "tests/wpt-script/chromium"
CHROMIUM_VERSION = "147.0.7727.50"
WPT_REVISION = "09d377d9"
# testharness.js waits 10 s for a "normal" test, 60 s for a "long" one.
PAGE_LIMIT_SECONDS = 75.0

REPORTER = b"""\
add_completion_callback(function (tests, status) {
  window.__divResults = {
    harness: status.status,
    message: status.message,
    tests: tests.map(function (t) {
      return {name: t.name, status: t.status, message: t.message};
    }),
  };
});
"""

# wpt/config.tmpl.json's aliases, with wptserve.py's vendor override.
DRIVER_FILES = {
    "/resources/testdriver.js": (
        "third_party/wpt_tools/wpt/resources/testdriver.js",
        "a74f264b3bafe6db9ad1e6b1f0ca2a2269a7d61b25a59df8e2ccf9f01c5fdf31"),
    "/resources/testdriver-actions.js": (
        "third_party/wpt_tools/wpt/resources/testdriver-actions.js",
        "e2db9153f3850983f423383d004042845b87d2a3a4d388b3f4846c915f571027"),
    "/resources/testdriver-vendor.js": (
        "third_party/blink/web_tests/resources/testdriver-vendor.js",
        "d84f8fcdaf4a2d3fbfd2f6951268345a2fb813637fe733a56421e282b370edf0"),
}
ORACLE_SUITES = {"pointerevents", "css/css-transitions", "css/css-animations", "css/css-easing",
                 "web-animations"}
# The switches of content/web_test's runner that decide these tests' results,
# so a run can be compared with Chromium's pinned baselines. The vendor file
# also stands down under WebDriver (`navigator.webdriver`), which a debugging
# port sets and content_shell, its usual host, doesn't.
REAL_INPUT_FLAGS = ["--enable-gpu-benchmarking",
                    "--enable-experimental-web-platform-features",
                    "--enable-blink-test-features",
                    "--touch-events=enabled",
                    "--force-device-scale-factor=1",
                    "--disable-blink-features=AutomationControlled",
                    # An 800x600 viewport, as content_shell has: the headless
                    # window's size includes 143 px of browser UI.
                    "--window-size=800,743"]

VARIANT = re.compile(r"""<meta\s+name=["']?variant["']?\s+content=["']([^"']*)["']""", re.I)

EXPECTATIONS = ("third_party/blink/web_tests/TestExpectations",
                "180c921c3ec73b32cc35d44c7755b92ac97ed4e73c8e65baf0e7cea51fc9ca84")
EXPECTATION = re.compile(r"(?:^|\s)external/wpt/(\S+)\s+\[ ([^\]]+) \]")
EXPECTATION_TAGS = re.compile(r"\[ ([^\]]+) \]\s+external/wpt/")
OTHER_PLATFORMS = ("Android", "Fuchsia", "Mac", "Win", "iOS")
BASELINE_ROW = re.compile(r"\[(FAIL|TIMEOUT|NOTRUN|PRECONDITION_FAILED)\] (.*)")
KEYBOARD_DRIVER = ("No eventSender", "We do not support keydown")
# A run that changes between these repeats can't stand for Chromium's result.
INPUT_REPEATS = 3

TEST_STATUS = {0: "PASS", 1: "FAIL", 2: "TIMEOUT", 3: "NOTRUN", 4: "PRECONDITION_FAILED"}
HARNESS_STATUS = {0: "OK", 1: "ERROR", 2: "TIMEOUT", 3: "PRECONDITION_FAILED"}


def exclusion(relative: str) -> str | None:
    """Why a file in the suite is not run, or None if it is a test page."""
    name = relative.rsplit("/", 1)[-1]
    parts = relative.split("/")
    if any(part in ("resources", "support") for part in parts[:-1]):
        return "helper"
    if not name.endswith((".html", ".htm", ".xhtml", ".svg", ".js")):
        return "helper"
    if name.endswith(".js"):
        if name.endswith((".any.js", ".window.js", ".worker.js")):
            return "wptserve-generated-wrapper"
        return "helper"
    stem = name.rsplit(".", 1)[0]
    if stem.endswith(("-ref", "-notref")) or "-ref-" in name or name.startswith("ref-"):
        return "reference"
    if "-manual" in name:
        return "manual"
    if ".sub." in name:
        return "wptserve-substitution"
    if ".https." in name:
        return "https"
    # RFC 0005 section 4: it needs wptserve's IDL files and parser alias.
    if stem == "idlharness" or stem.startswith("idlharness-"):
        return "idlharness"
    return None


def page_variants(path: Path) -> list[str]:
    """The page's `<meta name=variant>` queries, or the bare page; wptrunner
    runs each as its own test."""
    text = path.read_text(encoding="utf-8", errors="replace")
    return VARIANT.findall(text) or [""]


def is_testharness(path: Path) -> bool:
    text = path.read_text(encoding="utf-8", errors="replace")
    return "/resources/testharness.js" in text


def driver_files(wpt_root: Path) -> dict[str, bytes]:
    """The pinned test driver, by URL path."""
    checkout = wpt_root.parents[4]
    files = {}
    for url, (relative, digest) in DRIVER_FILES.items():
        path = checkout / relative
        if not path.is_file():
            raise SystemExit(f"{path} is missing: extract it from the pinned checkout")
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit(f"{relative} is not the pinned file")
        files[url] = data
    return files


def parse_expectations(text: str, suite: str) -> dict[str, str]:
    """`TestExpectations` entries for `suite` that apply on Linux."""
    entries: dict[str, str] = {}
    for line in text.splitlines():
        line = line.strip()
        match = EXPECTATION.search(line)
        if line.startswith("#") or not match or not match.group(1).startswith(suite + "/"):
            continue
        tags = EXPECTATION_TAGS.search(line)
        names = tags.group(1).split() if tags else []
        if "Linux" not in names and any(name.startswith(OTHER_PLATFORMS) for name in names):
            continue
        values = set(match.group(2).split()) - {"Pass", "Slow"}
        if values:
            entries[match.group(1)] = " ".join(sorted(values))
    return entries


def expectation_for(key: str, entries: dict[str, str]) -> str | None:
    """The entry for a run: its URL, its page, or a directory wildcard."""
    for name in (key, key.split("?", 1)[0]):
        if name in entries:
            return entries[name]
    for name, value in entries.items():
        if name.endswith("*") and key.startswith(name[:-1]):
            return value
    return None


def parse_baseline(text: str) -> tuple[dict[str, str], bool]:
    """A `-expected.txt`'s non-passing subtests and whether it has a harness error."""
    rows, harness_error = {}, False
    for line in text.splitlines():
        match = BASELINE_ROW.match(line)
        if match:
            rows[match.group(2)] = match.group(1)
        elif line.startswith("Harness Error."):
            harness_error = True
    return rows, harness_error


def baseline_path(wpt_root: Path, key: str) -> Path:
    """`page.html?a=b&c` has the baseline `page_a=b_c-expected.txt`."""
    path, _, query = key.partition("?")
    stem = path.rsplit(".", 1)[0]
    if query:
        stem += "_" + query.replace("&", "_")
    return wpt_root / (stem + "-expected.txt")


def oracle_status(result: dict, baseline: tuple[dict[str, str], bool] | None,
                  expectation: str | None) -> dict:
    """Whether this run reproduces Chromium's own pinned record of the test."""
    if expectation:
        return {"status": "disputed", "reason": f"test-expectations:{expectation}"}
    if result.get("unstable"):
        return {"status": "disputed", "reason": "unstable"}
    rows, harness_error = baseline or ({}, False)
    observed = {row["name"]: row["status"] for row in result["subtests"]}
    agrees = (all(status == rows.get(name, "PASS") for name, status in observed.items())
              and set(rows) <= set(observed)
              and (result["harness"] != "OK") == harness_error)
    if agrees:
        return {"status": "agrees"}
    messages = [result.get("message") or ""]
    messages += [row.get("message") or "" for row in result["subtests"]]
    if any(text in message for message in messages for text in KEYBOARD_DRIVER):
        return {"status": "disputed", "reason": "driver:keyboard"}
    return {"status": "disputed", "reason": "baseline-mismatch"}


def same_result(one: dict, two: dict) -> bool:
    def outcome(result):
        return result["harness"], [(row["name"], row["status"]) for row in result["subtests"]]
    return outcome(one) == outcome(two)


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, scripts: dict[str, bytes] | None = None, **kwargs):
        # The base class serves the request from its constructor.
        self.scripts = {"/resources/testharnessreport.js": REPORTER, **(scripts or {})}
        super().__init__(*args, **kwargs)

    def do_GET(self):  # noqa: N802 (stdlib name)
        script = self.scripts.get(self.path.split("?", 1)[0])
        if script is not None:
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript")
            self.send_header("Content-Length", str(len(script)))
            self.end_headers()
            self.wfile.write(script)
            return
        super().do_GET()

    def log_message(self, *args):
        pass


def harness():
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", HARNESS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Browser:
    def __init__(self, chrome: str, chrome_dir: str | None, real_input: bool = False,
                 extra_args: list[str] | None = None):
        self.h = harness()
        self.profile = tempfile.mkdtemp(prefix="div-wpt-script-")
        self.process = subprocess.Popen(
            [chrome, "--headless=new", "--no-first-run", "--no-default-browser-check",
             "--remote-debugging-port=0", f"--user-data-dir={self.profile}",
             "--window-size=800,600", "--disable-background-timer-throttling",
             "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows",
             *(REAL_INPUT_FLAGS if real_input else []), *(extra_args or []),
             "about:blank"],
            env=self.h.chrome_environment(chrome_dir),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        port = self.h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
        self.client = self.h._CdpWebSocket(self.h._page_websocket_url(port, timeout=30))
        product = self.client.command("Browser.getVersion").get("product", "")
        if CHROMIUM_VERSION not in product:
            raise SystemExit(f"unexpected browser version: {product}")
        self.client.command("Page.enable")
        if real_input:
            viewport = self.evaluate('innerWidth + "x" + innerHeight')
            if viewport != "800x600":
                self.close()
                raise SystemExit(f"the viewport is {viewport}, not 800x600")

    def evaluate(self, expression: str):
        result = self.client.command("Runtime.evaluate", {
            "expression": expression, "returnByValue": True})
        if "exceptionDetails" in result:
            return None
        return result.get("result", {}).get("value")

    def run(self, url: str) -> dict:
        self.client.command("Page.navigate", {"url": "about:blank"})
        time.sleep(0.05)
        self.client.command("Page.navigate", {"url": url})
        deadline = time.monotonic() + PAGE_LIMIT_SECONDS
        while time.monotonic() < deadline:
            time.sleep(0.1)
            value = self.evaluate("JSON.stringify(window.__divResults || null)")
            if value and value != "null":
                return json.loads(value)
        return {"harness": None, "message": "no completion within the page limit", "tests": []}

    def close(self) -> None:
        try:
            self.h._close_chromium_client(self.client)
        finally:
            self.process.kill()
            self.process.wait(timeout=10)
            shutil.rmtree(self.profile, ignore_errors=True)


def record(raw: dict) -> dict:
    tests = []
    for test in raw["tests"]:
        status = TEST_STATUS[test["status"]]
        row = {"name": test["name"], "status": status}
        if status != "PASS" and test.get("message"):
            row["message"] = test["message"]
        tests.append(row)
    harness_status = HARNESS_STATUS.get(raw["harness"], "NO-COMPLETION")
    result = {"harness": harness_status, "subtests": tests}
    if harness_status != "OK" and raw.get("message"):
        result["message"] = raw["message"]
    return result


def suite_files(wpt_root: Path, suite: str) -> list[str]:
    base = wpt_root / suite
    return sorted(
        path.relative_to(wpt_root).as_posix()
        for path in base.rglob("*") if path.is_file())


def capture(wpt_root: Path, suite: str, chrome: str, chrome_dir: str | None,
            only: list[str] | None = None, extra_args: list[str] | None = None) -> dict:
    files: dict[str, dict] = {}
    to_run = []
    for relative in suite_files(wpt_root, suite):
        reason = exclusion(relative)
        if reason == "helper":
            continue
        if reason is None and not is_testharness(wpt_root / relative):
            reason = "not-testharness"
        if reason is not None:
            files[relative] = {"excluded": reason}
            continue
        if only and relative not in only:
            continue
        to_run.extend(relative + variant for variant in page_variants(wpt_root / relative))
    real_input = suite in ORACLE_SUITES
    scripts = driver_files(wpt_root) if real_input else {}
    entries: dict[str, str] = {}
    if real_input:
        path = wpt_root.parents[4] / EXPECTATIONS[0]
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != EXPECTATIONS[1]:
            raise SystemExit(f"{path} is not the pinned TestExpectations")
        entries = parse_expectations(path.read_text(encoding="utf-8"), suite)
    handler = functools.partial(Handler, directory=str(wpt_root), scripts=scripts)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    browser = Browser(chrome, chrome_dir, real_input, extra_args)
    try:
        for relative in to_run:
            url = f"http://127.0.0.1:{server.server_address[1]}/{relative}"
            files[relative] = record(browser.run(url))
            if real_input:
                result = files[relative]
                for _ in range(INPUT_REPEATS - 1):
                    if not same_result(result, record(browser.run(url))):
                        result["unstable"] = True
                        break
                file = baseline_path(wpt_root, relative)
                baseline = parse_baseline(file.read_text(encoding="utf-8")) if file.is_file() else None
                result["oracle"] = oracle_status(result, baseline,
                                                 expectation_for(relative, entries))
            print(f"{relative}: {files[relative]['harness']}, "
                  f"{len(files[relative]['subtests'])} subtests", file=sys.stderr)
    finally:
        browser.close()
        server.shutdown()
    return {
        "chromium": CHROMIUM_VERSION,
        "wpt_revision": WPT_REVISION,
        "suite": suite,
        "files": dict(sorted(files.items())),
    }


def fixture_path(suite: str) -> Path:
    return FIXTURES / (suite.replace("/", "--") + ".json")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--wpt-root", type=Path, required=True)
    parser.add_argument("--suite", required=True,
                        help="a directory under the WPT root, e.g. resize-observer")
    parser.add_argument("--only", action="append",
                        help="run only this file (repeatable); for investigation")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--chrome-arg", action="append",
                        help="an extra browser flag (repeatable); for investigation")
    parser.add_argument("--chrome", type=Path,
                        help="the pinned chrome binary (default: the pixel harness's)")
    args = parser.parse_args()
    h = harness()
    if args.chrome:
        chrome, chrome_dir = str(args.chrome), str(args.chrome.parent)
    else:
        chrome, chrome_dir = h.find_chrome()
    if chrome is None:
        raise SystemExit("the pinned Chromium was not found")
    result = capture(args.wpt_root.resolve(), args.suite, chrome, chrome_dir, args.only,
                     args.chrome_arg)
    output = args.output or fixture_path(args.suite)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
