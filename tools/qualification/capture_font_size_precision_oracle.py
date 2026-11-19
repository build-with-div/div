"""Record which font sizes pinned Chromium 147 makes into the same font.

Each case lays one text out at one CSS font size, as an inline-block span
(layout-unit width) and as SVG `<text>` (`getComputedTextLength`, in float).
Two cases with equal widths were shaped with the same font. The native test
`div-engine/tests/font_size_precision.rs` requires Div to make two sizes
into one font exactly when Chromium does.

- `page` runs put many sizes on one page at 1x.
- `alone` runs give each size a fresh browser, at 1.5x and 2x (real zoom,
  RFC 0010). A page that meets two sizes at another scale can give the
  second the first one's font, so non-1x sizes are measured alone.
- `history` runs put two sizes on one page in each order, as evidence of
  that page-history effect, which Div does not model (a named gap).

The fonts are web fonts from this repository, so the test needs no system
font. Windows only: it uses the pinned Windows Chrome-for-Testing. Black-box
measurements; no Blink font source was read.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "tests/fidelity/fixtures/font-size-precision-v1/chromium-windows.json"

TEXT = "Hello, world. Sphinx of black quartz"
FONTS = {
    "DejaVu Sans": "crates/div-text/fonts/DejaVuSans.ttf",
    "Geist": "tests/fidelity/components/assets/fonts/geist/Geist-Variable.woff2",
}
# (label, @font-face size-adjust, font-size-adjust)
ADJUSTS = [("none", None, None), ("size-adjust 150%", 1.5, None),
           ("size-adjust 110%", 1.1, None), ("font-size-adjust 0.6", None, 0.6)]
ALONE_SIZES = [
    "10.11", "12.5", "12.505", "13.001", "13.009", "13.33", "13.331", "13.334", "13.335",
    "13.339", "15.999", "16", "16.001", "16.004", "16.005", "16.009", "16.011", "16.019",
    "16.05", "16.501", "16.509", "17.001", "17.009", "20.004", "20.006", "25.001", "25.009",
    "25.291", "25.294", "25.295", "25.298",
]
HISTORY_PAIRS = [
    ("16.001", "16.009"), ("17.001", "17.009"), ("13.001", "13.009"), ("16.501", "16.509"),
    ("16.011", "16.019"), ("25.001", "25.009"), ("13.331", "13.339"), ("25.291", "25.298"),
]

SCRIPT = """
[...document.querySelectorAll('[data-size]')].map(box => ({
  svg: box.querySelector('text').getComputedTextLength(),
  html: box.querySelector('span').getBoundingClientRect().width,
}))
"""


def hundredths(low: int, high: int) -> list[str]:
    return [f"{value // 100}.{value % 100:02d}" for value in range(low, high + 1)]


def thousandths(base: str) -> list[str]:
    whole, fraction = base.split(".")
    start = int(whole) * 1000 + int(fraction) * 10
    return [f"{value // 1000}.{value % 1000:03d}" for value in range(start, start + 11)]


def page(font: str, adjust: tuple, sizes: list[str]) -> str:
    _, size_adjust, font_size_adjust = adjust
    source = (ROOT / FONTS[font]).resolve().as_uri()
    descriptor = f" size-adjust: {size_adjust * 100:g}%;" if size_adjust else ""
    extra = f" font-size-adjust: {font_size_adjust};" if font_size_adjust else ""
    boxes = "".join(
        f'<div data-size style="font: {size}px Probe;{extra} line-height: normal">'
        f'<span style="display: inline-block; white-space: pre">{TEXT}</span>'
        f'<svg width="10" height="10" style="display: block"><text>{TEXT}</text></svg></div>'
        for size in sizes
    )
    return (f"<!DOCTYPE html><style>@font-face {{ font-family: Probe; src: url('{source}');"
            f"{descriptor} }} body {{ margin: 0 }}</style>{boxes}")


def measure(chrome: Path, font: str, adjust: tuple, sizes: list[str], scale: float) -> list[dict]:
    import windows_chrome

    with tempfile.TemporaryDirectory(prefix="div-font-size-oracle-") as directory:
        path = Path(directory) / "font-size.html"
        path.write_text(page(font, adjust, sizes), encoding="utf-8")
        with windows_chrome.WindowsChrome(chrome, 800, 600, scale) as browser:
            browser.load(path)
            measured = browser.evaluate(SCRIPT)
    return [{"size": size, **widths} for size, widths in zip(sizes, measured, strict=True)]


def run(kind: str, font: str, adjust: tuple, scale: float, cases: list[dict]) -> dict:
    _, size_adjust, font_size_adjust = adjust
    return {"kind": kind, "font": font, "scale": scale, "size_adjust": size_adjust,
            "font_size_adjust": font_size_adjust, "cases": cases}


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
        plain = ADJUSTS[0]
        sizes = hundredths(1000, 3000)
        runs.append(run("page", font, plain, 1.0, measure(chrome, font, plain, sizes, 1.0)))
        sizes = thousandths("13.33") + thousandths("16.00") + thousandths("25.29")
        sizes += ["13.3333", "13.33333", "16.0001", "25.2999"]
        runs.append(run("page", font, plain, 1.0, measure(chrome, font, plain, sizes, 1.0)))
        for adjust in ADJUSTS[1:]:
            runs.append(run("page", font, adjust, 1.0,
                            measure(chrome, font, adjust, ALONE_SIZES, 1.0)))
        for scale in (1.5, 2.0):
            cases = [measure(chrome, font, plain, [size], scale)[0] for size in ALONE_SIZES]
            runs.append(run("alone", font, plain, scale, cases))
            for first, second in HISTORY_PAIRS:
                for pair in ((first, second), (second, first)):
                    runs.append(run("history", font, plain, scale,
                                    measure(chrome, font, plain, list(pair), scale)))
        print(font, flush=True)
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "text": TEXT,
        "fonts": FONTS,
        "runs": runs,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=0) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(runs)} runs, {sum(len(r['cases']) for r in runs)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
