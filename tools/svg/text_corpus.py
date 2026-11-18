#!/usr/bin/env python3
"""Capture SVG text positions from pinned Chromium as a black-box oracle.

`div-svg`'s text layout (`div_svg::text`) ports Blink's BSD SVG text layout
algorithm and writes the LGPL parts (the per-character record, `tspan`, the
scaled-font rule) from SVG 2 and from black-box observation. This corpus
keeps that honest: Chromium 147 is queried only through the SVG text DOM
APIs, and `div-svg` must reproduce every number, as f32, exactly.

Each case is one `<svg>` in its own slot, with the vendored Geist and Inter
variable fonts loaded through `@font-face` (OFL, embedded as data URLs). For
every `<text>` in the case, in document order, the capture records:

- `getNumberOfChars()`, `getComputedTextLength()` and `getBBox()`;
- per addressable character: `getStartPositionOfChar`,
  `getEndPositionOfChar`, `getExtentOfChar` and `getRotationOfChar`;
- `getCTM()`.

SVG text is shaped at its size on screen, so the capture runs once per
device scale: 1 with nothing emulated, and 2 at real zoom
(`--force-device-scale-factor`, RFC 0010).

`pixels` captures the same cases, plus the paint cases (`paint/`: stroke,
`paint-order`, opacities, dashes, a non-scaling stroke, large glyphs), as
pixels: each case in its own slot of one sheet per device scale, rendered
through `render_chrome` (`tools/accountability/run_all_pixel_comparisons.py`)
after the web fonts load. `crates/div-svg/tests/text_corpus.rs` paints each
case with `div_svg::text::paint` and compares its slot exactly.

    text_corpus.py list [--pixels]
    text_corpus.py capture --chrome PATH \\
        --output tests/fidelity/fixtures/svg-text-v1/positions-linux.json
    text_corpus.py pixels --chrome PATH \\
        --output-prefix tests/fidelity/fixtures/svg-text-v1/pixels-linux
"""

from __future__ import annotations

import argparse
import base64
import datetime
import hashlib
import json
import subprocess
import sys
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/accountability"))
import run_all_pixel_comparisons as pixel_runner  # noqa: E402

SCHEMA = "div-svg-text-corpus-v1"
PIXEL_SCHEMA = "div-svg-text-pixels-v1"
EXPECTED_CHROMIUM = "147.0.7727.50"
FONTS = ROOT / "tests/fidelity/components/assets/fonts"
FACES = {
    "Geist": FONTS / "geist/Geist-Variable.woff2",
    "Inter": FONTS / "inter/InterVariable.woff2",
}
SCALES = (1, 2)
SLOT = 160
INK = "#09090b"
SAMPLE = "Jan Feb Mar 2024"


def case(case_id: str, body: str, *, size: tuple[int, int] = (240, 60), attrs: str = "",
         font: str = "Geist", font_size: float = 16, weight: int = 400) -> dict:
    """One `<svg>`; every `<text>` in it is measured."""
    width, height = size
    style = f"font-family:{font};font-size:{font_size:g}px;font-weight:{weight}"
    root = (f'<svg width="{width}" height="{height}" style="{style}"'
            f'{" " + attrs if attrs else ""}>')
    return {"id": case_id, "svg": root + body + "</svg>", "width": width, "height": height,
            "font": font, "font_size": font_size, "weight": weight}


def text(body: str, attrs: str = 'x="10" y="30"') -> str:
    return f"<text {attrs}>{body}</text>"


