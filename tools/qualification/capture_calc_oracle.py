"""Record how pinned Chromium 147 reads and resolves `calc()` in `line-height`.

Each case is a `line-height` value at one font size, set on its own block
holding one line of text. Chromium reports:

- `declared`: the inline style's serialization of the value, or an empty
  string when it dropped the declaration;
- `computed`: `getComputedStyle().lineHeight`;
- `height`: the block's `getBoundingClientRect().height`, its used line
  height in layout units, which the font's integer metrics can round;
- `lh`: the height of an empty block with the same style holding a child
  of `height: 1lh`, the computed line height in layout units.

Each case sits in its own 80px wrapper, so no case starts at a fractional
position.

The native test `divui/tests/calc_oracle.rs` parses every case with Div,
lays out the same blocks and requires the same acceptance and the same
heights, except where the test names a Div gap.

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
OUTPUT = ROOT / "tests/fidelity/fixtures/calc-v1/chromium-windows.json"

# Tailwind v4's text-* line heights at their font sizes, as shadcn writes them.
TAILWIND = [
    (12, "calc(1 / 0.75)"),
    (14, "calc(1.25 / 0.875)"),
    (16, "calc(1.5 / 1)"),
    (18, "calc(1.75 / 1.125)"),
    (20, "calc(1.75 / 1.25)"),
    (24, "calc(2 / 1.5)"),
    (30, "calc(2.25 / 1.875)"),
    (36, "calc(2.5 / 2.25)"),
]

# Values checked at each of these font sizes.
FONT_SIZES = [14, 16, 13.5, 15.75]
VALUES = [
    # Literals, for the paths calc() shares.
    "1.4285714",
    "1.25",
    "1.15",
    "1.375",
    "20px",
    "142.857%",
    "150%",
    "1.5em",
    # Numbers.
    "calc(1.5)",
    "calc(1 + 0.5)",
    "calc(3 * 0.5)",
    "calc(0.5 * 3)",
    "calc(1 / 3)",
    "calc(10 / 7)",
    "calc(1.25 / 0.875 * 1)",
    "calc((1 + 0.25) / 0.875)",
    "calc(2 - 0.5 - 0.25)",
    "calc(1 - (0.5 - 0.25))",
    "calc(calc(1.25) / 0.875)",
    "CALC(1.5)",
    "calc( 1.5 )",
    "calc(1e0 + 5e-1)",
    "calc(-1)",
    "calc(0)",
    # Lengths.
    "calc(20px)",
    "calc(10px + 10px)",
    "calc(10px * 1.5)",
    "calc(2 * 10px)",
    "calc(30px / 1.5)",
    "calc(1in / 3)",
    "calc(1pt + 1px)",
    "calc(15pt)",
    "calc(1em)",
    "calc(1.25em)",
    "calc(1em + 4px)",
    "calc(4px + 1em)",
    "calc(1em - 2px)",
    "calc(1em + 0.5rem)",
    "calc(2em / 3)",
    "calc(1.25rem)",
    "calc(1em + 2px + 1em)",
    "calc(2px + 1em - 1px)",
    "calc((1em + 2px) * 1.5)",
    "calc(1.5 * (1em + 2px))",
    "calc(1px - (1em - 20px))",
    "calc(-5px)",
    "calc(1em - 30px)",
    # Percentages.
    "calc(150%)",
    "calc(100% / 3)",
    "calc(50% * 3)",
    "calc(142.857%)",
    "calc(100% + 49.5%)",
    "calc(-10%)",
    # Lengths and percentages together.
    "calc(100% + 2px)",
    "calc(2px + 100%)",
    "calc(50% + 0.5em)",
    "calc(100% / 3 + 1px)",
    "calc(1em + 10%)",
    "calc(100% - 1px)",
    "calc(110% - 3px + 0.25em)",
    "calc(10% - 40px)",
    # Invalid.
    "calc(1 + 1px)",
    "calc(1px + 1)",
    "calc(1% + 1)",
    "calc(1px * 2px)",
    "calc(1px / 2px)",
    "calc(2 / 1px)",
    "calc(1px+2px)",
    "calc(1px +2px)",
    "calc(1px -2px)",
    "calc(1px - 2px)",
    "calc()",
    "calc(1px",
    "calc(1px))",
    "calc(1px 2px)",
    "calc(1deg)",
    "calc(1px * (2))",
    "calc(1 / 0)",
    "calc(1px / 0)",
    "calc((1px + 1em) / 0.5)",
]

SCRIPT = """
JSON.stringify([...document.querySelectorAll('[data-case]')].map(element => [
  element.style.lineHeight,
  getComputedStyle(element).lineHeight,
  element.getBoundingClientRect().height,
  element.nextElementSibling.getBoundingClientRect().height,
]))
"""


def cases() -> list[tuple[float, str]]:
    result = [(float(size), value) for size, value in TAILWIND]
    for size in FONT_SIZES:
        result.extend((float(size), value) for value in VALUES)
    return result


def page() -> str:
    blocks = []
    for size, value in cases():
        style = f"font-size: {size}px; line-height: {html.escape(value, quote=True)}"
        blocks.append(
            f'<div class="case"><div data-case style="{style}">x</div>'
            f'<div style="{style}"><div style="height: 1lh"></div></div></div>'
        )
    blocks = "".join(blocks)
    return (
        "<!DOCTYPE html><html><head><style>"
        "body { margin: 0; font-family: Arial; } .case { height: 80px; }"
        "</style></head><body>" + blocks + "</body></html>"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    with tempfile.TemporaryDirectory(prefix="div-calc-oracle-") as directory:
        page_path = Path(directory) / "calc.html"
        page_path.write_text(page(), encoding="utf-8")
        with windows_chrome.WindowsChrome(
            args.chrome or windows_chrome.DEFAULT_CHROME, 400, 300, 1.0
        ) as browser:
            browser.load(page_path)
            measured = json.loads(browser.evaluate(SCRIPT))
    rows = []
    for (size, value), (declared, computed, height, lh) in zip(cases(), measured, strict=True):
        rows.append(
            {
                "font_size": size,
                "line_height": value,
                "declared": declared,
                "computed": computed,
                "height": height,
                "lh": lh,
            }
        )
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "font_family": "Arial",
        "cases": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(rows)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
