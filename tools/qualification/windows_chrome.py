"""Drive pinned Windows Chrome-for-Testing for Windows reference measurements.

Windows Div text uses DirectWrite, so its references come from the
Windows build of the pinned Chromium release rather than the Linux oracle.
This module reuses the Linux harness's dependency-free CDP client and owns the
Windows process lifecycle: every child of the browser is ended before the
temporary profile is removed.
"""

from __future__ import annotations

import base64
from dataclasses import dataclass
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from typing import Callable
import urllib.request


ROOT = Path(__file__).resolve().parents[2]
HARNESS = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
CHROMIUM_VERSION = "147.0.7727.50"
# chrome.exe from storage.googleapis.com/chrome-for-testing-public/147.0.7727.50/win64/chrome-win64.zip
# (archive SHA-256 d014348fe444480c7490de4411fd3110420d659580a868d882bb929eca3522e9).
CHROME_SHA256 = "5a261cd9e0fa71f40fa5df4b9841da4f3311f9175d2e5868c27cb510ccd28505"
DEFAULT_CHROME = ROOT / "out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _harness():
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", HARNESS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The Windows text settings Chromium 147 reads for its text raster
# (ui/gfx/font_render_params_win.cc and ui/gfx/font_util_win.cc).
SPI_GETFONTSMOOTHING = 0x004A
SPI_GETFONTSMOOTHINGTYPE = 0x200A
SPI_GETFONTSMOOTHINGORIENTATION = 0x2012
FE_FONTSMOOTHINGSTANDARD = 0x0001
FE_FONTSMOOTHINGCLEARTYPE = 0x0002
FE_FONTSMOOTHINGORIENTATIONBGR = 0x0000
FE_FONTSMOOTHINGORIENTATIONRGB = 0x0001
TEXT_SETTINGS_KEY = r"SOFTWARE\Microsoft\Avalon.Graphics"
_DISPLAY_DEVICE_PRIMARY_DEVICE = 0x00000004


@dataclass(frozen=True)
class TextRasterSettings:
    """The system settings behind Windows Chromium's text raster.

    A field is ``None`` when Windows did not return it.
    """

    font_smoothing: bool | None
    smoothing_type: int | None
    orientation: int | None
    # The primary display's name as Chromium keys it ("DISPLAY1"), or None.
    primary_display: str | None
    # Whether the primary display has its own key under TEXT_SETTINGS_KEY.
    per_display_key: bool


def _system_parameter(action: int) -> int | None:
    import ctypes
    from ctypes import wintypes

    value = wintypes.UINT(0)
    if not ctypes.windll.user32.SystemParametersInfoW(action, 0, ctypes.byref(value), 0):
        return None
    return value.value


def _primary_display() -> str | None:
    """The primary display device's base name, found the way Chromium finds it."""
    import ctypes
    from ctypes import wintypes

    class DisplayDevice(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("DeviceName", wintypes.WCHAR * 32),
            ("DeviceString", wintypes.WCHAR * 128),
            ("StateFlags", wintypes.DWORD),
            ("DeviceID", wintypes.WCHAR * 128),
            ("DeviceKey", wintypes.WCHAR * 128),
        ]

    device = DisplayDevice()
    device.cb = ctypes.sizeof(DisplayDevice)
    index = 0
    while ctypes.windll.user32.EnumDisplayDevicesW(None, index, ctypes.byref(device), 0):
        if device.StateFlags & _DISPLAY_DEVICE_PRIMARY_DEVICE:
            return display_base_name(device.DeviceName)
        index += 1
    return None


def display_base_name(device_name: str) -> str:
    """The device name's last path component, as base::FilePath::BaseName gives it."""
    return device_name.replace("/", "\\").rstrip("\\").rsplit("\\", 1)[-1]


def _current_user_key_exists(path: str) -> bool:
    import winreg

    try:
        winreg.CloseKey(winreg.OpenKey(winreg.HKEY_CURRENT_USER, path))
    except FileNotFoundError:
        return False
    return True


def read_text_raster_settings(
    system_parameter: Callable[[int], int | None] | None = None,
    primary_display: Callable[[], str | None] | None = None,
    key_exists: Callable[[str], bool] | None = None,
) -> TextRasterSettings:
    """Read the text settings Windows Chromium reads. Tests inject the readers."""
    if None in (system_parameter, primary_display, key_exists) and os.name != "nt":
        raise SystemExit("the Windows text raster settings can only be read on Windows")
    system_parameter = system_parameter or _system_parameter
    primary_display = primary_display or _primary_display
    key_exists = key_exists or _current_user_key_exists
    smoothing = system_parameter(SPI_GETFONTSMOOTHING)
    display = primary_display()
    return TextRasterSettings(
        font_smoothing=None if smoothing is None else bool(smoothing),
        smoothing_type=system_parameter(SPI_GETFONTSMOOTHINGTYPE),
        orientation=system_parameter(SPI_GETFONTSMOOTHINGORIENTATION),
        primary_display=display,
        per_display_key=display is not None and key_exists(f"{TEXT_SETTINGS_KEY}\\{display}"),
    )


