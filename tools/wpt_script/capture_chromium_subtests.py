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

Usage (Linux; the pinned build from `run_all_pixel_comparisons.py`):

    python3 tools/wpt_script/capture_chromium_subtests.py \
        --wpt-root out/wpt-script/09d377d9/third_party/blink/web_tests/external/wpt \
        --suite resize-observer
"""

from __future__ import annotations

import argparse
import functools
import http.server
import importlib.util
import json
import os
from pathlib import Path
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
    return None


def is_testharness(path: Path) -> bool:
    text = path.read_text(encoding="utf-8", errors="replace")
    return "/resources/testharness.js" in text


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):  # noqa: N802 (stdlib name)
        if self.path.split("?", 1)[0] == "/resources/testharnessreport.js":
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript")
            self.send_header("Content-Length", str(len(REPORTER)))
            self.end_headers()
            self.wfile.write(REPORTER)
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
    def __init__(self, chrome: str, chrome_dir: str | None):
        self.h = harness()
        self.profile = tempfile.mkdtemp(prefix="div-wpt-script-")
        self.process = subprocess.Popen(
            [chrome, "--headless=new", "--no-first-run", "--no-default-browser-check",
             "--remote-debugging-port=0", f"--user-data-dir={self.profile}",
             "--window-size=800,600", "--disable-background-timer-throttling",
             "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows",
             "about:blank"],
            env=self.h.chrome_environment(chrome_dir),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        port = self.h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
        self.client = self.h._CdpWebSocket(self.h._page_websocket_url(port, timeout=30))
        product = self.client.command("Browser.getVersion").get("product", "")
        if CHROMIUM_VERSION not in product:
            raise SystemExit(f"unexpected browser version: {product}")
        self.client.command("Page.enable")

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
            only: list[str] | None = None) -> dict:
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
        to_run.append(relative)
    handler = functools.partial(Handler, directory=str(wpt_root))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    browser = Browser(chrome, chrome_dir)
    try:
        for relative in to_run:
            url = f"http://127.0.0.1:{server.server_address[1]}/{relative}"
            files[relative] = record(browser.run(url))
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
    result = capture(args.wpt_root.resolve(), args.suite, chrome, chrome_dir, args.only)
    output = args.output or fixture_path(args.suite)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
