#!/usr/bin/env python3
"""Write the multi-run width trace of `tests/fidelity/fixtures/multi-run-width-v1`.

Blink keeps each run's width as a float and adds those floats: in the order it
inserted the runs while shaping, and in visual order after spacing. Float
addition depends on the order, so the words here put several runs in one
result five ways:

- `ltr-scripts`: left to right, one segment per script (Latin, Cyrillic), with
  a fallback font for the second script;
- `ltr-fallback`: left to right, one Latin segment whose middle letters only
  the second font has, so the first font's runs are inserted first;
- `rtl-scripts`: right to left, Hebrew and Arabic segments;
- `rtl-fallback`: right to left, one Hebrew segment whose full stops come from
  the first font and whose letters come from the second;
- `spacing`: the left-to-right and `rtl-fallback` words with letter spacing,
  positive and negative. Arabic is left out: Blink adds no letter spacing to
  cursive scripts, which is a separate repair.

Usage: python3 tools/canvas/generate_multi_run_width_trace.py
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "tests/fidelity/fixtures/multi-run-width-v1/traces/multi-run-width.jsonl"
SIZES = ["13.7px", "17.3px", "33.3px", "51.9px", "97.1px"]
SHAPES = [(1, 1, 1), (3, 2, 5), (7, 1, 9), (12, 4, 3), (20, 1, 20), (31, 7, 11), (2, 30, 2)]
SPACINGS = ["1.7px", "0.3px", "-0.3px", "-40px"]


def line(**fields):
    return json.dumps(fields, ensure_ascii=False, separators=(",", ":"))


def words(first, second):
    """Three-part and four-part words from two letters."""
    for a, b, c in SHAPES:
        yield first * a + second * b + first * c
        yield second * a + first * b + second * c + first


def measure(lines, family, first, second, direction="ltr", spacing="0px", sizes=SIZES):
    lines.append(line(canvas="c", set="direction", value=direction))
    lines.append(line(canvas="c", set="letterSpacing", value=spacing))
    for size in sizes:
        lines.append(line(canvas="c", set="font", value=f"{size} {family}"))
        for word in words(first, second):
            lines.append(line(canvas="c", call="measureText", args=[word]))


def main():
    lines = [
        line(fontFace="A", file="Ahem.ttf"),
        line(fontFace="S", file="DejaVuSans.ttf"),
        line(fontFace="R", file="DejaVuSerif.ttf"),
        line(canvas="c", width=8, height=8),
    ]
    measure(lines, "A, S", "X", "Ж")  # ltr-scripts
    measure(lines, "A, S", "X", "ł")  # ltr-fallback
    measure(lines, "R, S", "א", "ب", direction="rtl")  # rtl-scripts
    measure(lines, "A, S", "א", ".", direction="rtl")  # rtl-fallback
    for spacing in SPACINGS:
        measure(lines, "A, S", "X", "Ж", spacing=spacing, sizes=SIZES[2:4])
        measure(lines, "A, S", "X", "ł", spacing=spacing, sizes=SIZES[2:4])
        measure(lines, "A, S", "א", ".", direction="rtl", spacing=spacing, sizes=SIZES[2:4])
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(OUTPUT, len(lines))


if __name__ == "__main__":
    main()
