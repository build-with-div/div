#!/usr/bin/env python3
"""Measure pinned Chrome for the platform-specific text tests.

Each scenario reproduces one test as HTML and records the values the test
asserts, at every scale the test covers. Scales other than 1 use real zoom
(`--force-device-scale-factor`, RFC 0010), the launch the census oracle uses.

`--platform windows` (the default) drives pinned Windows Chrome-for-Testing.
Tests that use Div's deterministic aliased raster configuration are measured
with Blink's NoFontAntialiasing feature, and Ahem is loaded with @font-face
from the repository bytes; it is never installed system-wide.

`--platform linux` drives pinned Linux Chrome-for-Testing (WSL here) for the
scenarios that also have Linux values. Aliased Ahem comes from the census
oracle's no-antialiasing Fontconfig (`ahem_noaa.conf`, which registers the
repository's Ahem), as the Linux oracle draws it, instead of a web font:
Blink draws an @font-face Ahem with the default antialiased style.
"""

from __future__ import annotations

import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import windows_chrome  # noqa: E402


ROOT = windows_chrome.ROOT
AHEM = ROOT / "crates/div-text/fonts/Ahem.ttf"
SCALES = (1.0, 1.25, 1.5, 2.0, 3.0)
ALIASED = ("--enable-blink-features=NoFontAntialiasing",)
LINUX_CHROME = Path.home() / "openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome"


def page(body: str, extra_style: str = "", web_font: bool = True) -> str:
    face = ""
    if web_font:
        font = base64.b64encode(AHEM.read_bytes()).decode("ascii")
        face = f"@font-face{{font-family:Ahem;src:url(data:font/truetype;base64,{font})}}"
    return (
        "<!DOCTYPE html><html><head><meta charset=utf-8><style>"
        f"{face}html,body{{margin:0}}{extra_style}</style></head><body>{body}</body></html>"
    )