def run_cases() -> list[dict]:
    out = []
    for font in ("Geist", "Inter"):
        for size in (12, 14, 16):
            for weight in (400, 600):
                out.append(case(f"run/{font.lower()}-{size}-{weight}", text(SAMPLE),
                                font=font, font_size=size, weight=weight))
    for size in (10, 11, 13, 18, 24, 36, 12.5, 15.3):
        out.append(case(f"run/geist-size-{size:g}", text("Revenue 42"), font_size=size,
                        size=(280, 70)))
    out += [
        case("run/digits", text("0123456789 $1,234.56")),
        case("run/punct", text("Sales (USD) – 12% / Q4")),
        case("run/kerning", text("AVATAR Ty To Wa")),
        case("run/ligature", text("office fi fl ffi")),
        case("run/x-fraction", text("Fraction", 'x="10.3" y="30.7"')),
        case("run/no-position", text("Origin"), size=(120, 40)),
        case("run/weight-700", text(SAMPLE), weight=700),
        case("run/weight-500-inter", text(SAMPLE), font="Inter", weight=500),
        case("run/italic", text(SAMPLE, 'x="10" y="30" font-style="italic"')),
        case("run/single-char", text("W")),
        case("run/space-only-words", text("a b c d e f")),
    ]
    return out


def list_cases() -> list[dict]:
    return [
        case("list/x", text("ABCDEF", 'x="10 30 50 70" y="30"')),
        case("list/y", text("ABCDEF", 'x="10" y="20 25 30 35"')),
        case("list/xy-extra", text("AB", 'x="10 40 70 100" y="30 35 40"')),
        case("list/dx", text("ABCDEF", 'x="10" y="30" dx="0 2 4 6"')),
        case("list/dy", text("ABCDEF", 'x="10" y="30" dy="0 1 2 -3"')),
        case("list/dx-em", text("ABCDEF", 'x="10" y="30" dx="0.5em 1em"')),
        case("list/dy-em", text("Tick", 'x="10" y="10" dy="0.71em"')),
        case("list/dy-ex", text("Tick", 'x="10" y="20" dy="1ex"')),
        case("list/dx-fraction", text("ABCD", 'x="10" y="30" dx="0.3 0.3 0.3"')),
        case("list/rotate", text("ABCDEF", 'x="10" y="30" rotate="10 20 30"')),
        case("list/rotate-single", text("ABCDEF", 'x="20" y="30" rotate="45"')),
        case("list/rotate-negative", text("ABCD", 'x="20" y="30" rotate="-30 0 30"')),
        case("list/x-percent", text("Middle", 'x="50%" y="30"')),
        case("list/y-percent", text("Middle", 'x="10" y="50%"')),
        case("list/x-and-dx", text("ABCDEF", 'x="10 40" y="30" dx="5 5 5"')),
        case("list/x-every-char", text("ABCD", 'x="10 30 50 70" y="30 30 30 30"')),
        case("list/commas", text("ABC", 'x="10,30,50" y="30"')),
    ]


def tspan_cases() -> list[dict]:
    return [
        case("tspan/recharts-tick",
             text('<tspan x="50" dy="0.71em">Jan</tspan>', 'x="50" y="20" text-anchor="middle"')),
        case("tspan/nested",
             text('A<tspan dx="5">B<tspan dy="3">C</tspan>D</tspan>E')),
        case("tspan/inherit-list",
             text('AB<tspan>CD</tspan>EF', 'x="10 30 50 70 90 110" y="30"')),
        case("tspan/override-list",
             text('AB<tspan x="100 120">CD</tspan>EF', 'x="10 30 50 70 90 110" y="30"')),
        case("tspan/font-size", text('Total <tspan font-size="24">128</tspan> units')),
        case("tspan/font-weight", text('Total <tspan font-weight="700">128</tspan> units')),
        case("tspan/multiline",
             text('<tspan x="10" dy="0">First</tspan><tspan x="10" dy="1.2em">Second</tspan>'
                  '<tspan x="10" dy="1.2em">Third</tspan>', 'x="10" y="16"'),
             size=(200, 70)),
        case("tspan/rotate-inherit",
             text('AB<tspan>CD</tspan>EF', 'x="10" y="30" rotate="5 10 15 20"')),
        case("tspan/rotate-own",
             text('AB<tspan rotate="30">CD</tspan>EF', 'x="10" y="30" rotate="5"')),
        case("tspan/empty", text('AB<tspan x="80"></tspan>CD')),
        case("tspan/dx-list", text('AB<tspan dx="3 6">CDE</tspan>F')),
        case("tspan/y-only", text('Low <tspan y="45">High</tspan> again')),
        case("tspan/inter-mixed", text('Geist <tspan font-family="Inter">Inter</tspan> Geist')),
        case("tspan/kern-plain", text("AVATAR To")),
        case("tspan/kern-split", text('A<tspan>V</tspan>ATAR <tspan>T</tspan>o')),
        case("tspan/kern-split-styled", text('A<tspan font-weight="400">V</tspan>A')),
        case("tspan/letter-spacing", text('Ab<tspan letter-spacing="3">cd</tspan>ef')),
        case("tspan/fill-only", text('Total <tspan fill="#ef4444">128</tspan> units')),
    ]


