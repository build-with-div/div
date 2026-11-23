#!/usr/bin/env python3
"""Record what pinned Chromium fires when a listener removes the pointer's target.

Runs on Linux with Chrome-for-Testing 147.0.7727.50, over a CDP pipe:

    python3 tests/fidelity/fixtures/pointer-removed-target-v1/capture.py --chrome PATH

Every scenario runs three times. The capture fails unless all three agree.
`crates/divui/tests/pointer_removed_target.rs` replays the result.
"""
import argparse
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERSION = "147.0.7727.50"
MOUSE_WHEN = ("pointerdown", "mousedown", "pointerup", "mouseup")
TAP_WHEN = ("pointerdown", "pointerup")
REMOVED = ("child", "parent")
FRAMES = "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))"
RESULT = (
    "JSON.stringify({log: window.log, focus: document.activeElement.id"
    " || document.activeElement.nodeName.toLowerCase()})"
)


class Browser:
    """Chrome driven over `--remote-debugging-pipe`."""

    def __init__(self, chrome: str):
        self.work = tempfile.mkdtemp()
        self.to_read, self.to_write = os.pipe()
        self.from_read, self.from_write = os.pipe()

        def pipes():
            read_end = os.dup(self.to_read)
            write_end = os.dup(self.from_write)
            os.dup2(read_end, 3, inheritable=True)
            os.dup2(write_end, 4, inheritable=True)

        self.process = subprocess.Popen(
            [chrome, "--headless=new", "--no-sandbox", "--disable-gpu", "--no-first-run",
             "--disable-background-networking", "--disable-component-update",
             "--disable-sync", "--remote-debugging-pipe", "--window-size=800,600",
             f"--user-data-dir={self.work}/profile", "about:blank"],
            preexec_fn=pipes, close_fds=False, stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(self.to_read)
        os.close(self.from_write)
        self.buffer = b""
        self.next_id = 0

    def call(self, method: str, params: dict | None = None, session: str | None = None) -> dict:
        self.next_id += 1
        ident = self.next_id
        message = {"id": ident, "method": method, "params": params or {}}
        if session:
            message["sessionId"] = session
        os.write(self.to_write, json.dumps(message).encode() + b"\0")
        while True:
            while b"\0" not in self.buffer:
                chunk = os.read(self.from_read, 65536)
                if not chunk:
                    raise RuntimeError("Chrome closed the pipe")
                self.buffer += chunk
            raw, self.buffer = self.buffer.split(b"\0", 1)
            reply = json.loads(raw)
            if reply.get("id") == ident:
                if "error" in reply:
                    raise RuntimeError(reply["error"])
                return reply["result"]

    def evaluate(self, session: str, expression: str):
        return self.call("Runtime.evaluate",
                         {"expression": expression, "returnByValue": True, "awaitPromise": True},
                         session)["result"].get("value")

    def close(self) -> None:
        self.process.kill()
        self.process.wait()
        shutil.rmtree(self.work, ignore_errors=True)


def open_page(browser: Browser, when: str, which: str, touch: bool) -> tuple[str, str]:
    target = browser.call("Target.createTarget", {"url": "about:blank"})["targetId"]
    session = browser.call("Target.attachToTarget",
                           {"targetId": target, "flatten": True})["sessionId"]
    browser.call("Page.enable", {}, session)
    if touch:
        browser.call("Emulation.setTouchEmulationEnabled",
                     {"enabled": True, "maxTouchPoints": 5}, session)
    browser.call("Page.navigate",
                 {"url": f"{(HERE / 'page.html').as_uri()}?when={when}&which={which}"}, session)
    ready = "location.protocol === 'file:' && document.readyState === 'complete'"
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            if browser.evaluate(session, ready):
                break
        except RuntimeError:
            pass  # The context is replaced while the page navigates.
        time.sleep(0.02)
    else:
        raise RuntimeError("the page did not load")
    browser.call("Page.bringToFront", {}, session)
    browser.call("Emulation.setFocusEmulationEnabled", {"enabled": True}, session)
    browser.evaluate(session, FRAMES)
    return target, session


def finish(browser: Browser, target: str, session: str, settle: float) -> dict:
    browser.evaluate(session, FRAMES)
    time.sleep(settle)
    browser.evaluate(session, FRAMES)
    result = json.loads(browser.evaluate(session, RESULT))
    browser.call("Target.closeTarget", {"targetId": target})
    return result


def mouse_scenario(browser: Browser, when: str, which: str) -> dict:
    target, session = open_page(browser, when, which, touch=False)

    def mouse(kind: str, button: str, buttons: int, clicks: int) -> None:
        browser.call("Input.dispatchMouseEvent",
                     {"type": kind, "x": 50, "y": 25, "button": button, "buttons": buttons,
                      "clickCount": clicks, "pointerType": "mouse"}, session)

    mouse("mouseMoved", "none", 0, 0)
    browser.evaluate(session, "window.recording = true; 0")
    mouse("mousePressed", "left", 1, 1)
    mouse("mouseReleased", "left", 0, 1)
    return finish(browser, target, session, 0.05)


def tap_scenario(browser: Browser, when: str, which: str) -> dict:
    target, session = open_page(browser, when, which, touch=True)
    browser.evaluate(session, "window.recording = true; 0")
    browser.call("Input.dispatchTouchEvent",
                 {"type": "touchStart", "touchPoints": [{"x": 50, "y": 25, "id": 1}]}, session)
    browser.call("Input.dispatchTouchEvent", {"type": "touchEnd", "touchPoints": []}, session)
    return finish(browser, target, session, 0.5)  # The tap gesture arrives after the release.


def capture(browser: Browser) -> dict:
    return {
        "mouse": {f"{when}/{which}": mouse_scenario(browser, when, which)
                  for when in MOUSE_WHEN for which in REMOVED},
        "tap": {f"{when}/{which}": tap_scenario(browser, when, which)
                for when in TAP_WHEN for which in REMOVED},
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--chrome", required=True)
    parser.add_argument("--output", type=Path, default=HERE / "chromium-linux.json")
    args = parser.parse_args()
    browser = Browser(args.chrome)
    try:
        product = browser.call("Browser.getVersion")["product"]
        if product != f"Chrome/{VERSION}":
            raise SystemExit(f"expected Chrome/{VERSION}, found {product}")
        runs = [capture(browser) for _ in range(3)]
    finally:
        browser.close()
    if any(run != runs[0] for run in runs):
        raise SystemExit("the three runs differ")
    record = {"chromium": VERSION, "platform": "linux", "input": "cdp", "repeats": 3, **runs[0]}
    with args.output.open("w", encoding="utf-8", newline="\n") as out:
        json.dump(record, out, indent=1)
        out.write("\n")
    print(f"{args.output}: {len(runs[0]['mouse'])} mouse and {len(runs[0]['tap'])} tap scenarios")


if __name__ == "__main__":
    main()