class LinuxChrome(windows_chrome.WindowsChrome):
    """The same session on pinned Linux Chrome-for-Testing.

    The census oracle's environment: the no-antialiasing Fontconfig when the
    scenario is aliased, with the shared real-zoom launch.
    """

    def __init__(self, chrome: Path, width: int, height: int, scale: float,
                 extra_args: tuple[str, ...] = ()):
        self._harness = windows_chrome._harness()
        self.chrome = chrome.resolve()
        self.width, self.height, self.scale = width, height, scale
        self.aliased = tuple(extra_args) == ALIASED
        self.extra_args = ()
        self.process = None
        self.client = None
        self.profile = tempfile.mkdtemp(prefix="div-linux-chrome-")

    def __enter__(self) -> "LinuxChrome":
        h = self._harness
        scale_flags, self._viewport_override = h.device_scale_launch(
            self.width, self.height, self.scale
        )
        self.process = subprocess.Popen(
            [str(self.chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0",
             f"--user-data-dir={self.profile}", *scale_flags, "about:blank"],
            env=h.chrome_environment(str(self.chrome.parent), use_ahem_noaa=self.aliased),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
        )
        port = h._wait_for_devtools_endpoint(self.profile, self.process, timeout=30)
        self.client = h._CdpWebSocket(h._page_websocket_url(port, timeout=30))
        self.client.command("Page.enable")
        self.client.command("Emulation.setDeviceMetricsOverride", self._viewport_override)
        version = self.client.command("Browser.getVersion")
        if windows_chrome.CHROMIUM_VERSION not in version.get("product", ""):
            raise SystemExit(f"unexpected browser version: {version}")
        return self

    def __exit__(self, *_exc) -> None:
        if self.client is not None:
            self._harness._close_chromium_client(self.client)
        if self.process is not None:
            self._harness._stop_chromium_process(self.process)
        for _ in range(50):
            try:
                shutil.rmtree(self.profile)
                return
            except FileNotFoundError:
                return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError(f"browser profile writers did not exit: {self.profile}")


WIDTH = "(id) => document.getElementById(id).getBoundingClientRect().width"
NEXT_FRAME = "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r(%s))))"


def atomic_word(chrome: windows_chrome.WindowsChrome) -> dict:
    rect = chrome.evaluate("(() => { const b = document.getElementById('w').getBoundingClientRect();"
                           " return [b.x, b.y, b.width, b.height]; })()")
    return {"word_rect": rect}


def font_units(chrome: windows_chrome.WindowsChrome) -> dict:
    js = f"[0,1,2,3].map(i => ({WIDTH})('d' + i))"
    before = chrome.evaluate(js)
    chrome.evaluate("document.body.style.fontSize = '20.37px'")
    return {"before": before, "after": chrome.evaluate(NEXT_FRAME % js, await_promise=True)}


def line_height_inheritance(chrome: windows_chrome.WindowsChrome) -> dict:
    js = f"[0,1,2,3,4,5].map(i => [({WIDTH})('n' + i), ({WIDTH})('c' + i)])"
    before = chrome.evaluate(js)
    chrome.evaluate("document.getElementById('p').style.fontSize = '20.37px'")
    return {"before": before, "after": chrome.evaluate(NEXT_FRAME % js, await_promise=True)}


def line_height_sequence(chrome: windows_chrome.WindowsChrome) -> dict:
    steps = [
        "",
        "n.style.font = '24px/1.5 Ahem'",
        "n.style.lineHeight = '2.5ex'",
        "n.style.lineHeight = '3.75ex'",
        "n.style.fontSize = '32px'",
        "n.style.lineHeight = '2.5ex'",
    ]
    values = []
    for step in steps:
        chrome.evaluate(f"(() => {{ const n = document.getElementById('n'); {step}; }})()")
        values.append(chrome.evaluate(NEXT_FRAME % f"({WIDTH})('n')", await_promise=True))
    return {"widths": values}


def block_strut(chrome: windows_chrome.WindowsChrome) -> dict:
    return {"height": chrome.evaluate("document.getElementById('p').getBoundingClientRect().height")}


def vertical_flex(chrome: windows_chrome.WindowsChrome) -> dict:
    return {"line_block_size": chrome.evaluate(f"({WIDTH})('i0')")}


ATOMIC_BODY = (
    '<div style="position:absolute;left:10px;top:485px;width:75px">'
    '<span id=w style="display:inline-block;font:16px Ahem">ABCDEFGHI</span></div>'
)
UNIT_CASES = [("2.5ch", "normal"), ("2.5ex", "normal"), ("2.5lh", "1.2"), ("2.5lh", "127.5%")]
UNITS_BODY = "".join(
    f'<div id=d{i} style="position:absolute;left:0;top:0;height:10px;width:{w};line-height:{lh}"></div>'
    for i, (w, lh) in enumerate(UNIT_CASES)
)
LH_UNITS = ["2ch", "2ex", "2lh", "1.5em", "1.5rem", "127.5%"]
LH_BODY = '<div id=p style="font:18.72px/1.25 Ahem">' + "".join(
    f'<div id=n{i} style="width:2.5lh;line-height:{u};font-size:30.41px">'
    f'<div id=c{i} style="font-size:11px;width:2.5lh"></div></div>'
    for i, u in enumerate(LH_UNITS)
) + "</div>"
SEQUENCE_BODY = '<div id=n style="font:20px Ahem;width:2lh;line-height:2ch"></div>'
STRUT_BODY = (
    '<div id=p style="width:200px;font-family:sans-serif">'
    '<div style="float:left;width:50px;height:20px"></div>'
    '<br style="clear:both"><br style="clear:both"></div>'
)
# The flex test's content-based size is the vertical-lr line block size: the
# default-font strut, since both 15px atomics are narrower than it.
FLEX_BODY = (
    '<div id=i0 style="writing-mode:vertical-lr;width:max-content;font:16px sans-serif">'
    '<span style="display:inline-block;width:15px;height:95px"></span>'
    '<span style="display:inline-block;width:15px;height:95px"></span></div>'
)

SCENARIOS = {
    # test path::function -> (body, body style, chrome flags, measure, scales)
    "crates/divui/tests/native_font_units.rs::atomic_intrinsic_text_keeps_chromium_fraction_after_native_font_alias_change":
        (ATOMIC_BODY, "", ALIASED, atomic_word, SCALES),
    "crates/divui/tests/native_font_units.rs::retained_font_units_update_after_native_rust_callbacks":
        (UNITS_BODY, "body{font:18.72px Ahem}", ALIASED, font_units, SCALES),
    "crates/divui/tests/native_line_height.rs::relative_line_height_updates_from_native_callbacks_and_inherits_fixed_values":
        (LH_BODY, "body{font-size:16px}", ALIASED, line_height_inheritance, SCALES),
    "crates/divui/tests/native_line_height.rs::relative_line_height_follows_shorthand_order_pseudos_and_animation_cancel":
        (SEQUENCE_BODY, "", ALIASED, line_height_sequence, (1.0,)),
    "crates/div-layout/src/block.rs::consecutive_clearing_break_without_intervening_float_keeps_line_strut":
        (STRUT_BODY, "", (), block_strut, (1.0,)),
    "crates/div-layout/src/flex/algorithm.rs::vertical_column_cross_margin_reduces_fit_content_before_wrapping":
        (FLEX_BODY, "", (), vertical_flex, (1.0,)),
}


# Scenarios with Linux values: the divui tests that assert Chromium values on
# both platforms. The div-layout scenarios are Windows-only.
LINUX_SCENARIOS = tuple(test for test in SCENARIOS if test.startswith("crates/divui/"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--platform", choices=("windows", "linux"), default="windows")
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    linux = args.platform == "linux"
    if linux:
        chrome_path = (args.chrome or LINUX_CHROME).resolve()
        session = LinuxChrome
        tests = LINUX_SCENARIOS
        chrome_sha256 = windows_chrome.sha256(chrome_path.read_bytes())
    else:
        chrome_path = args.chrome or windows_chrome.DEFAULT_CHROME
        session = windows_chrome.WindowsChrome
        tests = tuple(SCENARIOS)
        chrome_sha256 = windows_chrome.CHROME_SHA256
    harness = windows_chrome._harness()
    receipt = {
        "schema_version": 2,
        "platform": args.platform,
        "chromium_version": windows_chrome.CHROMIUM_VERSION,
        "chrome_sha256": chrome_sha256,
        "tool_sha256": windows_chrome.sha256(Path(__file__).read_bytes()),
        "session_sha256": windows_chrome.sha256(Path(windows_chrome.__file__).read_bytes()),
        "launcher_sha256": windows_chrome.sha256(windows_chrome.HARNESS.read_bytes()),
        "ahem_sha256": windows_chrome.sha256(AHEM.read_bytes()),
        "tests": {},
    }
    with tempfile.TemporaryDirectory(prefix="div-text-oracle-") as temporary:
        for test in tests:
            body, style, flags, measure, scales = SCENARIOS[test]
            # Linux draws aliased Ahem from Fontconfig, not from a web font.
            web_font = not (linux and flags == ALIASED)
            html_text = page(body, style, web_font)
            html = Path(temporary) / "page.html"
            html.write_text(html_text, encoding="utf-8", newline="\n")
            rows = {}
            methods = {}
            for scale in scales:
                repeats = []
                for _ in range(2):
                    with session(chrome_path, 800, 600, scale, flags) as chrome:
                        chrome.load(html)
                        repeats.append(measure(chrome))
                if repeats[0] != repeats[1]:
                    raise SystemExit(f"{test} at {scale}: repeated measurements differ")
                rows[format(scale, "g")] = repeats[0]
                methods[format(scale, "g")] = harness.device_scale_method(scale)
            receipt["tests"][test] = {
                "html_sha256": windows_chrome.sha256(html_text.encode("utf-8")),
                "chrome_flags": [] if linux else list(flags),
                "fontconfig": ("ahem_noaa.conf" if flags == ALIASED else "system") if linux else None,
                "device_scale_method": methods,
                "measurements": rows,
            }
            print(test.rsplit("::", 1)[1], json.dumps(rows), flush=True)
    args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n",
                           encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
