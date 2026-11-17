#!/usr/bin/env python3
"""Capture pinned Chromium 147 click counts from real platform mouse input.

CDP's `Input.dispatchMouseEvent` takes `clickCount` from the caller, so it
skips what this oracle measures: how the browser counts repeated clicks from
platform input, and the `detail`, `click` and `dblclick` events that follow.
This script gives a headed browser the platform's own button input:

- Windows: `WM_MOUSEMOVE` and `WM_[LMR]BUTTON{DOWN,UP}` posted to the window
  of the pinned browser this script launched, with the key state in `wParam`
  and the point in `lParam`. The real cursor and input queue are not touched.
  The system's double-click time and box (`GetDoubleClickTime`,
  `SM_CXDOUBLECLK`, `SM_CYDOUBLECLK`) are recorded; the timing steps are
  relative to that time.
- Linux: X11 button and motion events, and the Shift key, through XTEST on
  `$DISPLAY`, with the browser on Ozone's X11 platform (WSLg works). Chromium
  uses its own 500 ms limit there, not the desktop's.

Touch taps go through CDP `Input.dispatchTouchEvent`, which the browser's
gesture detector turns into taps and their tap count, on both platforms.

Each scale runs its own browser launched with `--force-device-scale-factor`
(RFC 0010 real zoom). Each capture runs twice and must match. Distances are
in device pixels. The test `crates/divui/tests/click_count_oracle.rs` reads
the JSON.
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
FIXTURE = ROOT / "tests/fidelity/fixtures/click-count-v1"
HARNESS = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
CHROMIUM_VERSION = "147.0.7727.50"
WIDTH, HEIGHT = 480, 320
SCALES = (1.0, 1.25)
# Chromium's Linux/Ozone limit (`ui::kDoubleClickTimeMs`).
LINUX_DOUBLE_CLICK_MS = 500

EVENTS = ("mousedown", "mouseup", "click", "dblclick", "auxclick", "contextmenu",
          "mousemove", "mouseover", "mouseout", "mouseenter", "mouseleave",
          "pointerdown", "pointerup", "pointermove", "pointercancel")

PAGE = """<!DOCTYPE html><html><head><style>
html, body { margin: 0; height: 100%; overflow: hidden; user-select: none;
             touch-action: none }