def text_raster_pin_failures(settings: TextRasterSettings) -> list[str]:
    """Why these settings differ from the oracle pin; empty when they match.

    Windows Chromium oracles are captured with ClearType on, RGB order and no
    per-display ClearType tuning (RFC 0012). Chromium then uses its build's
    sRGB gamma and contrast 1.0, which
    `RasterConfiguration::chromium_windows_lcd` reproduces.
    """
    failures = []
    if settings.font_smoothing is None:
        failures.append("font smoothing (SPI_GETFONTSMOOTHING) could not be read")
    elif not settings.font_smoothing:
        failures.append("font smoothing (SPI_GETFONTSMOOTHING) is off")
    elif settings.smoothing_type is None:
        failures.append("the font smoothing type (SPI_GETFONTSMOOTHINGTYPE) could not be read")
    elif settings.smoothing_type != FE_FONTSMOOTHINGCLEARTYPE:
        kind = (
            "standard"
            if settings.smoothing_type == FE_FONTSMOOTHINGSTANDARD
            else str(settings.smoothing_type)
        )
        failures.append(
            f"the font smoothing type (SPI_GETFONTSMOOTHINGTYPE) is {kind}, not ClearType"
        )
    if settings.orientation is None:
        failures.append(
            "the ClearType subpixel order (SPI_GETFONTSMOOTHINGORIENTATION) could not be read"
        )
    elif settings.orientation != FE_FONTSMOOTHINGORIENTATIONRGB:
        order = (
            "BGR"
            if settings.orientation == FE_FONTSMOOTHINGORIENTATIONBGR
            else str(settings.orientation)
        )
        failures.append(
            f"the ClearType subpixel order (SPI_GETFONTSMOOTHINGORIENTATION) is {order}, not RGB"
        )
    if settings.per_display_key:
        failures.append(
            f"HKCU\\{TEXT_SETTINGS_KEY}\\{settings.primary_display} exists (ClearType "
            "tuning), so Chromium would take DirectWrite's gamma, contrast and "
            "PixelStructure, which no oracle has measured"
        )
    return failures


def check_text_raster_pin(settings: TextRasterSettings | None = None) -> None:
    """Refuse to capture unless the system matches the oracle's text raster pin."""
    failures = text_raster_pin_failures(settings or read_text_raster_settings())
    if failures:
        raise SystemExit(
            "Windows Chromium oracles need ClearType on, RGB order and no per-display "
            "ClearType tuning (RFC 0012):\n" + "\n".join(f"- {failure}" for failure in failures)
        )


def verify_chrome(chrome: Path) -> Path:
    """Check the pinned browser binary and the system's text raster pin."""
    chrome = chrome.resolve()
    if sha256(chrome.read_bytes()) != CHROME_SHA256:
        raise SystemExit(f"{chrome} is not the pinned Windows Chrome-for-Testing build")
    if not (chrome.parent / f"{CHROMIUM_VERSION}.manifest").is_file():
        raise SystemExit(f"{chrome} lacks the {CHROMIUM_VERSION} manifest")
    check_text_raster_pin()
    return chrome


class WindowsChrome:
    """One headless browser with a fixed viewport at the requested device scale."""

    def __init__(self, chrome: Path, width: int, height: int, scale: float,
                 extra_args: tuple[str, ...] = ()):
        self._harness = _harness()
        self.chrome = verify_chrome(chrome)
        self.width, self.height, self.scale = width, height, scale
        self.extra_args = tuple(extra_args)
        self.process = None
        self.client = None
        self.profile = tempfile.mkdtemp(prefix="openui-win-chrome-")

    def __enter__(self) -> "WindowsChrome":
        # The same launch as the Linux oracle: real zoom at any scale other
        # than 1 (RFC 0010), with CDP fixing only the viewport size.
        scale_flags, self._viewport_override = self._harness.device_scale_launch(
            self.width, self.height, self.scale
        )
        self.process = subprocess.Popen(
            [str(self.chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0",
             f"--user-data-dir={self.profile}",
             *scale_flags, *self.extra_args, "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        h = self._harness
        port = h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
        self.client = h._CdpWebSocket(h._page_websocket_url(port, timeout=30))
        self.client.command("Page.enable")
        self.client.command("Emulation.setDeviceMetricsOverride", self._viewport_override)
        version = self.client.command("Browser.getVersion")
        if CHROMIUM_VERSION not in version.get("product", ""):
            raise SystemExit(f"unexpected browser version: {version}")
        return self

    def load(self, html: Path) -> None:
        url = "file:" + urllib.request.pathname2url(str(html.resolve()))
        self.client.command("Page.navigate", {"url": url})
        self.evaluate(
            "new Promise(resolve => { const done = () => document.fonts.ready.then(() => "
            "requestAnimationFrame(() => requestAnimationFrame(resolve)));"
            "if (document.readyState === 'complete') done();"
            "else addEventListener('load', done, {once: true}); })",
            await_promise=True,
        )
        metrics = self.evaluate("({width: innerWidth, height: innerHeight, dpr: devicePixelRatio})")
        if metrics != {"width": self.width, "height": self.height, "dpr": self.scale}:
            raise RuntimeError(f"viewport emulation failed: {metrics}")

    def evaluate(self, expression: str, await_promise: bool = False):
        result = self.client.command("Runtime.evaluate", {
            "expression": expression, "awaitPromise": await_promise, "returnByValue": True,
        })
        if "exceptionDetails" in result:
            raise RuntimeError(json.dumps(result["exceptionDetails"])[:500])
        return result.get("result", {}).get("value")

    def screenshot(self) -> bytes:
        """A stable PNG of the emulated viewport (two identical captures)."""
        previous = None
        for _ in range(8):
            data = base64.b64decode(self.client.command(
                "Page.captureScreenshot", {"format": "png", "fromSurface": True})["data"])
            if data == previous:
                return data
            previous = data
        raise RuntimeError("screenshots did not stabilize")

    def __exit__(self, *_exc) -> None:
        if self.client is not None:
            self._harness._close_chromium_client(self.client)
        if self.process is not None:
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.process.pid)],
                           capture_output=True)
            self.process.wait(timeout=10)
        for _ in range(50):
            try:
                shutil.rmtree(self.profile)
                return
            except FileNotFoundError:
                return
            except PermissionError:
                time.sleep(0.1)
        raise RuntimeError(f"browser profile writers did not exit: {self.profile}")
