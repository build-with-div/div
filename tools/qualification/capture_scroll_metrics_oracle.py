#!/usr/bin/env python3
"""Capture pinned Chromium 147 scroll dimensions for examples/c/scroll_metrics.c.

The C example checks client and scroll sizes, scroll offsets and bounds at
1x, 1.25x, 1.5x, 2x and 3x. Its expected values above 1x were captured with
CDP-emulated device scale (`Emulation.setDeviceMetricsOverride`), which RFC
0010 no longer accepts for non-1x oracles. This script recaptures each of its
scenarios with a browser launched with `--force-device-scale-factor`, the
real-zoom method, in pinned Windows Chrome-for-Testing 147.0.7727.50. The
override it still sets only sizes the viewport, at the forced scale.

Div's native root is the viewport and the root element, so the browser's
`<html>` (the scrolling element) stands for it, with `<body>` margins zero.
Each scenario records what the web can observe: `clientWidth`,
`clientHeight`, `scrollWidth`, `scrollHeight`, `scrollLeft`, `scrollTop` and
`getBoundingClientRect()`. Every query runs twice and must match. The test
`crates/divui/tests/scroll_metrics_oracle.rs` reads the JSON.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fidelity/fixtures/scroll-metrics-v1/chromium-windows.json"
SCALES = (1.0, 1.25, 1.5, 2.0, 3.0)
WIDTH, HEIGHT = 320, 240

PAGE = """<!DOCTYPE html><html><head><style>
html, body { margin: 0; padding: 0 }
</style></head><body>{body}</body></html>"""

# Shared helpers, declared inside each scenario's function.
HELPERS = """
const dims = e => ({client: [e.clientWidth, e.clientHeight],
                    scroll: [e.scrollWidth, e.scrollHeight]});
const offset = e => [e.scrollLeft, e.scrollTop];
const rect = e => { const r = e.getBoundingClientRect();
                    return [r.x, r.y, r.width, r.height]; };
const settle = () => new Promise(resolve =>
  requestAnimationFrame(() => requestAnimationFrame(resolve)));
const html = document.documentElement;
"""

# scroll_metrics.c `viewport`: the root with `overflow: auto` and absolutely
# positioned content at (27, 22), 300x224.
VIEWPORT_BODY = ('<div id="content" style="position:absolute;left:27px;top:22px;'
                 'width:300px;height:224px"></div>')
VIEWPORT = """
(async () => {""" + HELPERS + """
  const content = document.getElementById("content");
  const steps = {};
  html.style.overflowX = "auto"; html.style.overflowY = "auto";
  await settle();
  steps.original = dims(html);
  html.scrollTo(1000, 1000); await settle();
  steps.original_scrolled = offset(html);
  content.style.width = "80px"; content.style.height = "40px"; await settle();
  steps.small_offset = offset(html);
  steps.small = dims(html);
  html.style.overflowX = "scroll"; html.style.overflowY = "hidden"; await settle();
  steps.asymmetric = dims(html);
  html.style.overflowY = "scroll"; await settle();
  steps.forced = dims(html);
  html.style.overflowX = "hidden"; html.style.overflowY = "hidden";
  content.style.width = "300px"; content.style.height = "224px"; await settle();
  steps.hidden = dims(html);
  html.scrollTo(1000, 1000); await settle();
  steps.hidden_scrolled = offset(html);
  return steps;
})()
"""
# After VIEWPORT the viewport is resized to 360x260 and RESIZED runs.
RESIZED = """
(async () => {""" + HELPERS + """
  await settle(); return {resized_offset: offset(html), resized: dims(html)}; })()