#target { position: fixed; inset: 0 }
</style></head><body><div id="target"></div><script>
window.clickLog = [];
window.cancelPointerdown = false;
for (const type of TYPES) addEventListener(type, event => {
  if (event.target !== window && event.target.id !== "target") return;
  const entry = {type: event.type, detail: event.detail, button: event.button,
                 buttons: event.buttons, shift: event.shiftKey, ctrl: event.ctrlKey,
                 x: Math.round(event.clientX * 100) / 100,
                 y: Math.round(event.clientY * 100) / 100};
  if ("pointerType" in event) {
    entry.pointerType = event.pointerType;
    entry.isPrimary = event.isPrimary;
  }
  clickLog.push(entry);
}, {capture: true});
addEventListener("pointerdown", event => {
  if (cancelPointerdown) event.preventDefault();
});
// No native context menu or middle-button autoscroll during the capture.
addEventListener("contextmenu", event => event.preventDefault());
addEventListener("mousedown", event => { if (event.button === 1) event.preventDefault(); });
</script></body></html>""".replace("TYPES", json.dumps(list(EVENTS)))

READ = "JSON.stringify(clickLog.splice(0))"

# Mouse scenarios. Steps, in device pixels from the scenario's start point:
#   ("move", dx, dy), ("down", button, modifiers), ("up", button, modifiers),
#   ("sleep", ms), ("sleep-limit", offset_ms): the double-click time plus the
#   offset. Buttons: "L", "M", "R". Modifiers: a string of "s" (Shift).
CLICK = (("down", "L", ""), ("up", "L", ""))
GAP = (("sleep", 100),)


def clicks(count: int, button: str = "L") -> tuple:
    steps = ()
    for index in range(count):
        if index:
            steps += GAP
        steps += (("down", button, ""), ("up", button, ""))
    return steps


MOUSE_SCENARIOS = {
    "single": CLICK,
    "double": clicks(2),
    "triple": clicks(3),
    "quadruple": clicks(4),
    "time-inside": CLICK + (("sleep-limit", -150),) + CLICK,
    "time-outside": CLICK + (("sleep-limit", 150),) + CLICK,
    "distance-inside": CLICK + GAP + (("move", 2, 2),) + CLICK,
    "distance-outside-x": CLICK + GAP + (("move", 3, 0),) + CLICK,
    "distance-outside-y": CLICK + GAP + (("move", 0, 3),) + CLICK,
    "away-and-back": CLICK + (("move", 20, 0), ("sleep", 50), ("move", 0, 0)) + CLICK,
    "button-change": CLICK + GAP + (("down", "R", ""), ("up", "R", "")),
    "right-double": clicks(2, "R"),
    "middle-double": clicks(2, "M"),
    "modifier-change": CLICK + GAP + (("down", "L", "s"), ("up", "L", "s")),
    "drag-small": (("down", "L", ""), ("move", 1, 0), ("up", "L", "")),
    "drag-away": (("down", "L", ""), ("move", 20, 0), ("up", "L", "")),
    "long-press-still": (("down", "L", ""), ("sleep-limit", 200), ("up", "L", "")),
    "long-press-move": (("down", "L", ""), ("sleep-limit", 200), ("move", 1, 0),
                        ("up", "L", "")),
    "double-then-drag": clicks(2) + GAP + (("down", "L", ""), ("move", 20, 0),
                                           ("up", "L", "")),
    "move-inside-between": CLICK + (("move", 2, 0), ("sleep", 50), ("move", 0, 0)) + CLICK,
    "anchor-drift": CLICK + GAP + (("move", 2, 0),) + CLICK + GAP + (("move", 4, 0),) + CLICK,
}

# Touch scenarios, through CDP: ("tap", dx, dy) in CSS pixels, ("sleep", ms),
# ("cancel-pointerdown",) makes the page cancel `pointerdown` from then on.
TOUCH_SCENARIOS = {
    "tap": (("tap", 0, 0),),
    "double-tap": (("tap", 0, 0), ("sleep", 100), ("tap", 0, 0)),
    "triple-tap": (("tap", 0, 0), ("sleep", 100), ("tap", 0, 0), ("sleep", 100),
                   ("tap", 0, 0)),
    "quadruple-tap": (("tap", 0, 0), ("sleep", 100), ("tap", 0, 0), ("sleep", 100),
                      ("tap", 0, 0), ("sleep", 100), ("tap", 0, 0)),
    "tap-time-350": (("tap", 0, 0), ("sleep", 350), ("tap", 0, 0)),
    "tap-time-450": (("tap", 0, 0), ("sleep", 450), ("tap", 0, 0)),
    "tap-time-650": (("tap", 0, 0), ("sleep", 650), ("tap", 0, 0)),
    "tap-time-800": (("tap", 0, 0), ("sleep", 800), ("tap", 0, 0)),
    "tap-distance-15": (("tap", 0, 0), ("sleep", 100), ("tap", 15, 0)),
    "tap-distance-19": (("tap", 0, 0), ("sleep", 100), ("tap", 19, 0)),
    "tap-distance-21": (("tap", 0, 0), ("sleep", 100), ("tap", 21, 0)),
    "tap-distance-40": (("tap", 0, 0), ("sleep", 100), ("tap", 40, 0)),
    "double-tap-cancelled": (("cancel-pointerdown",), ("tap", 0, 0), ("sleep", 100),
                             ("tap", 0, 0)),
}

# The pause before each scenario, longer than any double-click time, so
# scenarios do not count each other's clicks.
RESET_MS = 1500


def harness():
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", HARNESS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Browser:
    """A headed pinned browser showing the page, with a CDP client."""

    def __init__(self, chrome: Path, page: Path, scale: float, extra: tuple[str, ...]):
        self.h = harness()
        self.profile = tempfile.mkdtemp(prefix="div-click-oracle-")
        size = (int(WIDTH * scale + 0.5), int((HEIGHT + 80) * scale + 0.5))
        self.process = subprocess.Popen(
            [str(chrome), "--no-first-run", "--no-default-browser-check",
             "--remote-debugging-port=0", f"--user-data-dir={self.profile}",
             f"--force-device-scale-factor={scale}", "--window-position=0,0",
             f"--window-size={size[0]},{size[1]}", *extra, "--app=" + page.as_uri()],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        port = self.h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
        self.client = self.h._CdpWebSocket(self.h._page_websocket_url(port, timeout=30))
        product = self.client.command("Browser.getVersion").get("product", "")
        if CHROMIUM_VERSION not in product:
            raise SystemExit(f"unexpected browser version: {product}")
        for _ in range(100):
            if self.evaluate("typeof clickLog") == "object":
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("the page did not load")
        if self.evaluate("devicePixelRatio") != scale:
            raise RuntimeError("--force-device-scale-factor was not applied")

    def evaluate(self, expression: str):
        result = self.client.command("Runtime.evaluate", {
            "expression": expression, "returnByValue": True})
        if "exceptionDetails" in result:
            raise RuntimeError(json.dumps(result["exceptionDetails"])[:500])
        return result.get("result", {}).get("value")

    def read(self) -> list:
        time.sleep(0.4)
        return json.loads(self.evaluate(READ))

    def close(self) -> None:
        try:
            self.h._close_chromium_client(self.client)
        finally:
            if os.name == "nt":
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.process.pid)],
                               capture_output=True)
            else:
                self.process.kill()
            self.process.wait(timeout=10)
            shutil.rmtree(self.profile, ignore_errors=True)


def run_touch(browser: Browser) -> dict:
    """The touch scenarios, through CDP, at the viewport's centre."""
    browser.client.command("Emulation.setTouchEmulationEnabled",
                           {"enabled": True, "maxTouchPoints": 1})
    results = {}
    for name, steps in TOUCH_SCENARIOS.items():
        time.sleep(RESET_MS / 1000)
        browser.evaluate("cancelPointerdown = false")
        browser.read()
        for step in steps:
            if step[0] == "sleep":
                time.sleep(step[1] / 1000)
            elif step[0] == "cancel-pointerdown":
                browser.evaluate("cancelPointerdown = true")
            else:
                x, y = WIDTH / 2 + step[1], HEIGHT / 2 + step[2]
                browser.client.command("Input.dispatchTouchEvent", {
                    "type": "touchStart", "touchPoints": [{"x": x, "y": y, "id": 0}]})
                browser.client.command("Input.dispatchTouchEvent", {
                    "type": "touchEnd", "touchPoints": []})
        results[name] = {"steps": [list(step) for step in steps], "events": browser.read()}
    browser.evaluate("cancelPointerdown = false")
    return results


