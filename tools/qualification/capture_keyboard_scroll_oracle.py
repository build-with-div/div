#!/usr/bin/env python3
"""Capture pinned Chromium 147's keyboard scrolling from real platform input.

Item I2 (sample-markdown MD-23) gives Div keyboard scrolling, `keypress` and
keyboard-focusable scrollers. This script gives a headed pinned
Chrome-for-Testing 147.0.7727.50 the platform's own key and button input:

- Windows: `WM_KEYDOWN` and `WM_KEYUP` (with the extended-key bit for the
  navigation keys), `WM_MOUSEMOVE` and `WM_LBUTTON{DOWN,UP}` are posted to the
  window of the browser this script launched. Windows' own `TranslateMessage`
  makes the `WM_CHAR` of a character key. Chromium takes keys only while its
  window is the foreground window, which a launched browser becomes, so the
  script checks the foreground before every input and stops if it changed.
  Posted messages do not set the key state Chromium reads for modifiers, so
  scenarios with modifier keys are captured on Linux only.
- Linux: X11 keys, modifiers, motion and buttons through XTEST on `$DISPLAY`,
  with the browser on Ozone's X11 platform (WSLg works).

The real cursor is never moved on Windows, and no system setting is read or
changed. Each scale runs its own browser launched with
`--force-device-scale-factor` (RFC 0010 real zoom); CDP fixes the viewport's
size. Each scenario loads the page afresh, then sends its steps. After each
step the script waits until every scroll offset has settled (Chromium animates
keyboard scrolls) and records:

- the `keydown`, `keypress`, `keyup` and `click` events, in order, with their
  key, code, modifiers and target;
- which elements fired `scroll` (not how often: that depends on the
  animation's frames);
- every scroller's offsets and the root's;
- the focused element and the form controls' values.

Each capture runs twice and must match. The test
`crates/divui/tests/keyboard_scroll_oracle.rs` replays the JSON through Div.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fidelity/fixtures/keyboard-scroll-v1"
HARNESS = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
CHROMIUM_VERSION = "147.0.7727.50"
WIDTH, HEIGHT = 480, 400
SCALES = (1.0, 1.25)

# Every box is absolutely placed with whole CSS pixels so Div builds the same
# geometry. Scrollers: (id, left, top, width, height, extra style, content
# width, content height). Content elements are `<id>Content`.
SCROLLERS = [
    ("outer", 20, 20, 200, 150, "overflow:auto", 600, 900),
    ("contain", 240, 20, 150, 100, "overflow:auto;overscroll-behavior:contain", 300, 300),
    ("hscroll", 240, 140, 150, 60, "overflow-x:auto;overflow-y:hidden", 500, 40),
    ("padded", 20, 190, 150, 120, "overflow:auto;scroll-padding-top:30px", 150, 1000),
    ("withButton", 240, 220, 150, 80, "overflow:auto", 150, 400),
]
# `inner` is a scroller inside `outer`'s content; `innerText` is inside it.
INNER = ("inner", 10, 10, 150, 100, "overflow:auto", 400, 500)
DOCUMENT_SIZE = (1500, 2500)

PAGE = """<!DOCTYPE html><html id="html"><head><style>
html, body { margin: 0; padding: 0 }
div, button, input, textarea { position: absolute; box-sizing: border-box; margin: 0 }
button, input, textarea { padding: 0; border: 1px solid #888; font: 12px sans-serif }
</style></head><body id="body">
<div id="spacer" style="left:0;top:0;width:WIDTHpx;height:HEIGHTpx"></div>
BOXES
<input id="field" style="left:20px;top:330px;width:100px;height:24px" value="text">
<textarea id="area" style="left:140px;top:325px;width:120px;height:40px">one</textarea>
<button id="btn" style="left:280px;top:330px;width:60px;height:24px">b</button>
<input id="check" type="checkbox" style="left:360px;top:334px;width:16px;height:16px">
<input id="radio" type="radio" name="r" style="left:390px;top:334px;width:16px;height:16px">
<script>
window.oracle = {events: [], scrolled: new Set()};
const name = t => t === document ? "root" : (t.id || t.nodeName);
for (const type of ["keydown", "keypress", "keyup", "click"]) {
  addEventListener(type, e => oracle.events.push({type: e.type,
    key: e.key, code: e.code, shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey,
    meta: e.metaKey, target: name(e.target)}), {capture: true});
}
addEventListener("scroll", e => oracle.scrolled.add(name(e.target)), {capture: true});
window.oracleRead = () => {
  const out = {events: oracle.events.splice(0), scrolled: [...oracle.scrolled].sort()};
  oracle.scrolled.clear();
  return out;
};
window.oracleState = () => {
  const offsets = {root: [scrollX, scrollY]};
  for (const id of IDS) {
    const element = document.getElementById(id);
    if (element) offsets[id] = [element.scrollLeft, element.scrollTop];
  }
  const active = document.activeElement;
  return {offsets, active: active ? (active.id || active.nodeName) : null,
    values: {field: field.value, area: area.value, check: check.checked,
             radio: radio.checked}};
};
</script></body></html>"""

ACTIONS = {
    # Named page actions; the Div test implements each by name.
    "remove-inner-text": "document.getElementById('innerText').remove()",
    "remove-inner": "document.getElementById('inner').remove()",
    "cancel-keydown": "addEventListener('keydown', e => e.preventDefault())",
    "cancel-keypress": "addEventListener('keypress', e => e.preventDefault())",
    "cancel-mousedown": "addEventListener('mousedown', e => e.preventDefault())",
}


def page() -> str:
    def box(spec, parent_offset=(0, 0)):
        name, left, top, width, height, style, content_width, content_height = spec
        return (f'<div id="{name}" style="left:{left}px;top:{top}px;width:{width}px;'
                f'height:{height}px;{style}">'
                f'<div id="{name}Content" style="left:0;top:0;width:{content_width}px;'
                f'height:{content_height}px">')

    boxes = []
    for spec in SCROLLERS:
        if spec[0] == "outer":
            boxes.append(box(spec) + box(INNER)
                         + '<div id="innerText" style="left:20px;top:20px;width:60px;'
                           'height:40px"></div></div></div></div></div>')
        elif spec[0] == "withButton":
            boxes.append(box(spec) + '<button id="btnIn" style="left:10px;top:10px;'
                         'width:60px;height:24px">in</button></div></div>')
        else:
            boxes.append(box(spec) + "</div></div>")
    ids = [spec[0] for spec in SCROLLERS] + ["inner", "area"]
    return (PAGE.replace("WIDTH", str(DOCUMENT_SIZE[0]))
            .replace("HEIGHT", str(DOCUMENT_SIZE[1]))
            .replace("BOXES", "\n".join(boxes))
            .replace("IDS", json.dumps(ids)))


# Steps: ("key", name, modifiers) with modifiers a string of "s" (Shift),
# "c" (Ctrl), "a" (Alt); ("click", element id), a left click at the element's
# centre, rounded to whole multiples of 4 CSS px so that it is a whole device
# pixel at 1.25; ("do", action).
def keys(*names, mods=""):
    return [("key", name, mods) for name in names]


SCENARIOS = {
    "root": keys("Down", "Down", "Up", "PageDown", "PageUp", "Space", "End", "Home",
                 "Right", "Right", "Left"),
    "press_inner": [("click", "innerText")] + keys("Down", "Down", "Down", "End", "Down",
                                                   "PageDown", "End", "Down", "Home"),
    "contain": [("click", "containContent")] + keys("End", "Down", "PageDown"),
    "removed_press_target": [("click", "innerText"), ("do", "remove-inner-text")]
    + keys("Down", "PageDown"),
    "removed_press_ancestor": [("click", "innerText"), ("do", "remove-inner")]
    + keys("Down", "PageDown"),
    "focused_button_in_scroller": [("click", "btnIn")] + keys("Down", "Down", "Space",
                                                               "PageDown"),
    # The field stays in view until the last key, so no caret reveal scrolls.
    "text_field": [("click", "field")] + keys("Down", "End", "Home", "Space", "A",
                                              "PageDown"),
    "textarea": [("click", "area")] + keys("PageDown", "Down", "End", "Space"),
    "controls": [("click", "check")] + keys("Space", "Down") + [("click", "radio")]
    + keys("Down", "Up", "PageDown"),
    "cancel_keydown": [("do", "cancel-keydown")] + keys("Down", "PageDown", "Space"),
    "cancel_keypress": [("do", "cancel-keypress")] + keys("Space", "Down", "PageDown"),
    "cancel_mousedown": [("do", "cancel-mousedown"), ("click", "innerText")] + keys("Down"),
    # Ten Tabs: the eleventh leaves the page for the browser window.
    "focus_order": keys(*["Tab"] * 10),
    "click_scroller": [("click", "containContent")] + keys("Tab", "Down"),
    "keypress_keys": keys("A", "Digit1", "Enter", "Escape", "Backspace", "Delete",
                          "Space"),
    "hscroll": [("click", "hscrollContent")] + keys("Right", "Right", "Left", "Down",
                                                    "End"),
    "padded": [("click", "paddedContent")] + keys("PageDown", "PageDown", "PageUp"),
    # Linux only: modifiers.
    "modifiers": keys("Space") + [("key", "Space", "s"), ("key", "Down", "s"),
                                  ("key", "End", "c"), ("key", "Home", "c"),
                                  ("key", "Down", "c"), ("key", "Down", "a"),
                                  ("key", "Up", "a")],
    "modified_keypress": [("key", "A", "s"), ("key", "A", "c"), ("key", "Digit1", "s"),
                          ("key", "Enter", "s"), ("key", "Enter", "c")],
}


def has_modifiers(steps) -> bool:
    return any(step[0] == "key" and step[2] for step in steps)


# Windows virtual keys and scan codes (US layout), and whether the key is
# extended; X11 keysyms.
WINDOWS_KEYS = {
    "Down": (0x28, 0x50, True), "Up": (0x26, 0x48, True), "Left": (0x25, 0x4B, True),
    "Right": (0x27, 0x4D, True), "PageDown": (0x22, 0x51, True),
    "PageUp": (0x21, 0x49, True), "Home": (0x24, 0x47, True), "End": (0x23, 0x4F, True),
    "Space": (0x20, 0x39, False), "Tab": (0x09, 0x0F, False), "Enter": (0x0D, 0x1C, False),
    "Escape": (0x1B, 0x01, False), "Backspace": (0x08, 0x0E, False),
    "Delete": (0x2E, 0x53, True), "A": (0x41, 0x1E, False), "Digit1": (0x31, 0x02, False),
}
X11_KEYS = {
    "Down": 0xFF54, "Up": 0xFF52, "Left": 0xFF51, "Right": 0xFF53, "PageDown": 0xFF56,
    "PageUp": 0xFF55, "Home": 0xFF50, "End": 0xFF57, "Space": 0x20, "Tab": 0xFF09,
    "Enter": 0xFF0D, "Escape": 0xFF1B, "Backspace": 0xFF08, "Delete": 0xFFFF,
    "A": 0x61, "Digit1": 0x31,
}
X11_MODIFIERS = {"s": 0xFFE1, "c": 0xFFE3, "a": 0xFFE9}

SETTLE_POLL = 0.05
SETTLE_STABLE = 6


def harness():
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", HARNESS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Aborted(RuntimeError):
    pass


class Browser:
    """A headed pinned browser with a CDP client, its viewport fixed by CDP."""

    def __init__(self, chrome: Path, scale: float, extra: tuple[str, ...]):
        self.h = harness()
        self.scale = scale
        self.profile = tempfile.mkdtemp(prefix="div-keyboard-oracle-")
        self.directory = Path(tempfile.mkdtemp(prefix="div-keyboard-page-"))
        (self.directory / "page.html").write_text(page(), encoding="utf-8")
        size = (int((WIDTH + 120) * scale), int((HEIGHT + 160) * scale))
        self.process = subprocess.Popen(
            [str(chrome), "--no-first-run", "--no-default-browser-check",
             "--remote-debugging-port=0", f"--user-data-dir={self.profile}",
             f"--force-device-scale-factor={scale}", "--window-position=200,100",
             f"--window-size={size[0]},{size[1]}", *extra,
             "--app=" + (self.directory / "page.html").as_uri() + "?start"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            port = self.h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
            self.client = self.h._CdpWebSocket(self.h._page_websocket_url(port, timeout=30))
            self.client.command("Page.enable")
            product = self.client.command("Browser.getVersion").get("product", "")
            if CHROMIUM_VERSION not in product:
                raise SystemExit(f"unexpected browser version: {product}")
            self.client.command("Emulation.setDeviceMetricsOverride", {
                "width": WIDTH, "height": HEIGHT, "deviceScaleFactor": 0, "mobile": False})
        except BaseException:
            self.close()
            raise
        self.origin = None

    def evaluate(self, expression: str, await_promise: bool = False):
        result = self.client.command("Runtime.evaluate", {
            "expression": expression, "awaitPromise": await_promise, "returnByValue": True})
        if "exceptionDetails" in result:
            raise RuntimeError(json.dumps(result["exceptionDetails"])[:500])
        return result.get("result", {}).get("value")

    def load(self, tag: str) -> None:
        url = (self.directory / "page.html").as_uri() + "?" + tag
        self.client.command("Page.navigate", {"url": url})
        for _ in range(100):
            ready = self.evaluate("location.search === %s && document.readyState === 'complete'"
                                  " && typeof oracleState === 'function'" % json.dumps("?" + tag))
            if ready:
                break
            time.sleep(0.05)
        else:
            raise RuntimeError("the page did not load")
        self.evaluate("new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
                      await_promise=True)
        metrics = self.evaluate("[innerWidth, innerHeight, devicePixelRatio]")
        if metrics != [WIDTH, HEIGHT, self.scale]:
            raise RuntimeError(f"viewport emulation failed: {metrics}")

    def settle(self) -> dict:
        """The step's record, once every offset has been still for a while."""
        previous, stable = None, 0
        for _ in range(200):
            time.sleep(SETTLE_POLL)
            state = self.evaluate("JSON.stringify(oracleState())")
            stable = stable + 1 if state == previous else 0
            previous = state
            if stable >= SETTLE_STABLE:
                record = self.evaluate("oracleRead()")
                record.update(json.loads(self.evaluate("JSON.stringify(oracleState())")))
                return record
        raise RuntimeError("the scroll offsets did not settle")

    def point(self, element: str) -> tuple[int, int]:
        """The centre of the element's visible part (clipped by its scrolling
        ancestors and the viewport) in CSS px, rounded to a multiple of 4."""
        x, y = self.evaluate("""(() => {
          const element = document.getElementById(%s);
          let r = element.getBoundingClientRect();
          let [left, top, right, bottom] = [r.left, r.top, r.right, r.bottom];
          for (let a = element.parentElement; a; a = a.parentElement) {
            const style = getComputedStyle(a);
            if (style.overflowX === 'visible' && style.overflowY === 'visible') continue;
            const c = a.getBoundingClientRect();
            left = Math.max(left, c.left + a.clientLeft);
            top = Math.max(top, c.top + a.clientTop);
            right = Math.min(right, c.left + a.clientLeft + a.clientWidth);
            bottom = Math.min(bottom, c.top + a.clientTop + a.clientHeight);
          }
          right = Math.min(right, innerWidth); bottom = Math.min(bottom, innerHeight);
          return [(left + right) / 2, (top + bottom) / 2];
        })()""" % json.dumps(element))
        return (int(round(x / 4)) * 4, int(round(y / 4)) * 4)

    def calibrate(self, move) -> None:
        """The viewport's origin in the input's device pixels: one move,
        read back as a client point, checked at a second."""
        self.evaluate("window.calibration = null; addEventListener('mousemove', "
                      "e => { calibration = [e.clientX, e.clientY]; })")
        guess = self.guess()
        move(*guess)
        time.sleep(0.3)
        client = self.evaluate("calibration")
        if client is None:
            raise RuntimeError(f"no mousemove reached the page at {guess}")
        self.origin = (guess[0] - client[0] * self.scale, guess[1] - client[1] * self.scale)
        move(*self.device(40, 40))
        time.sleep(0.3)
        if self.evaluate("calibration") != [40, 40]:
            raise RuntimeError(f"the viewport origin {self.origin} is not stable")

    def device(self, x: float, y: float) -> tuple[int, int]:
        return (int(round(self.origin[0] + x * self.scale)),
                int(round(self.origin[1] + y * self.scale)))

    def close(self) -> None:
        try:
            if getattr(self, "client", None) is not None:
                self.h._close_chromium_client(self.client)
        finally:
            if os.name == "nt":
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.process.pid)],
                               capture_output=True)
            else:
                self.process.kill()
            self.process.wait(timeout=10)
            shutil.rmtree(self.profile, ignore_errors=True)
            shutil.rmtree(self.directory, ignore_errors=True)