def whitespace_cases() -> list[dict]:
    return [
        case("ws/collapse", text("  Hello   world  ")),
        case("ws/newlines", text("Hello\n     world\n")),
        case("ws/tspan-boundary", text("Hello <tspan> world</tspan> again")),
        case("ws/tspan-trailing", text("<tspan>Hello </tspan> <tspan> world</tspan>")),
        case("ws/tabs", text("a\tb\t\tc")),
        case("ws/pre", text("a  b   c", 'x="10" y="30" style="white-space:pre"')),
        case("ws/nbsp", text("a  b c")),
    ]


def anchor_cases() -> list[dict]:
    return [
        case("anchor/start", text("Anchor", 'x="120" y="30" text-anchor="start"')),
        case("anchor/middle", text("Anchor", 'x="120" y="30" text-anchor="middle"')),
        case("anchor/end", text("Anchor", 'x="120" y="30" text-anchor="end"')),
        case("anchor/middle-chunks", text("ABC", 'x="30 120 210" y="30" text-anchor="middle"')),
        case("anchor/end-chunks",
             text('Jan<tspan x="140">Feb</tspan><tspan x="220">Mar</tspan>',
                  'x="60" y="30" text-anchor="end"')),
        case("anchor/middle-dx", text("Centered", 'x="120" y="30" dx="4 2" text-anchor="middle"')),
        case("anchor/end-rotate", text("Rotated", 'x="200" y="30" rotate="15" text-anchor="end"')),
        case("anchor/tspan-chunk",
             text('Left<tspan x="200" text-anchor="end">Right</tspan>', 'x="10" y="30"')),
        case("anchor/middle-y-chunks", text("ABC", 'x="120" y="20 30 40" text-anchor="middle"')),
        case("anchor/middle-fraction", text("Half", 'x="100.5" y="30" text-anchor="middle"')),
    ]


def baseline_cases() -> list[dict]:
    out = []
    for value in ("auto", "alphabetic", "middle", "central", "text-before-edge",
                  "text-after-edge"):
        out.append(case(f"baseline/dominant-{value}",
                        text("Hxg 42", f'x="10" y="30" dominant-baseline="{value}"')))
    for value in ("middle", "central", "text-before-edge", "text-after-edge"):
        out.append(case(f"baseline/alignment-{value}",
                        text(f'Hxg <tspan alignment-baseline="{value}">Hxg</tspan> Hxg')))
    out.append(case("baseline/dominant-tspan",
                    text('Hxg <tspan dominant-baseline="middle">Hxg</tspan> Hxg')))
    for value in ("sub", "super", "3px", "-4px", "20%", "0.5em"):
        out.append(case(f"baseline/shift-{value.replace('%', 'pct')}",
                        text(f'H<tspan baseline-shift="{value}">2</tspan>O')))
    out.append(case("baseline/middle-inter",
                    text("Hxg 42", 'x="10" y="30" dominant-baseline="middle"'), font="Inter"))
    return out