def capture_windows(chrome: Path, page: Path, scale: float) -> tuple[dict, dict]:
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    # Per-monitor DPI awareness, so window and client coordinates are physical.
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    limit = user32.GetDoubleClickTime()
    system = {"double_click_time_ms": limit,
              "double_click_width": user32.GetSystemMetrics(36),   # SM_CXDOUBLECLK
              "double_click_height": user32.GetSystemMetrics(37)}  # SM_CYDOUBLECLK
    if limit + 200 >= RESET_MS:
        raise SystemExit(f"the double-click time {limit} ms is too long for this capture")

    messages = {"L": (0x0201, 0x0202, 0x0001), "R": (0x0204, 0x0205, 0x0002),
                "M": (0x0207, 0x0208, 0x0010)}
    WM_MOUSEMOVE, MK_SHIFT = 0x0200, 0x0004

    browser = Browser(chrome, page, scale, ())
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

        user32.EnumWindows(callback_type(collect), 0)
        if len(windows) != 1:
            raise RuntimeError(f"expected one browser window, found {len(windows)}")
        hwnd = windows[0]
        # HWND_TOPMOST, without moving, sizing or activating it.
        user32.SetWindowPos(hwnd, wintypes.HWND(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010)
        client = wintypes.RECT()
        user32.GetClientRect(hwnd, ctypes.byref(client))
        start = (client.right // 2, client.bottom * 2 // 3)
        held = {"keys": 0}

        def post(message, dx, dy, keys):
            x, y = start[0] + dx, start[1] + dy
            user32.PostMessageW(hwnd, message, keys, ((y & 0xFFFF) << 16) | (x & 0xFFFF))

        mouse = {}
        for name, steps in MOUSE_SCENARIOS.items():
            time.sleep(RESET_MS / 1000)
            position = (0, 0)
            post(WM_MOUSEMOVE, 0, 0, 0)
            browser.read()
            for step in steps:
                kind = step[0]
                if kind == "move":
                    position = (step[1], step[2])
                    post(WM_MOUSEMOVE, *position, held["keys"])
                elif kind == "sleep":
                    time.sleep(step[1] / 1000)
                elif kind == "sleep-limit":
                    time.sleep((limit + step[1]) / 1000)
                else:
                    down, up, flag = messages[step[1]]
                    shift = MK_SHIFT if "s" in step[2] else 0
                    if kind == "down":
                        held["keys"] |= flag
                        post(down, *position, held["keys"] | shift)
                    else:
                        held["keys"] &= ~flag
                        post(up, *position, held["keys"] | shift)
                time.sleep(0.01)
            mouse[name] = {"steps": [list(step) for step in steps], "events": browser.read()}
        touch = run_touch(browser)
        return {"mouse": mouse, "touch": touch}, system
    finally:
        browser.close()


def capture_x11(chrome: Path, page: Path, scale: float) -> tuple[dict, dict]:
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
    buttons = {"L": 1, "M": 2, "R": 3}
    shift_key = x11.XKeysymToKeycode(display, 0xFFE1)  # XK_Shift_L
    limit = LINUX_DOUBLE_CLICK_MS

    browser = Browser(chrome, page, scale, ("--ozone-platform=x11", "--no-sandbox"))
    try:
        time.sleep(1.0)  # let the window manager place the window
        left, top, outer_width, outer_height = browser.evaluate(
            "[screenX, screenY, outerWidth, outerHeight]")
        # Screen coordinates are in DIPs; X11 coordinates are device pixels.
        start = (int((left + outer_width / 2) * scale),
                 int((top + outer_height * 2 / 3) * scale))

        def motion(dx, dy):
            xtst.XTestFakeMotionEvent(display, -1, start[0] + dx, start[1] + dy, 0)
            x11.XFlush(display)

        mouse = {}
        for name, steps in MOUSE_SCENARIOS.items():
            time.sleep(RESET_MS / 1000)
            motion(0, 0)
            browser.read()
            for step in steps:
                kind = step[0]
                if kind == "move":
                    motion(step[1], step[2])
                elif kind == "sleep":
                    time.sleep(step[1] / 1000)
                elif kind == "sleep-limit":
                    time.sleep((limit + step[1]) / 1000)
                else:
                    shift = "s" in step[2]
                    if shift and kind == "down":
                        xtst.XTestFakeKeyEvent(display, shift_key, 1, 0)
                    xtst.XTestFakeButtonEvent(display, buttons[step[1]], kind == "down", 0)
                    if shift and kind == "up":
                        xtst.XTestFakeKeyEvent(display, shift_key, 0, 0)
                    x11.XFlush(display)
                time.sleep(0.01)
            mouse[name] = {"steps": [list(step) for step in steps], "events": browser.read()}
        touch = run_touch(browser)
        return {"mouse": mouse, "touch": touch}, {"double_click_time_ms": limit}
    finally:
        browser.close()
        x11.XCloseDisplay(display)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True,
                        help="the pinned Chrome-for-Testing 147.0.7727.50 binary")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    windows = os.name == "nt"
    if windows:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import windows_chrome

        chrome = windows_chrome.verify_chrome(args.chrome)
        name, method = "chromium-windows.json", (
            "WM_MOUSEMOVE and WM_[LMR]BUTTON{DOWN,UP} posted to the pinned browser's "
            "window, wParam key state; touch through CDP")
        capture = capture_windows
    else:
        chrome = args.chrome.resolve()
        name, method = "chromium-linux-x11.json", (
            "X11 buttons, motion and Shift through XTEST, --ozone-platform=x11; "
            "touch through CDP")
        capture = capture_x11

    scales = {}
    system = {}
    with tempfile.TemporaryDirectory(prefix="div-click-page-") as directory:
        page = Path(directory) / "click.html"
        page.write_text(PAGE, encoding="utf-8")
        for scale in SCALES:
            first, system = capture(chrome, page, scale)
            second, _ = capture(chrome, page, scale)
            if first != second:
                raise RuntimeError(f"captures at {scale} differ:\n"
                                   f"{json.dumps(first)}\n{json.dumps(second)}")
            scales[str(scale)] = first

    document = {
        "chromium": CHROMIUM_VERSION,
        "platform": "windows" if windows else "linux-x11",
        "host": platform.platform(),
        "input": method,
        "scale_method": "--force-device-scale-factor (RFC 0010 real zoom)",
        "viewport": [WIDTH, HEIGHT],
        "page": "one position:fixed target filling the viewport; capture-phase window "
                "listeners log each event at the target; contextmenu and middle "
                "mousedown are cancelled so no menu or autoscroll opens",
        "units": "mouse steps in device pixels from the start point; touch steps in CSS "
                 "pixels from the viewport centre; sleep-limit is the double-click time "
                 "plus the offset in ms",
        "reset_ms": RESET_MS,
        "system": system,
        "scales": scales,
    }
    output = args.output or FIXTURE / name
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
