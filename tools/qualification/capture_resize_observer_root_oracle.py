#!/usr/bin/env python3
"""Capture pinned Chromium 147 ResizeObserver entries for the root element.

Div's native document root is both the viewport and the root element, so the
browser element it stands for is `<html>`. Each case styles `<html>` with a
1px border and 5px padding at a 200x100 viewport, gives `<body>` no margin and
puts one block in it, then records the first ResizeObserver entry for
`<html>` with every size, plus `clientWidth` and `clientHeight`. The test
`the_native_root_matches_chromium_for_the_root_element` in
`crates/divui/tests/resize_observer.rs` reads the JSON this script writes.

Only the Windows Chrome-for-Testing build is pinned on Windows hosts; Div's
Windows build is the platform under test.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fidelity/fixtures/resize-observer-root-v1"
WIDTH, HEIGHT = 200, 100
SCALES = (1.0, 1.25)
# (name, html overflow, child block height in CSS pixels)
CASES = (
    ("visible", "visible", 20),
    ("scroll", "scroll", 20),
    ("auto-overflowing", "auto", 500),
)
EXPRESSION = """
new Promise(resolve => {
  const target = document.documentElement;
  const observer = new ResizeObserver(entries => {
    observer.disconnect();
    const entry = entries[0];
    const size = list => ({inline: list[0].inlineSize, block: list[0].blockSize});
    const rect = entry.contentRect;
    resolve({
      content_rect: [rect.x, rect.y, rect.width, rect.height],
      border_box: size(entry.borderBoxSize),
      content_box: size(entry.contentBoxSize),
      device_pixel_content_box: size(entry.devicePixelContentBoxSize),
      client: [target.clientWidth, target.clientHeight],
    });
  });
  observer.observe(target);
})
"""


def page(overflow: str, child_height: int) -> str:
    return (
        "<!DOCTYPE html><html><head><style>"
        f"html{{margin:0;border:1px solid black;padding:5px;overflow:{overflow}}}"
        "body{margin:0}"
        f"div{{width:40px;height:{child_height}px;background:green}}"
        "</style></head><body><div></div></body></html>"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    chrome = args.chrome or windows_chrome.DEFAULT_CHROME
    results = []
    with tempfile.TemporaryDirectory(prefix="div-ro-root-oracle-") as directory:
        for scale in SCALES:
            # Emulated metrics alone scale only the compositor: layout and
            # `devicePixelContentBoxSize` need the browser's own scale.
            force = (f"--force-device-scale-factor={scale}",)
            with windows_chrome.WindowsChrome(chrome, WIDTH, HEIGHT, scale, force) as browser:
                for name, overflow, child_height in CASES:
                    path = Path(directory) / f"{name}.html"
                    path.write_text(page(overflow, child_height), encoding="utf-8")
                    browser.load(path)
                    first = browser.evaluate(EXPRESSION, await_promise=True)
                    again = browser.evaluate(EXPRESSION, await_promise=True)
                    if first != again:
                        raise RuntimeError(f"unstable entry for {name} at {scale}x")
                    results.append({
                        "case": name, "overflow": overflow,
                        "child_height": child_height, "scale": scale, **first,
                    })
                    print(name, scale, first, flush=True)
    payload = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "chrome_sha256": windows_chrome.CHROME_SHA256,
        "platform": "windows",
        "viewport": [WIDTH, HEIGHT],
        "html": "margin:0;border:1px solid black;padding:5px;overflow:<case>",
        "cases": results,
    }
    output = args.output or FIXTURE / "chromium-windows.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {len(results)} entries to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