def spacing_cases() -> list[dict]:
    return [
        case("spacing/letter-1px", text(SAMPLE, 'x="10" y="30" letter-spacing="1"')),
        case("spacing/letter-half", text(SAMPLE, 'x="10" y="30" letter-spacing="0.5px"')),
        case("spacing/letter-negative", text(SAMPLE, 'x="10" y="30" letter-spacing="-0.5"')),
        case("spacing/letter-em", text(SAMPLE, 'x="10" y="30" letter-spacing="0.1em"')),
        case("spacing/word-4px", text(SAMPLE, 'x="10" y="30" word-spacing="4"')),
        case("spacing/word-em", text(SAMPLE, 'x="10" y="30" word-spacing="1em"')),
        case("spacing/letter-anchor-middle",
             text(SAMPLE, 'x="120" y="30" letter-spacing="2" text-anchor="middle"')),
    ]


def transform_cases() -> list[dict]:
    t = text("Scaled 42", 'x="5" y="16"')
    return [
        case("xf/scale-2", f'<g transform="scale(2)">{t}</g>', size=(240, 60)),
        case("xf/scale-1.25", f'<g transform="scale(1.25)">{t}</g>'),
        case("xf/scale-0.5", f'<g transform="scale(0.5)">{text("Scaled 42", "x=\"10\" y=\"60\"")}</g>'),
        case("xf/scale-1.37", f'<g transform="scale(1.37)">{t}</g>'),
        case("xf/scale-nonuniform", f'<g transform="scale(2 1)">{t}</g>'),
        case("xf/viewbox-2", t, attrs='viewBox="0 0 120 30"'),
        case("xf/viewbox-1.25", t, attrs='viewBox="0 0 192 48"'),
        case("xf/viewbox-none", t, attrs='viewBox="0 0 120 60" preserveAspectRatio="none"'),
        case("xf/rotate-30", f'<g transform="rotate(30 60 30)">{t}</g>'),
        case("xf/skew", f'<g transform="skewX(20)">{t}</g>'),
        case("xf/translate-fraction", f'<g transform="translate(0.3 0.7)">{t}</g>'),
        case("xf/text-transform", text("Scaled 42", 'x="5" y="16" transform="scale(1.5)"')),
        case("xf/rotate-tick",
             text("January", 'x="80" y="20" transform="rotate(-45 80 20)" text-anchor="end"'),
             size=(160, 80)),
    ]


def chart_cases() -> list[dict]:
    months = ("Jan", "Feb", "Mar", "Apr", "May", "Jun")
    ticks = "".join(
        f'<text x="{20 + 40 * i}" y="12" text-anchor="middle" fill="#71717a">'
        f'<tspan x="{20 + 40 * i}" dy="0.71em">{m}</tspan></text>'
        for i, m in enumerate(months))
    yticks = "".join(
        f'<text x="40" y="{10 + 22 * i}" text-anchor="end" fill="#71717a">'
        f'<tspan x="40" dy="0.355em">{v}</tspan></text>'
        for i, v in enumerate(("300", "200", "100", "0")))
    rotated = "".join(
        f'<text x="{30 + 50 * i}" y="10" text-anchor="end" transform="rotate(-35 {30 + 50 * i} 10)">'
        f'<tspan x="{30 + 50 * i}" dy="0.71em">{m}uary</tspan></text>'
        for i, m in enumerate(months[:4]))
    center = ('<text x="120" y="60" text-anchor="middle" dominant-baseline="middle">'
              '<tspan x="120" y="60" font-size="30" font-weight="700">1,125</tspan>'
              '<tspan x="120" y="84" fill="#71717a">Visitors</tspan></text>')
    legend = "".join(
        f'<text x="{14 + 70 * i}" y="20" dominant-baseline="central">{name}</text>'
        for i, name in enumerate(("Desktop", "Mobile", "Tablet")))
    bar_labels = "".join(
        f'<text x="{20 + 36 * i}" y="{40 - 4 * i}" text-anchor="middle" font-size="12">{v}</text>'
        for i, v in enumerate((186, 305, 237, 73, 209, 214)))
    pie_labels = ('<text x="150" y="30" text-anchor="start" dominant-baseline="central">Chrome 275</text>'
                  '<text x="90" y="30" text-anchor="end" dominant-baseline="central">Safari 200</text>')
    radial = ('<text x="60" y="40" text-anchor="middle" dominant-baseline="middle">'
              '<tspan x="60" y="40" font-size="24" font-weight="700">200</tspan>'
              '<tspan x="60" y="62">Visitors</tspan></text>')
    return [
        case("chart/x-axis", ticks, size=(240, 40), font_size=12),
        case("chart/y-axis", yticks, size=(60, 80), font_size=12),
        case("chart/rotated-ticks", rotated, size=(240, 80), font_size=12),
        case("chart/center-label", center, size=(240, 100), font_size=14),
        case("chart/legend", legend, size=(240, 40), font_size=12),
        case("chart/bar-labels", bar_labels, size=(240, 50)),
        case("chart/pie-labels", pie_labels, size=(240, 60), font_size=12),
        case("chart/radial-label", radial, size=(120, 80), font_size=12),
        case("chart/inter-axis", ticks, size=(240, 40), font="Inter", font_size=12),
        case("chart/tooltip-value", text('Desktop <tspan font-weight="600">186</tspan>',
                                         'x="10" y="20"'), font_size=12),
    ]


