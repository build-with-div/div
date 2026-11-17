#!/usr/bin/env python3
"""Capture pinned Chromium 147 wheel events from real platform wheel input.

CDP's `Input.dispatchMouseEvent` takes wheel deltas in CSS pixels, so it skips
the platform conversion this oracle measures: how many pixels a wheel notch
is, in which direction, in which `deltaMode`, and what Shift and Ctrl do.
This script instead gives a headed browser the platform's own wheel input:

- Windows: `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` posted to the browser window,
  with the key state in `wParam`, as a mouse driver sends them. The pointer
  and the real input queue are not touched. The window is made topmost for
  the capture, because Chromium drops a wheel message whose point is not over
  one of its own windows (`ui::RerouteMouseWheel`).
- Linux: X11 wheel buttons 4 to 7 (and the Shift and Control keys) through
  the XTEST extension on `$DISPLAY`, with the browser on Ozone's X11
  platform. No root is needed; WSLg's X server works.

Each scale runs its own browser launched with `--force-device-scale-factor`,
the real-zoom oracle method of RFC 0010. The page is one scroller filling the
viewport; a passive `wheel` listener on the window records every event, and
the scroller's offsets are read once they settle. Each capture runs twice and
must match. The test `crates/divui/tests/wheel_events.rs` reads the JSON.
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
FIXTURE = ROOT / "tests/fidelity/fixtures/wheel-events-v1"
HARNESS = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
CHROMIUM_VERSION = "147.0.7727.50"
WIDTH, HEIGHT = 480, 320
SCALES = (1.0, 1.25)

PAGE = """<!DOCTYPE html><html><head><style>
html, body { margin: 0; height: 100%; overflow: hidden }
#scroller { position: fixed; inset: 0; overflow: scroll }
#content { width: 4000px; height: 4000px }
</style></head><body><div id="scroller"><div id="content"></div></div><script>
window.wheelLog = [];
addEventListener("wheel", event => wheelLog.push({
  target: event.target.id,
  deltaX: event.deltaX, deltaY: event.deltaY, deltaZ: event.deltaZ,
  deltaMode: event.deltaMode,
  wheelDeltaX: event.wheelDeltaX, wheelDeltaY: event.wheelDeltaY,
  button: event.button, buttons: event.buttons,
  bubbles: event.bubbles, cancelable: event.cancelable,
  shiftKey: event.shiftKey, ctrlKey: event.ctrlKey,
  altKey: event.altKey, metaKey: event.metaKey,
}), {passive: true});
</script></body></html>"""

READ = "JSON.stringify({log: wheelLog.splice(0), scroll: [scroller.scrollLeft, scroller.scrollTop]})"

# Windows steps: (name, message, wheel delta in 120ths of a notch, key state).
WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOUSEHWHEEL = 0x0200, 0x020A, 0x020E
MK_SHIFT, MK_CONTROL = 0x0004, 0x0008
WINDOWS_STEPS = (
    ("down", WM_MOUSEWHEEL, -120, 0),
    ("up", WM_MOUSEWHEEL, 120, 0),
    ("down", WM_MOUSEWHEEL, -120, 0),
    ("half-down", WM_MOUSEWHEEL, -60, 0),
    ("three-down", WM_MOUSEWHEEL, -360, 0),
    ("right", WM_MOUSEHWHEEL, 120, 0),
    ("left", WM_MOUSEHWHEEL, -120, 0),
    ("right", WM_MOUSEHWHEEL, 120, 0),
    ("shift-down", WM_MOUSEWHEEL, -120, MK_SHIFT),
)
# X11 steps: (name, wheel button, modifier keysym or None).
XK_SHIFT_L, XK_CONTROL_L = 0xFFE1, 0xFFE3
X11_STEPS = (
    ("down", 5, None),
    ("up", 4, None),
    ("down", 5, None),
    ("right", 7, None),
    ("left", 6, None),
    ("right", 7, None),
    ("shift-down", 5, XK_SHIFT_L),
)


def harness():
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", HARNESS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Browser:
    """A headed pinned browser showing the page, with a CDP client."""

    def __init__(self, chrome: Path, page: Path, scale: float, extra: tuple[str, ...]):
        self.h = harness()
        self.profile = tempfile.mkdtemp(prefix="div-wheel-oracle-")
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
            if self.evaluate("typeof wheelLog") == "object":
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

    def settle(self) -> dict:
        """The events since the last read, once the scroll offsets settle."""
        time.sleep(0.3)
        previous, stable = None, 0
        for _ in range(50):
            time.sleep(0.1)
            scroll = self.evaluate("[scroller.scrollLeft, scroller.scrollTop]")
            stable = stable + 1 if scroll == previous else 0
            previous = scroll
            if stable >= 3:
                return json.loads(self.evaluate(READ))
        raise RuntimeError("the scroll offsets did not settle")

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


GEOMETRY = ("({viewport: [innerWidth, innerHeight], "
            "scroller_client: [scroller.clientWidth, scroller.clientHeight]})")


def capture_windows(chrome: Path, page: Path, scale: float) -> tuple[dict, dict]:
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    # Per-monitor DPI awareness, so window and screen coordinates are physical.
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    lines, chars = wintypes.UINT(), wintypes.UINT()
    user32.SystemParametersInfoW(0x0068, 0, ctypes.byref(lines), 0)  # SPI_GETWHEELSCROLLLINES
    user32.SystemParametersInfoW(0x006C, 0, ctypes.byref(chars), 0)  # SPI_GETWHEELSCROLLCHARS
    system = {"wheel_scroll_lines": lines.value, "wheel_scroll_chars": chars.value}

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
        local = (client.right // 2, client.bottom * 2 // 3)
        point = wintypes.POINT(*local)
        user32.ClientToScreen(hwnd, ctypes.byref(point))
        # A first move: Chromium drops a wheel message before the pointer
        # has entered the window.
        user32.PostMessageW(hwnd, WM_MOUSEMOVE, 0, (local[1] << 16) | local[0])
        browser.settle()
        steps = []
        for name, message, delta, keys in WINDOWS_STEPS:
            wparam = ((delta & 0xFFFF) << 16) | keys
            lparam = ((point.y & 0xFFFF) << 16) | (point.x & 0xFFFF)
            user32.PostMessageW(hwnd, message, wparam, lparam)
            result = browser.settle()
            steps.append({"name": name, "input": {
                "message": "WM_MOUSEHWHEEL" if message == WM_MOUSEHWHEEL else "WM_MOUSEWHEEL",
                "wheel_delta": delta,
                "shift": bool(keys & MK_SHIFT), "control": bool(keys & MK_CONTROL),
            }, **result})
        return {**browser.evaluate(GEOMETRY), "steps": steps}, system
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
    xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                          ctypes.c_ulong]
    xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = x11.XOpenDisplay(None)
    if not display:
        raise SystemExit("no X display: set DISPLAY")

    browser = Browser(chrome, page, scale, ("--ozone-platform=x11", "--no-sandbox"))
    try:
        time.sleep(1.0)  # let the window manager place the window
        left, top, outer_width, outer_height = browser.evaluate(
            "[screenX, screenY, outerWidth, outerHeight]")
        # Screen coordinates are in DIPs; X11 coordinates are device pixels.
        x = int((left + outer_width / 2) * scale)
        y = int((top + outer_height * 2 / 3) * scale)
        xtst.XTestFakeMotionEvent(display, -1, x, y, 0)
        x11.XFlush(display)
        browser.settle()
        steps = []
        for name, button, keysym in X11_STEPS:
            key = x11.XKeysymToKeycode(display, keysym) if keysym else 0
            if key:
                xtst.XTestFakeKeyEvent(display, key, 1, 0)
            xtst.XTestFakeButtonEvent(display, button, 1, 0)
            xtst.XTestFakeButtonEvent(display, button, 0, 0)
            if key:
                xtst.XTestFakeKeyEvent(display, key, 0, 0)
            x11.XFlush(display)
            result = browser.settle()
            steps.append({"name": name, "input": {
                "button": button,
                "shift": keysym == XK_SHIFT_L, "control": keysym == XK_CONTROL_L,
            }, **result})
        return {**browser.evaluate(GEOMETRY), "steps": steps}, {}
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
            "WM_MOUSEWHEEL/WM_MOUSEHWHEEL posted to the browser window, wParam key state")
        capture = capture_windows
    else:
        chrome = args.chrome.resolve()
        name, method = "chromium-linux-x11.json", (
            "X11 wheel buttons 4-7 and modifier keys through XTEST, --ozone-platform=x11")
        capture = capture_x11

    scales = {}
    system = {}
    with tempfile.TemporaryDirectory(prefix="div-wheel-page-") as directory:
        page = Path(directory) / "wheel.html"
        page.write_text(PAGE, encoding="utf-8")
        for scale in SCALES:
            first, system = capture(chrome, page, scale)
            second, _ = capture(chrome, page, scale)
            if first != second:
                raise RuntimeError(f"captures at {scale} differ:\n{first}\n{second}")
            scales[str(scale)] = first

    document = {
        "chromium": CHROMIUM_VERSION,
        "platform": "windows" if windows else "linux-x11",
        "host": platform.platform(),
        "input": method,
        "scale_method": "--force-device-scale-factor (RFC 0010 real zoom)",
        "viewport": [WIDTH, HEIGHT],
        "page": "one position:fixed overflow:scroll scroller filling the viewport, "
                "4000x4000 content; a passive window wheel listener",
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
