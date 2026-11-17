"""Record how pinned Chromium 147 reads `style` attributes.

Each style case is a `style` attribute on its own `div`. Through CSSOM,
Chromium reports, for each probe property, the value and priority it kept
(an empty value when it kept nothing). The native test
`div-engine/tests/style_attribute.rs` parses every case with Div's
browser-compatible parser and requires the same declarations, the same
importance and the same values, except where the test names a Div gap.

Windows only: it uses the pinned Windows Chrome-for-Testing.
"""

from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "tests/fidelity/fixtures/style-attribute-v1/chromium-windows.json"

PROBES = [
    "color",
    "background-color",
    "display",
    "opacity",
    "padding",
    "width",
    "font-size",
    "font-weight",
    "font-family",
]

# Keep in step with the native test, which names the cases Div reads
# differently (its gaps) by index.
STYLE_CASES = [
    "color: red",
    "color: red; display: flex; opacity: 0.5",
    "COLOR: Red; Display: FLEX",
    "color: red; colr: blue; display: flex",
    "display: sideways; color: red",
    "color: red; color: blue",
    "color: red !important; color: blue",
    "color: red; color: blue !important",
    "color: red !IMPORTANT",
    "color: red ! important",
    "color: red !ie; display: flex",
    "color: red !important !important; display: flex",
    "color /* c */ : /* c */ red /* c */ ; display: flex",
    "color: red; display: flex /* open",
    'font-family: "a;b"; color: red',
    'font-family: "open; color: red',
    "font-family: 'a\nb'; color: red",
    "color: rgb(255, 0, 0; display: flex",
    "color: rgb(255, 0, 0",
    "color: red); display: flex",
    "color: red; } display: flex",
    "{color: red}; display: flex",
    "@foo; color: red",
    "@foo { x: y } color: red; display: flex",
    "color:red;display:flex",
    ";;color: red;;",
    "color",
    "color:; display: flex",
    ": red; display: flex",
    "color red; display: flex",
    "--x: 1; color: red",
    "color: var(--x); display: flex",
    "color: inherit; display: flex",
    "padding: 8px 16px",
    "padding: 8px 16px 4px 2px 1px; display: flex",
    "width: 10; display: flex",
    "width: 0",
    "width: 10 px; display: flex",
    "width: 10PX",
    "font-size: 12px; font-weight: bold",
    "color:\n\tred;\n display:\tflex",
    "color: red !important; color: blue !important",
    "background-color: #ff000080",
    "display: flex; display: grid; display: bogus",
    "color: red /**/ !important",
    "1color: red; display: flex",
    "color: red;; ; display: flex",
    "opacity: .5",
    "opacity: 5e-1",
    "width: 1e1px",
    "width: 10.px; display: flex",
    "opacity: 50%",
    "opacity: normal; display: flex",
    "opacity: 0.5e; display: flex",
]

SCRIPT = """
(() => {
  const probes = %s;
  const style = [...document.querySelectorAll('[data-style]')].map(element => {
    const kept = {};
    for (const name of probes) {
      kept[name] = [element.style.getPropertyValue(name),
                    element.style.getPropertyPriority(name)];
    }
    return kept;
  });
  return JSON.stringify({style});
})()
"""


def page() -> str:
    divs = "".join(
        f'<div data-style style="{html.escape(text, quote=True)}"></div>' for text in STYLE_CASES
    )
    return f"<!DOCTYPE html><html><body>{divs}</body></html>"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    with tempfile.TemporaryDirectory(prefix="div-style-attribute-oracle-") as directory:
        page_path = Path(directory) / "style-attribute.html"
        page_path.write_text(page(), encoding="utf-8")
        with windows_chrome.WindowsChrome(
            args.chrome or windows_chrome.DEFAULT_CHROME, 200, 200, 1.0
        ) as browser:
            browser.load(page_path)
            measured = json.loads(browser.evaluate(SCRIPT % json.dumps(PROBES)))
    style = []
    for text, kept in zip(STYLE_CASES, measured["style"], strict=True):
        style.append(
            {
                "style": text,
                "kept": {
                    name: {"value": value, "important": priority == "important"}
                    for name, (value, priority) in kept.items()
                    if value
                },
            }
        )
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "probes": PROBES,
        "style": style,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(style)} style cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