"""

# scroll_metrics.c `nested`: an 80x80 box with 10px padding, a 3px border,
# `overflow: hidden` and no scrollbars, holding a 130x120 child.
NESTED_BODY = ('<div id="port" style="position:absolute;left:40px;top:40px;width:80px;'
               'height:80px;padding:10px;border:3px solid orange;overflow:hidden;'
               'scrollbar-width:none"><div id="child" style="width:130px;height:120px">'
               '</div></div>')
NESTED = """
(async () => {""" + HELPERS + """
  const port = document.getElementById("port");
  const child = document.getElementById("child");
  const steps = {};
  await settle();
  steps.original = dims(port);
  port.scrollTo(1000, 1000); await settle();
  steps.scrolled = offset(port);
  steps.child_bounds = rect(child);
  port.style.direction = "rtl"; port.scrollTo(-1000, 1000); await settle();
  steps.rtl_scrolled = offset(port);
  child.style.width = "30px"; child.style.height = "20px"; await settle();
  steps.small_offset = offset(port);
  steps.small = dims(port);
  port.style.width = "140px"; port.style.height = "140px"; await settle();
  steps.resized = dims(port);
  return steps;
})()
"""

# scroll_metrics.c `integer_api`: fractional sizes with a 2px border and
# `overflow: scroll`, then a transform, then `overflow: hidden`; and a
# fractional inline-block.
INTEGER_BODY = ('<div id="scroller" style="width:100.5px;height:80.5px;border:2px solid black;'
                'overflow:scroll;scrollbar-width:auto"><div style="width:200.5px;'
                'height:160.5px"></div></div><span id="inline" '
                'style="display:inline-block;width:40.5px;height:20.5px"></span>')
INTEGER = """
(async () => {""" + HELPERS + """
  const scroller = document.getElementById("scroller");
  const inline = document.getElementById("inline");
  const steps = {};
  await settle();
  steps.root = dims(html);
  steps.scroll = dims(scroller);
  scroller.style.transform = "scale(2,3)"; await settle();
  steps.transformed = dims(scroller);
  steps.transformed_bounds = rect(scroller);
  scroller.style.overflow = "hidden"; await settle();
  steps.hidden = dims(scroller);
  steps.inline_block = dims(inline);
  return steps;
})()
"""


def run(browser, directory: Path, name: str, body: str, expression: str) -> dict:
    page = directory / f"{name}.html"
    page.write_text(PAGE.replace("{body}", body), encoding="utf-8")
    browser.load(page)
    return browser.evaluate(expression, await_promise=True)


def capture(chrome: Path, scale: float, directory: Path) -> dict:
    import windows_chrome

    force = (f"--force-device-scale-factor={scale}",)
    result = {}
    with windows_chrome.WindowsChrome(chrome, WIDTH, HEIGHT, scale, force) as browser:
        result["viewport"] = run(browser, directory, "viewport", VIEWPORT_BODY, VIEWPORT)
        browser.client.command("Emulation.setDeviceMetricsOverride", {
            "width": 360, "height": 260, "deviceScaleFactor": scale, "mobile": False,
            "screenWidth": 360, "screenHeight": 260,
        })
        result["viewport"].update(browser.evaluate(RESIZED, await_promise=True))
        browser.client.command("Emulation.setDeviceMetricsOverride", {
            "width": WIDTH, "height": HEIGHT, "deviceScaleFactor": scale, "mobile": False,
            "screenWidth": WIDTH, "screenHeight": HEIGHT,
        })
        result["nested"] = run(browser, directory, "nested", NESTED_BODY, NESTED)
        result["integer_api"] = run(browser, directory, "integer", INTEGER_BODY, INTEGER)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=FIXTURE)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    chrome = args.chrome or windows_chrome.DEFAULT_CHROME
    scales = {}
    with tempfile.TemporaryDirectory(prefix="div-scroll-metrics-oracle-") as directory:
        for scale in SCALES:
            first = capture(chrome, scale, Path(directory))
            second = capture(chrome, scale, Path(directory))
            if first != second:
                raise RuntimeError(f"captures at {scale} differ:\n{first}\n{second}")
            scales[str(scale)] = first
    document = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "scale_method": "--force-device-scale-factor (RFC 0010 real zoom)",
        "viewport": [WIDTH, HEIGHT],
        "source": "examples/c/scroll_metrics.c: viewport, nested and integer_api",
        "scales": scales,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