def corpus() -> list[dict]:
    return (run_cases() + list_cases() + tspan_cases() + whitespace_cases() + anchor_cases()
            + baseline_cases() + spacing_cases() + transform_cases() + chart_cases())


def paint_cases() -> list[dict]:
    """Pixel-only cases: how the glyphs are filled and stroked."""
    word = "Revenue 42"
    big = 'x="10" y="44"'
    return [
        case("paint/stroke", text(word, f'{big} stroke="#ef4444"'), font_size=36,
             size=(240, 60)),
        case("paint/stroke-only", text(word, f'{big} fill="none" stroke="#2563eb"'),
             font_size=36, size=(240, 60)),
        case("paint/stroke-wide", text(word, f'{big} stroke="#ef4444" stroke-width="3" '
                                             'stroke-linejoin="round"'),
             font_size=36, size=(240, 60)),
        case("paint/order-stroke", text(word, f'{big} stroke="#ef4444" stroke-width="4" '
                                              'paint-order="stroke"'),
             font_size=36, size=(240, 60)),
        case("paint/fill-opacity", text(word, f'{big} fill="#2563eb" fill-opacity="0.5"'),
             font_size=36, size=(240, 60)),
        case("paint/stroke-opacity", text(word, f'{big} stroke="#ef4444" stroke-width="2" '
                                                'stroke-opacity="0.5"'),
             font_size=36, size=(240, 60)),
        case("paint/stroke-dash", text(word, f'{big} fill="none" stroke="#09090b" '
                                             'stroke-dasharray="3 2"'),
             font_size=36, size=(240, 60)),
        case("paint/stroke-scaled",
             f'<g transform="scale(1.5)">{text(word, "x=\"6\" y=\"30\" stroke=\"#ef4444\"")}</g>',
             font_size=24, size=(240, 60)),
        case("paint/non-scaling",
             f'<g transform="scale(1.5)">{text(word, "x=\"6\" y=\"30\" stroke=\"#ef4444\" vector-effect=\"non-scaling-stroke\"")}</g>',
             font_size=24, size=(240, 60)),
        case("paint/tspan-stroke",
             text(f'Total <tspan stroke="#ef4444" fill="none">128</tspan> units', big),
             font_size=30, size=(240, 60)),
        case("paint/current-color",
             text(word, f'{big} fill="currentColor" color="#16a34a"'), font_size=36,
             size=(240, 60)),
        case("paint/crisp", text(word, f'{big} shape-rendering="crispEdges"'), font_size=36,
             size=(240, 60)),
        case("paint/large", text("Ag", 'x="8" y="86"'), font_size=96, size=(160, 100)),
        case("paint/rotate-stroke",
             text("ABCD", 'x="20" y="40" rotate="20" stroke="#ef4444"'), font_size=32,
             size=(200, 60)),
    ]


def pixel_cases() -> list[dict]:
    return corpus() + paint_cases()


SHEET_WIDTH = 1200
GAP = 8