def run_scenarios(browser: Browser, inputs, scenarios: dict, run: int) -> dict:
    """Every scenario through `inputs` (key, click and guard callables)."""
    results = {}
    for name, steps in scenarios.items():
        browser.load(f"{name}-{run}")
        inputs.park()
        browser.settle()
        log = []
        for index, step in enumerate(steps):
            try:
                inputs.guard()
            except Aborted as error:
                raise Aborted(f"{error} (scale {browser.scale}, {name} step {index} {step})")
            if step[0] == "key":
                inputs.key(step[1], step[2])
                recorded = list(step)
            elif step[0] == "click":
                x, y = browser.point(step[1])
                inputs.click(*browser.device(x, y))
                recorded = ["click", step[1], x, y]
            else:
                browser.evaluate(ACTIONS[step[1]])
                recorded = list(step)
            entry = browser.settle()
            entry["input"] = recorded
            log.append(entry)
        results[name] = log
    return results


def capture_windows(chrome: Path, scale: float, scenarios: dict, run: int) -> dict:
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    # Per-monitor DPI awareness, so client coordinates are physical pixels.
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    WM_KEYDOWN, WM_KEYUP, WM_MOUSEMOVE = 0x0100, 0x0101, 0x0200
    WM_LBUTTONDOWN, WM_LBUTTONUP, MK_LBUTTON = 0x0201, 0x0202, 0x0001

    browser = Browser(chrome, scale, ())
    try:
        windows = []
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        def collect(hwnd, _):
            pid = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            name = ctypes.create_unicode_buffer(64)
            user32.GetClassNameW(hwnd, name, 64)
            if (pid.value == browser.process.pid and name.value == "Chrome_WidgetWin_1"
                    and user32.IsWindowVisible(hwnd)):
                windows.append(hwnd)
            return True

        for _ in range(50):
            windows.clear()
            user32.EnumWindows(callback_type(collect), 0)
            if windows:
                break
            time.sleep(0.1)
        if len(windows) != 1:
            raise RuntimeError(f"expected one browser window, found {len(windows)}")
        hwnd = windows[0]

        class Inputs:
            def guard(self):
                if user32.GetForegroundWindow() != hwnd:
                    raise Aborted("the browser is not the foreground window: stopped")

            def post(self, message, w, x, y):
                user32.PostMessageW(hwnd, message, w, ((y & 0xFFFF) << 16) | (x & 0xFFFF))

            def park(self):
                # A move to a blank corner of the page, so hover is the same
                # at every scenario's start.
                self.post(WM_MOUSEMOVE, 0, *browser.device(WIDTH - 8, HEIGHT - 8))

            def key(self, name, modifiers):
                assert not modifiers, "modifiers cannot be posted"
                vk, scan, extended = WINDOWS_KEYS[name]
                flags = 1 | (scan << 16) | ((1 << 24) if extended else 0)
                user32.PostMessageW(hwnd, WM_KEYDOWN, vk, flags)
                user32.PostMessageW(hwnd, WM_KEYUP, vk, flags | (1 << 30) | (1 << 31))

            def click(self, x, y):
                self.post(WM_MOUSEMOVE, 0, x, y)
                self.post(WM_LBUTTONDOWN, MK_LBUTTON, x, y)
                self.post(WM_LBUTTONUP, 0, x, y)

        inputs = Inputs()
        time.sleep(1.0)
        inputs.guard()
        browser.load(f"calibrate-{run}")
        client = wintypes.RECT()
        user32.GetClientRect(hwnd, ctypes.byref(client))
        browser.guess = lambda: (client.right // 2, client.bottom // 2)
        browser.calibrate(lambda x, y: inputs.post(WM_MOUSEMOVE, 0, x, y))
        return run_scenarios(browser, inputs, scenarios, run)
    finally:
        browser.close()


def capture_x11(chrome: Path, scale: float, scenarios: dict, run: int) -> dict:
    import ctypes

    x11 = ctypes.CDLL("libX11.so.6")
    xtst = ctypes.CDLL("libXtst.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int,
                                          ctypes.c_ulong]
    xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
                                          ctypes.c_int, ctypes.c_ulong]
    xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int,
                                       ctypes.c_ulong]
    display = x11.XOpenDisplay(None)
    if not display:
        raise SystemExit("no X display: set DISPLAY")

    def keycode(keysym):
        return x11.XKeysymToKeycode(display, keysym)

    def motion(x, y):
        xtst.XTestFakeMotionEvent(display, -1, x, y, 0)
        x11.XFlush(display)

    browser = Browser(chrome, scale, ("--ozone-platform=x11", "--no-sandbox"))
    try:
        class Inputs:
            def guard(self):
                if not browser.evaluate("document.hasFocus()"):
                    raise Aborted("the page lost focus: stopped")

            def park(self):
                motion(*browser.device(WIDTH - 8, HEIGHT - 8))

            def key(self, name, modifiers):
                held = [keycode(X11_MODIFIERS[m]) for m in modifiers]
                for code in held:
                    xtst.XTestFakeKeyEvent(display, code, 1, 0)
                code = keycode(X11_KEYS[name])
                xtst.XTestFakeKeyEvent(display, code, 1, 0)
                xtst.XTestFakeKeyEvent(display, code, 0, 0)
                for code in reversed(held):
                    xtst.XTestFakeKeyEvent(display, code, 0, 0)
                x11.XFlush(display)

            def click(self, x, y):
                motion(x, y)
                xtst.XTestFakeButtonEvent(display, 1, 1, 0)
                xtst.XTestFakeButtonEvent(display, 1, 0, 0)
                x11.XFlush(display)

        inputs = Inputs()
        time.sleep(1.0)  # let the window manager place the window
        browser.load(f"calibrate-{run}")
        # Screen coordinates are DIPs; X11's are device pixels. The viewport
        # sits at the top left of the window's content, below its title bar.
        left, top = browser.evaluate("[screenX, screenY]")
        browser.guess = lambda: (int((left + WIDTH / 2) * scale),
                                 int((top + 60 + HEIGHT / 2) * scale))
        browser.calibrate(motion)
        inputs.guard()
        return run_scenarios(browser, inputs, scenarios, run)
    finally:
        browser.close()
        x11.XCloseDisplay(display)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True,
                        help="the pinned Chrome-for-Testing 147.0.7727.50 binary")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--only", help="capture these comma-separated scenarios once and print")
    parser.add_argument("--scales", default=",".join(str(s) for s in SCALES))
    args = parser.parse_args()
    windows = os.name == "nt"
    if windows:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import windows_chrome

        chrome = windows_chrome.verify_chrome(args.chrome)
        name, capture = "chromium-windows.json", capture_windows
        method = ("WM_KEYDOWN/WM_KEYUP (extended bit for navigation keys; Windows makes "
                  "WM_CHAR), WM_MOUSEMOVE and WM_LBUTTON{DOWN,UP} posted to the pinned "
                  "browser's foreground window; unmodified keys only")
        scenarios = {k: v for k, v in SCENARIOS.items() if not has_modifiers(v)}
    else:
        chrome = args.chrome.resolve()
        name, capture = "chromium-linux-x11.json", capture_x11
        method = "X11 keys, modifiers, motion and buttons through XTEST, --ozone-platform=x11"
        scenarios = dict(SCENARIOS)
    scales = [float(s) for s in args.scales.split(",")]
    if args.only:
        wanted = args.only.split(",")
        scenarios = {k: v for k, v in scenarios.items() if k in wanted}
        print(json.dumps({str(scale): capture(chrome, scale, scenarios, 0) for scale in scales},
                         indent=1))
        return 0

    started = time.monotonic()
    captured = {}
    try:
        for scale in scales:
            first = capture(chrome, scale, scenarios, 1)
            second = capture(chrome, scale, scenarios, 2)
            if first != second:
                differing = [k for k in first if first[k] != second.get(k)]
                raise RuntimeError(f"captures at {scale} differ in {differing}")
            captured[str(scale)] = first
    except Aborted as error:
        print(f"ABORTED after {time.monotonic() - started:.0f} s: {error}")
        return 2
    document = {
        "chromium": CHROMIUM_VERSION,
        "platform": "windows" if windows else "linux-x11",
        "host": platform.platform(),
        "input": method,
        "scale_method": "--force-device-scale-factor (RFC 0010 real zoom)",
        "viewport": [WIDTH, HEIGHT],
        "document_size": list(DOCUMENT_SIZE),
        "scrollers": [list(spec) for spec in SCROLLERS] + [list(INNER)],
        "actions": ACTIONS,
        "scales": captured,
    }
    output = args.output or FIXTURES / name
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {output} in {time.monotonic() - started:.0f} s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
