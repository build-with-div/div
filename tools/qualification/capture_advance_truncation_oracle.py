"""Record pinned Chromium 147's widths of long single-font texts (T3).

Blink hands HarfBuzz each glyph advance truncated to 1/65536 px, and sets
the HarfBuzz scale from the font size the same way; Skia's SkShaper rounds
both. One glyph moves by at most 1/65536 px, so each case repeats a short
text until those units add up to layout units: `n` alone (no kerning),
the kerned pairs `AV` and `To`, and a sentence. Each case is one
inline-block span (layout-unit width, `white-space: pre`) at one CSS size.
The native test `div-engine/tests/advance_truncation.rs` requires Div's
span widths to equal these.

The fonts are web fonts from this repository, so the test needs no system
font. Windows only: it uses the pinned Windows Chrome-for-Testing, whose
DirectWrite advances are not on 1/64 px. Black-box measurements.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "tests/fidelity/fixtures/advance-truncation-v1/chromium-windows.json"

FONTS = {
    "DejaVu Sans": "crates/div-text/fonts/DejaVuSans.ttf",
    "Geist": "tests/fidelity/components/assets/fonts/geist/Geist-Variable.woff2",
}
# (name, repeated unit, repetitions)
TEXTS = [
    ("n", "n", 1000),
    ("AV", "AV", 500),
    ("To", "To", 500),
    ("sentence", "Sphinx of black quartz, judge my vow. ", 25),
]
SIZES = [
    "10", "10.37", "11.37", "12", "12.5", "13", "13.33", "14.2", "15.3", "16",
    "16.05", "16.66", "17.71", "18.66", "20", "21.13", "24", "25.29", "28.88", "32",
]

SCRIPT = """
[...document.querySelectorAll('[data-size]')].map(span => span.getBoundingClientRect().width)
"""


def page(font: str, text: str) -> str:
    source = (ROOT / FONTS[font]).resolve().as_uri()
    spans = "".join(
        f'<div><span data-size style="display: inline-block; white-space: pre; '
        f'font: {size}px Probe">{text}</span></div>'
        for size in SIZES
    )
    return (f"<!DOCTYPE html><style>@font-face {{ font-family: Probe; src: url('{source}'); }}"
            f" body {{ margin: 0 }}</style>{spans}")


def measure(chrome: Path, font: str, text: str) -> list[float]:
    import windows_chrome

    with tempfile.TemporaryDirectory(prefix="div-advance-oracle-") as directory:
        path = Path(directory) / "advances.html"
        path.write_text(page(font, text), encoding="utf-8")
        with windows_chrome.WindowsChrome(chrome, 800, 600, 1.0) as browser:
            browser.load(path)
            return browser.evaluate(SCRIPT)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    chrome = args.chrome or windows_chrome.DEFAULT_CHROME
    runs = []
    for font in FONTS:
        for name, unit, count in TEXTS:
            widths = measure(chrome, font, unit * count)
            runs.append({
                "font": font,
                "text": name,
                "cases": [{"size": size, "width": width}
                          for size, width in zip(SIZES, widths, strict=True)],
            })
        print(font, flush=True)
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "fonts": FONTS,
        "texts": [{"name": name, "unit": unit, "count": count} for name, unit, count in TEXTS],
        "runs": runs,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(runs)} runs, {sum(len(r['cases']) for r in runs)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