def pixel_sheet(cases: list[dict], scale: int) -> dict:
    """Shelf-pack the cases into one sheet, in order, `GAP` px apart."""
    placed = []
    x = y = row = 0
    for c in cases:
        if x and x + c["width"] > SHEET_WIDTH:
            x, y, row = 0, y + row + GAP, 0
        placed.append({**c, "x": x, "y": y})
        x += c["width"] + GAP
        row = max(row, c["height"])
    return {"scale": scale, "width": SHEET_WIDTH, "height": y + row + GAP, "cases": placed}


def pixel_page(sheet: dict) -> str:
    slots = "\n".join(
        f'<div style="left:{c["x"]}px;top:{c["y"]}px">{c["svg"]}</div>' for c in sheet["cases"])
    return ("<!doctype html><meta charset=utf-8>"
            f"<style>{font_faces()}html{{overflow:hidden;background:#fff;color:{INK}}}"
            "body{margin:0}div{position:absolute}svg{display:block}</style>\n"
            f"{slots}\n")


def capture_pixels(chrome: Path, prefix: Path) -> int:
    cases = pixel_cases()
    sheets = [pixel_sheet(cases, scale) for scale in SCALES]
    version = ""
    prefix.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="div-svg-text-", ignore_cleanup_errors=True) as tmp:
        for sheet in sheets:
            scale = sheet["scale"]
            page = Path(tmp) / f"sheet-{scale}x.html"
            page.write_text(pixel_page(sheet), encoding="utf-8")
            png = prefix.parent / f"{prefix.name}-{scale}x.png"
            for attempt in range(3):
                if pixel_runner.render_chrome(
                    str(page), str(png), str(chrome), str(chrome.parent),
                    logical_width=sheet["width"], logical_height=sheet["height"],
                    device_scale=float(scale),
                ):
                    break
                print(f"{scale}x capture attempt {attempt + 1} failed", file=sys.stderr)
                time.sleep(1)
            else:
                raise SystemExit(f"could not capture the {scale}x sheet")
            sheet["png"] = png.name
            sheet["png_sha256"] = hashlib.sha256(png.read_bytes()).hexdigest()
            sheet["method"] = pixel_runner.device_scale_method(scale)
    version = browser_version(chrome)

    fonts = {family: hashlib.sha256(path.read_bytes()).hexdigest()
             for family, path in FACES.items()}
    corpus_text = json.dumps({"sheets": [{k: v for k, v in s.items() if not k.startswith("png")}
                                         for s in sheets], "fonts": fonts}, sort_keys=True)
    header = {
        "schema": PIXEL_SCHEMA,
        "captured": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "chromium": version,
        "pinned": EXPECTED_CHROMIUM in version,
        "platform": sys.platform,
        "fonts_sha256": fonts,
        "corpus_sha256": hashlib.sha256(corpus_text.encode()).hexdigest(),
    }
    lines = ["{"]
    lines += [f"{json.dumps(k)}: {json.dumps(v)}," for k, v in sorted(header.items())]
    lines.append('"sheets": [')
    sheet_texts = []
    for sheet in sheets:
        meta = {k: v for k, v in sheet.items() if k != "cases"}
        body = ",\n".join(json.dumps(c, sort_keys=True) for c in sheet["cases"])
        sheet_texts.append(json.dumps(meta, sort_keys=True)[:-1] + ', "cases": [\n' + body + "]}")
    lines.append(",\n".join(sheet_texts))
    lines.append("]}")
    out = prefix.parent / f"{prefix.name}.json"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    json.loads(out.read_text(encoding="utf-8"))
    print(f"{len(cases)} cases x {len(SCALES)} scales captured as pixels from {version} "
          f"into {out}")
    return 0


PAGE_SCRIPT = r"""
(async () => {
  await document.fonts.ready;
  const faces = [...document.fonts].map(f => [f.family, f.status]);
  const r4 = (b) => [b.x, b.y, b.width, b.height];
  const m = (x) => x && [x.a, x.b, x.c, x.d, x.e, x.f];
  const out = {};
  for (const svg of document.querySelectorAll('svg[data-case]')) {
    const texts = [];
    for (const el of svg.querySelectorAll('text')) {
      const rec = {};
      try {
        const n = el.getNumberOfChars();
        rec.chars = n;
        rec.length = el.getComputedTextLength();
        rec.bbox = r4(el.getBBox());
        rec.ctm = m(el.getCTM());
        rec.start = []; rec.end = []; rec.extent = []; rec.rotation = [];
        for (let i = 0; i < n; i++) {
          const s = el.getStartPositionOfChar(i), e = el.getEndPositionOfChar(i);
          rec.start.push([s.x, s.y]);
          rec.end.push([e.x, e.y]);
          rec.extent.push(r4(el.getExtentOfChar(i)));
          rec.rotation.push(el.getRotationOfChar(i));
        }
      } catch (e) { rec.error = String(e); }
      texts.push(rec);
    }
    out[svg.dataset.case] = texts;
  }
  return {faces, cases: out};
})()
"""


def font_faces() -> str:
    rules = []
    for family, path in FACES.items():
        data = base64.b64encode(path.read_bytes()).decode("ascii")
        rules.append(f"@font-face{{font-family:{family};font-weight:100 900;"
                     f"src:url(data:font/woff2;base64,{data}) format('woff2')}}")
    return "".join(rules)


def page_html(cases: list[dict]) -> str:
    rows = "\n".join(
        f'<div class="c" style="top:{SLOT * index}px">'
        + c["svg"].replace("<svg ", '<svg data-case="%s" ' % c["id"], 1)
        + "</div>"
        for index, c in enumerate(cases)
    )
    return ("<!doctype html><meta charset=utf-8>"
            f"<style>{font_faces()}html{{background:#fff;color:{INK}}}"
            "body{margin:0}.c{position:absolute;left:0}svg{display:block;overflow:visible}"
            "</style>\n"
            f"{rows}\n")


def capture(chrome: Path, cases: list[dict], scale: int) -> tuple[dict, str]:
    """Return the per-case results and the browser's own version string."""
    html = page_html(cases)
    with tempfile.TemporaryDirectory(prefix="div-svg-text-", ignore_cleanup_errors=True) as tmp, \
            ExitStack() as stack:
        page = Path(tmp) / "corpus.html"
        page.write_text(html, encoding="utf-8")
        profile = Path(tmp) / "profile"
        flags, override = pixel_runner.device_scale_launch(800, 600, scale)
        process = subprocess.Popen(
            [str(chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0", *flags,
             f"--user-data-dir={profile}", "about:blank"],
            env=pixel_runner.chrome_environment(str(chrome.parent)),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        stack.callback(pixel_runner._stop_chromium_process, process)
        port = pixel_runner._wait_for_devtools_endpoint(str(profile), process, timeout=30)
        client = pixel_runner._CdpWebSocket(pixel_runner._page_websocket_url(port, timeout=30))
        stack.callback(pixel_runner._close_chromium_client, client)
        version = client.command("Browser.getVersion").get("product", "unknown")
        client.command("Emulation.setDeviceMetricsOverride", override)
        client.command("Page.enable")
        client.command("Page.navigate", {"url": page.as_uri()})
        deadline = time.monotonic() + 30
        while evaluate(client, "document.readyState") != "complete":
            if time.monotonic() > deadline:
                raise TimeoutError("corpus page did not load")
            time.sleep(0.05)
        dpr = evaluate(client, "window.devicePixelRatio")
        if float(dpr) != float(scale):
            raise RuntimeError(f"devicePixelRatio is {dpr}, expected {scale}")
        result = evaluate(client, PAGE_SCRIPT, await_promise=True)
        bad = [f for f in result["faces"] if f[1] != "loaded"]
        if bad or len(result["faces"]) != len(FACES):
            raise RuntimeError(f"web fonts not loaded: {result['faces']}")
        return result["cases"], version


def browser_version(chrome: Path) -> str:
    """The product string of `chrome`, which `render_chrome` does not report."""
    with tempfile.TemporaryDirectory(prefix="div-svg-text-", ignore_cleanup_errors=True) as tmp,             ExitStack() as stack:
        profile = Path(tmp) / "profile"
        process = subprocess.Popen(
            [str(chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0",
             f"--user-data-dir={profile}", "about:blank"],
            env=pixel_runner.chrome_environment(str(chrome.parent)),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
        )
        stack.callback(pixel_runner._stop_chromium_process, process)
        port = pixel_runner._wait_for_devtools_endpoint(str(profile), process, timeout=30)
        client = pixel_runner._CdpWebSocket(pixel_runner._page_websocket_url(port, timeout=30))
        stack.callback(pixel_runner._close_chromium_client, client)
        return client.command("Browser.getVersion").get("product", "unknown")


def evaluate(client, expression: str, await_promise: bool = False):
    response = client.command(
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": await_promise})
    if "exceptionDetails" in response:
        raise RuntimeError(f"page script failed: {response['exceptionDetails']}")
    return response.get("result", {}).get("value")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    lst = sub.add_parser("list")
    lst.add_argument("--pixels", action="store_true", help="the pixel set's cases")
    cap = sub.add_parser("capture")
    cap.add_argument("--chrome", type=Path, required=True)
    cap.add_argument("--output", type=Path, required=True)
    pix = sub.add_parser("pixels")
    pix.add_argument("--chrome", type=Path, required=True)
    pix.add_argument("--output-prefix", type=Path, required=True)
    args = parser.parse_args()

    cases = pixel_cases() if getattr(args, "pixels", False) else corpus()
    ids = [c["id"] for c in pixel_cases()]
    if len(set(ids)) != len(ids):
        raise SystemExit("duplicate case ids")
    if args.command == "pixels":
        return capture_pixels(args.chrome, args.output_prefix)
    ids = [c["id"] for c in cases]
    if args.command == "list":
        for c in cases:
            print(f'{c["id"]}\t{c["svg"]}')
        print(f"{len(cases)} cases", file=sys.stderr)
        return 0

    per_scale = {}
    version = ""
    for scale in SCALES:
        for attempt in range(3):
            try:
                results, version = capture(args.chrome, cases, scale)
                break
            except (RuntimeError, TimeoutError, OSError) as error:
                if attempt == 2:
                    raise
                print(f"{scale}x attempt {attempt + 1} failed ({error}); retrying",
                      file=sys.stderr)
        missing = sorted(set(ids) - set(results or {}))
        if missing:
            raise SystemExit(f"no {scale}x result for {len(missing)} cases, first {missing[0]}")
        errors = [(i, t["error"]) for i in ids for t in results[i] if "error" in t]
        if errors:
            raise SystemExit(f"{scale}x query errors: {errors[:3]}")
        per_scale[scale] = results

    fonts = {family: hashlib.sha256(path.read_bytes()).hexdigest()
             for family, path in FACES.items()}
    corpus_text = json.dumps({"cases": cases, "fonts": fonts}, sort_keys=True)
    header = {
        "schema": SCHEMA,
        "captured": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "chromium": version,
        "pinned": EXPECTED_CHROMIUM in version,
        "platform": sys.platform,
        "fonts_sha256": fonts,
        "scales": list(SCALES),
        "corpus_sha256": hashlib.sha256(corpus_text.encode()).hexdigest(),
    }
    lines = ["{"]
    lines += [f"{json.dumps(k)}: {json.dumps(v)}," for k, v in sorted(header.items())]
    lines.append('"cases": [')
    # One case per line keeps the tracked oracle small and its diffs readable.
    lines.append(",\n".join(
        json.dumps({**c, "chromium": {str(s): per_scale[s][c["id"]] for s in SCALES}},
                   sort_keys=True)
        for c in cases))
    lines.append("]}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    json.loads(args.output.read_text(encoding="utf-8"))
    print(f"{len(cases)} cases x {len(SCALES)} scales captured from {version} into {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
