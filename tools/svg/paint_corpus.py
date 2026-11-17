#!/usr/bin/env python3
"""Capture SVG painting from pinned Chromium as a pixel oracle.

`div-svg`'s painter (`div_svg::paint`) ports Blink's BSD SVG painters and
stroke setup. This corpus checks it pixel for pixel: each case is one inline
`<svg>` in its own fixed slot on a white page, and Chromium 147 renders the
page once per device scale. `crates/div-svg/tests/paint_corpus.rs` paints
every case with `div-svg` and compares it with that case's slot, exactly.

The capture goes through `render_chrome` in
`tools/accountability/run_all_pixel_comparisons.py`: at scale 1 nothing is
emulated, and at any other scale Chromium runs at real zoom
(`--force-device-scale-factor`, RFC 0010). The output for one OS is:

- `<prefix>.json`: the cases, their slots, and the sheet for each scale;
- `<prefix>-<scale>x.png`: Chromium's screenshot of each sheet.

There are three sets, each with its own fixtures and gap list: `v1` (fills,
strokes, transforms, viewports and the Lucide icons), `v2` (dashes,
`pathLength`, `paint-order`, `shape-rendering`, `vector-effect` and
chart-shaped compositions) and `v3` (`clipPath`, every case at both scales).

    paint_corpus.py list [--set v2]
    paint_corpus.py capture --set v2 --chrome PATH --output-prefix \\
        tests/fidelity/fixtures/svg-paint-v2/chromium-linux
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/accountability"))
import run_all_pixel_comparisons as pixel_runner  # noqa: E402

SCHEMA = "div-svg-paint-corpus-v1"
EXPECTED_CHROMIUM = "147.0.7727.50"
LUCIDE = ROOT / "tests/fidelity/components/assets/icons/lucide"
SLOT = 112
COLUMNS = 10
INK = "#09090b"


def case(case_id: str, body: str, *, size: tuple[int, int] = (64, 64),
         attrs: str = "", offset: tuple[float, float] = (0.0, 0.0)) -> dict:
    width, height = size
    root = f'<svg width="{width}" height="{height}"{" " + attrs if attrs else ""}>'
    return {"id": case_id, "svg": root + body + "</svg>", "width": width,
            "height": height, "offset": list(offset)}


def fill_cases() -> list[dict]:
    f = 'fill="#3b82f6"'
    return [
        case("fill/rect", f'<rect x="8" y="8" width="48" height="40" {f}/>'),
        case("fill/rect-half", f'<rect x="8.5" y="8.5" width="47" height="40.25" {f}/>'),
        case("fill/rect-quarter", f'<rect x="8.25" y="7.75" width="20.5" height="30.5" {f}/>'),
        case("fill/rect-rounded", f'<rect x="6" y="6" width="52" height="44" rx="10" {f}/>'),
        case("fill/rect-rounded-xy", f'<rect x="6" y="6" width="52" height="44" rx="14" ry="6" {f}/>'),
        case("fill/circle", f'<circle cx="32" cy="32" r="24" {f}/>'),
        case("fill/circle-half", f'<circle cx="32.5" cy="31.5" r="20.25" {f}/>'),
        case("fill/circle-small", f'<circle cx="10" cy="10" r="1.5" {f}/><circle cx="20.5" cy="10.5" r="0.75" {f}/>'),
        case("fill/ellipse", f'<ellipse cx="32" cy="32" rx="28" ry="14" {f}/>'),
        case("fill/polygon-nonzero", f'<polygon points="32,4 40,58 4,22 60,22 24,58" {f}/>'),
        case("fill/polygon-evenodd", f'<polygon points="32,4 40,58 4,22 60,22 24,58" fill-rule="evenodd" {f}/>'),
        case("fill/path-curves", f'<path d="M8 40 C8 8 56 8 56 40 Q32 64 8 40z" {f}/>'),
        case("fill/path-arcs", f'<path d="M8 32 A24 16 30 1 1 56 32 A12 12 0 0 0 8 32z" {f}/>'),
        case("fill/path-subpaths", f'<path d="M4 4h56v56h-56z M16 16v32h32v-32z" fill-rule="evenodd" {f}/>'),
        case("fill/color-ink", f'<rect x="8" y="8" width="48" height="48" rx="8" fill="{INK}"/>'),
        case("fill/color-red", '<circle cx="32" cy="32" r="24" fill="#ff0000"/>'),
        case("fill/color-gray", '<circle cx="32" cy="32" r="24" fill="#808080"/>'),
        case("fill/opacity-half", '<circle cx="32" cy="32" r="24" fill="#ff0000" fill-opacity="0.5"/>'),
        case("fill/opacity-third", '<circle cx="32" cy="32" r="24" fill="#3b82f6" fill-opacity="0.33"/>'),
        case("fill/opacity-overlap",
             '<circle cx="24" cy="32" r="18" fill="#ff0000" fill-opacity="0.6"/>'
             '<circle cx="40" cy="32" r="18" fill="#0000ff" fill-opacity="0.4"/>'),
        case("fill/default-black", '<rect x="8" y="8" width="48" height="48"/>'),
    ]


def stroke_cases() -> list[dict]:
    s = f'fill="none" stroke="{INK}"'
    line = '<line x1="12" y1="32" x2="52" y2="32" stroke-width="8" stroke-linecap="%s" ' + s + "/>"
    poly = ('<polyline points="8,52 32,12 56,52" stroke-width="6" '
            'stroke-linejoin="%s" ' + s + "/>")
    return [
        case("stroke/width-1", f'<rect x="8" y="8" width="48" height="48" stroke-width="1" {s}/>'),
        case("stroke/width-1-half", f'<rect x="8.5" y="8.5" width="47" height="47" stroke-width="1" {s}/>'),
        case("stroke/width-2", f'<rect x="8" y="8" width="48" height="48" stroke-width="2" {s}/>'),
        case("stroke/width-3", f'<rect x="8" y="8" width="48" height="48" stroke-width="3" {s}/>'),
        case("stroke/width-half", f'<rect x="8" y="8" width="48" height="48" stroke-width="0.5" {s}/>'),
        case("stroke/width-hairline", f'<line x1="4" y1="10" x2="60" y2="54" stroke-width="0.1" {s}/>'),
        case("stroke/default-width", '<circle cx="32" cy="32" r="24" fill="none" stroke="#ff0000"/>'),
        case("stroke/circle", f'<circle cx="32" cy="32" r="22" stroke-width="2" {s}/>'),
        case("stroke/ellipse", f'<ellipse cx="32" cy="32" rx="26" ry="12" stroke-width="3" {s}/>'),
        case("stroke/rect-rounded", f'<rect x="8" y="8" width="48" height="48" rx="8" stroke-width="2" {s}/>'),
        case("stroke/cap-butt", line % "butt"),
        case("stroke/cap-round", line % "round"),
        case("stroke/cap-square", line % "square"),
        case("stroke/join-miter", poly % "miter"),
        case("stroke/join-round", poly % "round"),
        case("stroke/join-bevel", poly % "bevel"),
        case("stroke/miterlimit-1",
             '<polyline points="8,52 32,24 56,52" stroke-width="6" stroke-miterlimit="1" ' + s + "/>"),
        case("stroke/miterlimit-10",
             '<polyline points="8,52 32,24 56,52" stroke-width="6" stroke-miterlimit="10" ' + s + "/>"),
        case("stroke/miter-sharp",
             '<polyline points="8,56 32,8 40,56" stroke-width="4" stroke-miterlimit="10" ' + s + "/>"),
        case("stroke/zero-length-round",
             '<path d="M32 32h0" stroke-width="10" stroke-linecap="round" ' + s + "/>"),
        case("stroke/opacity", '<rect x="8" y="8" width="48" height="48" fill="none" '
             'stroke="#ff0000" stroke-width="8" stroke-opacity="0.5"/>'),
        case("stroke/zero-width", f'<rect x="8" y="8" width="48" height="48" stroke-width="0" fill="#3b82f6" stroke="#ff0000"/>'),
        case("both/fill-stroke", '<rect x="10" y="10" width="44" height="44" fill="#3b82f6" '
             f'stroke="{INK}" stroke-width="4"/>'),
        case("both/opacities", '<circle cx="32" cy="32" r="22" fill="#3b82f6" fill-opacity="0.5" '
             'stroke="#ff0000" stroke-width="6" stroke-opacity="0.5"/>'),
        case("both/path", '<path d="M8 48 L24 12 L40 40 L56 8" fill="#93c5fd" '
             f'stroke="{INK}" stroke-width="3" stroke-linejoin="round" stroke-linecap="round"/>'),
    ]


def transform_cases() -> list[dict]:
    r = 'width="24" height="16" fill="#3b82f6" stroke="#09090b" stroke-width="2"'
    return [
        case("transform/translate-half", f'<rect x="8" y="8" {r} transform="translate(0.5 0.5)"/>'),
        case("transform/rotate", f'<rect x="20" y="24" {r} transform="rotate(30 32 32)"/>'),
        case("transform/scale", f'<rect x="4" y="4" {r} transform="scale(1.5)"/>'),
        case("transform/scale-nonuniform",
             '<circle cx="16" cy="16" r="10" fill="none" stroke="#09090b" stroke-width="2" '
             'transform="scale(2 1)"/>'),
        case("transform/skew", f'<rect x="8" y="16" {r} transform="skewX(20)"/>'),
        case("transform/matrix", f'<rect x="8" y="8" {r} transform="matrix(0.8 0.3 -0.2 0.9 10 4)"/>'),
        case("transform/group",
             '<g transform="translate(32 32)"><g transform="rotate(45)">'
             f'<rect x="-12" y="-8" {r}/></g></g>'),
        case("transform/group-inherit",
             '<g fill="none" stroke="#ff0000" stroke-width="3" transform="scale(0.75)">'
             '<circle cx="40" cy="40" r="20"/><path d="M10 70 L70 10"/></g>'),
    ]


def viewport_cases() -> list[dict]:
    body = ('<rect x="0" y="0" width="100" height="50" fill="#93c5fd"/>'
            '<circle cx="50" cy="25" r="20" fill="none" stroke="#09090b" stroke-width="4"/>')
    return [
        case("viewbox/scale-2", '<path d="M4 12h16M12 4v16" stroke="#09090b" stroke-width="2"/>',
             size=(48, 48), attrs='viewBox="0 0 24 24"'),
        case("viewbox/scale-1.25", '<path d="M4 12h16M12 4v16" stroke="#09090b" stroke-width="2"/>',
             size=(30, 30), attrs='viewBox="0 0 24 24"'),
        case("viewbox/scale-0.75", '<circle cx="12" cy="12" r="9" fill="none" stroke="#09090b" '
             'stroke-width="2"/>', size=(18, 18), attrs='viewBox="0 0 24 24"'),
        case("viewbox/offset", '<circle cx="62" cy="62" r="10" fill="#3b82f6"/>',
             size=(40, 40), attrs='viewBox="50 50 24 24"'),
        case("viewbox/meet", body, size=(60, 60), attrs='viewBox="0 0 100 50"'),
        case("viewbox/meet-xmin-ymax", body, size=(60, 60),
             attrs='viewBox="0 0 100 50" preserveAspectRatio="xMinYMax meet"'),
        case("viewbox/slice", body, size=(60, 60),
             attrs='viewBox="0 0 100 50" preserveAspectRatio="xMidYMid slice"'),
        case("viewbox/none", body, size=(60, 60),
             attrs='viewBox="0 0 100 50" preserveAspectRatio="none"'),
        case("viewbox/overflow-clip",
             '<circle cx="12" cy="12" r="16" fill="#3b82f6"/>', size=(48, 48),
             attrs='viewBox="0 0 24 24"'),
        case("nested/svg",
             '<rect width="64" height="64" fill="#e5e7eb"/>'
             '<svg x="8" y="8" width="48" height="32" viewBox="0 0 24 16">'
             '<circle cx="12" cy="8" r="10" fill="#3b82f6"/></svg>'),
        case("nested/svg-half",
             '<svg x="8.5" y="8.5" width="47" height="31" viewBox="0 0 24 16">'
             '<circle cx="12" cy="8" r="10" fill="#3b82f6"/></svg>'),
        case("offset/root-half", '<rect x="8" y="8" width="48" height="48" fill="#3b82f6"/>',
             offset=(0.5, 0.5)),
        case("offset/root-quarter", '<circle cx="32" cy="32" r="20" fill="none" stroke="#09090b" '
             'stroke-width="2"/>', offset=(0.25, 0.75)),
    ]


def lucide_markup(name: str, size: int) -> str:
    text = (LUCIDE / f"{name}.svg").read_text(encoding="utf-8")
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
    text = " ".join(text.split()).replace("> <", "><").replace(" />", "/>")
    text = text.replace('width="24" height="24"', f'width="{size}" height="{size}"', 1)
    assert f'width="{size}"' in text, name
    return text


def lucide_cases() -> list[dict]:
    names = sorted(p.stem for p in LUCIDE.glob("*.svg"))
    out = []
    for size in (16, 24):
        for name in names:
            out.append({"id": f"lucide/{name}-{size}", "svg": lucide_markup(name, size),
                        "width": size, "height": size, "offset": [0.0, 0.0]})
    return out


S = f'fill="none" stroke="{INK}"'
GRID = "#d4d4d8"
BLUE = "#2563eb"


def dash_cases() -> list[dict]:
    line = '<line x1="6" y1="32" x2="58" y2="32" stroke-width="2" ' + S + ' %s/>'
    wide = '<line x1="8" y1="32" x2="56" y2="32" stroke-width="6" ' + S + ' %s/>'
    return [
        case("dash/line", line % 'stroke-dasharray="6 4"'),
        case("dash/line-odd", line % 'stroke-dasharray="6 4 2"'),
        case("dash/line-single", line % 'stroke-dasharray="5"'),
        case("dash/line-comma", line % 'stroke-dasharray="6,2,1"'),
        case("dash/line-fraction", line % 'stroke-dasharray="2.5 1.25"'),
        case("dash/offset", line % 'stroke-dasharray="6 4" stroke-dashoffset="3"'),
        case("dash/offset-negative", line % 'stroke-dasharray="6 4" stroke-dashoffset="-3"'),
        case("dash/offset-large", line % 'stroke-dasharray="6 4" stroke-dashoffset="23.5"'),
        case("dash/negative", line % 'stroke-dasharray="4 -2"'),
        case("dash/all-zero", line % 'stroke-dasharray="0 0"'),
        case("dash/zero-gap", line % 'stroke-dasharray="4 0"'),
        case("dash/dots-round", wide % 'stroke-dasharray="0 10" stroke-linecap="round"'),
        case("dash/dots-square", wide % 'stroke-dasharray="0 10" stroke-linecap="square"'),
        case("dash/dots-butt", wide % 'stroke-dasharray="0 10"'),
        case("dash/caps-round", wide % 'stroke-dasharray="8 8" stroke-linecap="round"'),
        case("dash/percent", line % 'stroke-dasharray="10% 5%"'),
        case("dash/offset-percent", line % 'stroke-dasharray="6 4" stroke-dashoffset="5%"'),
        case("dash/units", line % 'stroke-dasharray="1mm 2px"'),
        case("dash/rect", '<rect x="8" y="8" width="48" height="40" stroke-width="2" '
             'stroke-dasharray="8 4" ' + S + '/>'),
        case("dash/rect-grid", '<rect x="8.5" y="8.5" width="47" height="39" stroke-width="1" '
             f'stroke-dasharray="3 3" fill="none" stroke="{GRID}"/>'),
        case("dash/rect-rounded", '<rect x="8" y="8" width="48" height="40" rx="10" stroke-width="2" '
             'stroke-dasharray="6 3" ' + S + '/>'),
        case("dash/circle", '<circle cx="32" cy="32" r="22" stroke-width="3" '
             'stroke-dasharray="7 4" ' + S + '/>'),
        case("dash/circle-offset", '<circle cx="32" cy="32" r="22" stroke-width="3" '
             'stroke-dasharray="7 4" stroke-dashoffset="5" ' + S + '/>'),
        case("dash/ellipse", '<ellipse cx="32" cy="32" rx="26" ry="14" stroke-width="2" '
             'stroke-dasharray="5 3" ' + S + '/>'),
        case("dash/polyline", '<polyline points="8,52 24,12 40,52 56,12" stroke-width="3" '
             'stroke-dasharray="10 4" ' + S + '/>'),
        case("dash/polygon", '<polygon points="32,6 58,52 6,52" stroke-width="2" '
             'stroke-dasharray="9 5" stroke-linejoin="round" ' + S + '/>'),
        case("dash/path-curve", '<path d="M6 48 C14 4 50 4 58 48" stroke-width="2" '
             'stroke-dasharray="6 3" ' + S + '/>'),
        case("dash/path-arc", '<path d="M8 40 A24 24 0 0 1 56 40" stroke-width="3" '
             'stroke-dasharray="4 4" stroke-linecap="round" ' + S + '/>'),
        case("dash/path-subpaths", '<path d="M8 16h48M8 32h48M8 48h48" stroke-width="2" '
             'stroke-dasharray="7 5" ' + S + '/>'),
        case("dash/path-closed", '<path d="M12 12h40v40h-40z" stroke-width="3" '
             'stroke-dasharray="15 5" stroke-dashoffset="10" ' + S + '/>'),
        case("dash/pathlength-line", line % 'pathLength="10" stroke-dasharray="1 0.5"'),
        case("dash/pathlength-circle", '<circle cx="32" cy="32" r="22" stroke-width="3" '
             'pathLength="100" stroke-dasharray="10 5" ' + S + '/>'),
        case("dash/pathlength-rect", '<rect x="8" y="8" width="48" height="40" stroke-width="2" '
             'pathLength="40" stroke-dasharray="3 2" ' + S + '/>'),
        case("dash/pathlength-path", '<path d="M6 48 C14 4 50 4 58 48" stroke-width="2" '
             'pathLength="1" stroke-dasharray="0.05 0.03" ' + S + '/>'),
        case("dash/pathlength-offset", line % 'pathLength="100" stroke-dasharray="10 5" '
             'stroke-dashoffset="7"'),
        case("dash/pathlength-zero", line % 'pathLength="0" stroke-dasharray="6 4"'),
        case("dash/pathlength-negative", line % 'pathLength="-5" stroke-dasharray="6 4"'),
        case("dash/transform", '<g transform="rotate(30 32 32) scale(1.5)">'
             '<line x1="4" y1="21" x2="38" y2="21" stroke-width="2" stroke-dasharray="4 2" '
             + S + '/></g>'),
        case("dash/viewbox", '<circle cx="12" cy="12" r="9" stroke-width="2" '
             'stroke-dasharray="3 2" ' + S + '/>', size=(48, 48), attrs='viewBox="0 0 24 24"'),
        case("dash/hairline", '<line x1="4" y1="10.5" x2="60" y2="54.5" stroke-width="0.5" '
             'stroke-dasharray="1 1" ' + S + '/>'),
        case("dash/opacity", wide % 'stroke-dasharray="8 3" stroke-opacity="0.5"'),
    ]


def order_cases() -> list[dict]:
    shape = ('<circle cx="32" cy="32" r="18" fill="#93c5fd" stroke="' + INK
             + '" stroke-width="10" stroke-opacity="0.6" paint-order="%s"/>')
    return [
        case("order/normal", shape % "normal"),
        case("order/stroke", shape % "stroke"),
        case("order/stroke-fill", shape % "stroke fill"),
        case("order/fill-stroke", shape % "fill stroke"),
        case("order/markers", shape % "markers"),
        case("order/markers-stroke", shape % "markers stroke"),
        case("order/invalid", shape % "fill fill"),
        case("order/inherit", '<g paint-order="stroke"><rect x="12" y="12" width="40" height="40" '
             f'fill="#93c5fd" stroke="{INK}" stroke-width="8"/></g>'),
    ]


def rendering_cases() -> list[dict]:
    crisp = 'shape-rendering="crispEdges"'
    return [
        case("crisp/rect-half", f'<rect x="8.5" y="8.5" width="47" height="40.5" fill="#3b82f6" {crisp}/>'),
        case("crisp/rect-quarter", f'<rect x="8.25" y="7.75" width="20.5" height="30.4" fill="#3b82f6" {crisp}/>'),
        case("crisp/circle", f'<circle cx="32.3" cy="31.6" r="20.25" fill="#3b82f6" {crisp}/>'),
        case("crisp/stroke-1", f'<rect x="8" y="8" width="48" height="48" stroke-width="1" {S} {crisp}/>'),
        case("crisp/stroke-1-half", f'<rect x="8.5" y="8.5" width="47" height="47" stroke-width="1" {S} {crisp}/>'),
        case("crisp/stroke-3", f'<rect x="8.3" y="8.3" width="47" height="47" stroke-width="3" {S} {crisp}/>'),
        case("crisp/diagonal", f'<line x1="4" y1="10" x2="60" y2="54" stroke-width="2" {S} {crisp}/>'),
        case("crisp/path", f'<path d="M8 40 C8 8 56 8 56 40 Q32 64 8 40z" fill="#93c5fd" '
             f'stroke="{INK}" stroke-width="2" {crisp}/>'),
        case("crisp/rotate", f'<rect x="20" y="24" width="24" height="16" fill="#3b82f6" '
             f'transform="rotate(30 32 32)" {crisp}/>'),
        case("crisp/inherit", f'<g {crisp}><circle cx="32" cy="32" r="20" fill="none" '
             f'stroke="{INK}" stroke-width="2"/></g>'),
        case("crisp/dash-grid", f'<g {crisp} stroke="{GRID}" stroke-dasharray="3 3">'
             '<line x1="0" y1="16.5" x2="64" y2="16.5"/><line x1="0" y1="40" x2="64" y2="40"/>'
             '<line x1="20.5" y1="0" x2="20.5" y2="64"/><line x1="44" y1="0" x2="44" y2="64"/></g>'),
        case("rendering/optimize-speed", '<circle cx="32" cy="32" r="20.5" fill="#3b82f6" '
             'shape-rendering="optimizeSpeed"/>'),
        case("rendering/geometric-precision", '<circle cx="32" cy="32" r="20.5" fill="#3b82f6" '
             'shape-rendering="geometricPrecision"/>'),
        case("rendering/auto", '<circle cx="32" cy="32" r="20.5" fill="#3b82f6" shape-rendering="auto"/>'),
    ]


def non_scaling_cases() -> list[dict]:
    ns = 'vector-effect="non-scaling-stroke"'
    return [
        case("nonscaling/scale", f'<rect x="3" y="3" width="15" height="12" stroke-width="2" {S} {ns} '
             'transform="scale(3)"/>'),
        case("nonscaling/scale-nonuniform", f'<circle cx="10" cy="30" r="8" stroke-width="2" {S} {ns} '
             'transform="scale(3 1)"/>'),
        case("nonscaling/skew", f'<rect x="8" y="16" width="30" height="24" stroke-width="2" {S} {ns} '
             'transform="skewX(25)"/>'),
        case("nonscaling/rotate", f'<rect x="20" y="24" width="24" height="16" stroke-width="3" {S} {ns} '
             'transform="rotate(30 32 32)"/>'),
        case("nonscaling/viewbox", f'<path d="M4 12h16M12 4v16" stroke="{INK}" stroke-width="2" {ns}/>',
             size=(48, 48), attrs='viewBox="0 0 24 24"'),
        case("nonscaling/viewbox-none", f'<circle cx="50" cy="25" r="20" stroke-width="2" {S} {ns}/>',
             size=(60, 60), attrs='viewBox="0 0 100 50" preserveAspectRatio="none"'),
        case("nonscaling/nested",
             '<svg x="4" y="4" width="56" height="56" viewBox="0 0 14 14">'
             f'<circle cx="7" cy="7" r="5" stroke-width="1.5" {S} {ns}/></svg>'),
        case("nonscaling/dash", f'<line x1="2" y1="10" x2="28" y2="10" stroke-width="2" {S} {ns} '
             'stroke-dasharray="4 2" transform="scale(2)"/>'),
        case("nonscaling/fill", f'<rect x="4" y="4" width="16" height="12" fill="#93c5fd" stroke="{INK}" '
             f'stroke-width="2" {ns} transform="scale(2.5)"/>'),
        case("nonscaling/line", f'<line x1="2" y1="2" x2="20" y2="14" stroke-width="1.5" {S} {ns} '
             'transform="scale(2.5)"/>'),
        case("nonscaling/curve", f'<path d="M2 18 C4 2 16 2 18 18" stroke-width="2" {S} {ns} '
             'stroke-linecap="round" transform="scale(3 2.5)"/>'),
        case("nonscaling/group", f'<g {ns} transform="scale(3)"><rect x="3" y="3" width="15" height="12" '
             f'stroke-width="2" {S}/></g>'),
        case("nonscaling/offset", f'<rect x="3" y="3" width="15" height="12" stroke-width="2" {S} {ns} '
             'transform="scale(3)"/>', offset=(0.5, 0.25)),
    ]


def chart_cases() -> list[dict]:
    """Hand-written in the shape of Recharts' output (shadcn charts), with no
    text: grid, lines, dots, bars, an area, a draw-in state and sectors."""
    size = (100, 80)
    grid = (f'<g stroke="{GRID}" stroke-dasharray="3 3" fill="none">'
            + "".join(f'<line x1="0" y1="{y}" x2="100" y2="{y}"/>' for y in (10, 30, 50, 70))
            + "".join(f'<line x1="{x}" y1="0" x2="{x}" y2="80"/>' for x in (12, 37, 62, 87))
            + "</g>")
    line_d = "M5,60C15,60,20,25,30,25C40,25,45,45,55,45C65,45,70,15,80,15C88,15,92,30,95,30"
    dots = "".join(f'<circle cx="{x}" cy="{y}" r="4" fill="#fff" stroke="{BLUE}" stroke-width="2"/>'
                   for x, y in ((5, 60), (30, 25), (55, 45), (80, 15)))
    bars = "".join(f'<path d="M{x},{y + 4}a4,4 0 0 1 4,-4h8a4,4 0 0 1 4,4v{76 - y}h-16z" '
                   f'fill="{BLUE}" fill-opacity="0.8"/>'
                   for x, y in ((6, 30), (30, 12), (54, 44), (78, 22)))
    sectors = "".join(
        f'<path d="M50,40L{x1},{y1}A30,30 0 0 1 {x2},{y2}Z" fill="{c}" stroke="#fff" stroke-width="2"/>'
        for (x1, y1, x2, y2, c) in ((50, 10, 80, 40, "#2563eb"), (80, 40, 50, 70, "#60a5fa"),
                                    (50, 70, 23.5, 54.1, "#93c5fd"), (23.5, 54.1, 50, 10, "#1e40af")))
    return [
        case("chart/grid", grid, size=size),
        case("chart/line", f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="2" '
             'stroke-linejoin="round" stroke-linecap="round"/>', size=size),
        case("chart/line-dots", f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="2"/>'
             + dots, size=size),
        case("chart/bars", bars, size=size),
        case("chart/area", f'<path d="{line_d}L95,80L5,80Z" fill="{BLUE}" fill-opacity="0.4"/>'
             f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="2"/>', size=size),
        case("chart/draw-in", f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="2" '
             'stroke-dasharray="55.5px 200px" stroke-dashoffset="0px"/>', size=size),
        case("chart/reference", grid + '<line x1="50.5" y1="0" x2="50.5" y2="80" stroke="#ef4444" '
             'stroke-width="1" stroke-dasharray="4 4"/>', size=size),
        case("chart/pie", sectors, size=size),
        case("chart/radial", f'<circle cx="50" cy="40" r="30" fill="none" stroke="{GRID}" stroke-width="10"/>'
             f'<circle cx="50" cy="40" r="30" fill="none" stroke="{BLUE}" stroke-width="10" '
             'pathLength="100" stroke-dasharray="68 100" stroke-linecap="round" '
             'transform="rotate(-90 50 40)"/>', size=size),
    ]


def clip_cases() -> list[dict]:
    """`clipPath` (set v3): every case clips the same content, a filled square
    crossed by a thick stroke, so both fills and strokes show the clip edge."""
    content = (f'<rect x="0" y="0" width="64" height="64" fill="{BLUE}"/>'
               '<path d="M0,64L64,0" stroke="#ef4444" stroke-width="6"/>')
    star = "32,4 40,56 6,22 58,22 24,56"

    def clipped(case_id: str, clip: str, *, attrs: str = "", defs: str = "",
                target: str = 'clip-path="url(#c)"', body: str = content, **kw) -> dict:
        return case(case_id, f'<defs><clipPath id="c"{" " + attrs if attrs else ""}>{clip}'
                    f'</clipPath>{defs}</defs><g {target}>{body}</g>', **kw)

    second = '<clipPath id="d"><circle cx="32" cy="32" r="24"/></clipPath>'
    chart_size = (100, 80)
    line_d = "M5,60C15,60,20,25,30,25C40,25,45,45,55,45C65,45,70,15,80,15C88,15,92,30,95,30"
    plot = '<clipPath id="c"><rect x="5" y="5" width="90" height="70"/></clipPath>'
    return [scoped(c, i) for i, c in enumerate([
        # One child: the path fast path.
        clipped("clip/rect", '<rect x="8.5" y="8.25" width="40" height="30"/>'),
        clipped("clip/rect-rounded", '<rect x="8" y="8" width="48" height="40" rx="9"/>'),
        clipped("clip/circle", '<circle cx="32" cy="32" r="22.3"/>'),
        clipped("clip/ellipse", '<ellipse cx="32" cy="30" rx="26" ry="14.5"/>'),
        clipped("clip/path-curve", '<path d="M6,50C6,10,58,10,58,50Q32,62,6,50z"/>'),
        clipped("clip/polygon", f'<polygon points="{star}"/>'),
        clipped("clip/line", '<line x1="0" y1="0" x2="64" y2="64" stroke="#000"/>'),
        clipped("clip/zero-size", '<rect x="8" y="8" width="0" height="30"/>'),
        clipped("clip/stroke-ignored", '<rect x="16" y="16" width="32" height="32" '
                'stroke="#000" stroke-width="12"/>'),
        clipped("clip/fill-none", '<rect x="12" y="12" width="40" height="40" fill="none"/>'),
        # clip-rule.
        clipped("rule/evenodd", f'<polygon points="{star}" clip-rule="evenodd"/>'),
        clipped("rule/inherited", f'<polygon points="{star}"/>', attrs='clip-rule="evenodd"'),
        clipped("rule/fill-rule-ignored", f'<polygon points="{star}" fill-rule="evenodd"/>'),
        # Transforms.
        clipped("xf/child", '<rect x="12" y="20" width="40" height="24" transform="rotate(20 32 32)"/>'),
        clipped("xf/clippath", '<rect x="12" y="20" width="40" height="24"/>',
                attrs='transform="rotate(15 32 32)"'),
        clipped("xf/both", '<rect x="12" y="20" width="40" height="24" transform="scale(1.2)"/>',
                attrs='transform="translate(-4.5 2)"'),
        clipped("xf/target", '<rect x="8" y="8" width="40" height="30"/>',
                target='clip-path="url(#c)" transform="translate(5.5 3.25) scale(0.8)"'),
        case("xf/shape-target", '<defs><clipPath id="c"><circle cx="32" cy="32" r="20"/></clipPath></defs>'
             f'<rect x="4" y="4" width="56" height="56" fill="{BLUE}" clip-path="url(#c)" '
             'transform="rotate(10 32 32)"/>'),
        # objectBoundingBox units.
        clipped("units/obb-rect", '<rect x="0.1" y="0.15" width="0.8" height="0.5"/>',
                attrs='clipPathUnits="objectBoundingBox"'),
        clipped("units/obb-circle", '<circle cx="0.5" cy="0.5" r="0.4"/>',
                attrs='clipPathUnits="objectBoundingBox"',
                body=f'<rect x="4" y="16" width="56" height="32" fill="{BLUE}"/>'),
        clipped("units/obb-transform", '<rect x="0.2" y="0.2" width="0.6" height="0.6"/>',
                attrs='clipPathUnits="objectBoundingBox" transform="rotate(10)"'),
        clipped("units/obb-stroked", '<circle cx="0.5" cy="0.5" r="0.5"/>',
                attrs='clipPathUnits="objectBoundingBox"',
                body=f'<rect x="16" y="16" width="32" height="32" fill="{BLUE}" '
                     'stroke="#ef4444" stroke-width="10"/>'),
        case("units/obb-shape", '<defs><clipPath id="c" clipPathUnits="objectBoundingBox">'
             '<rect x="0.25" y="0" width="0.5" height="1"/></clipPath></defs>'
             f'<circle cx="32" cy="32" r="26" fill="{BLUE}" clip-path="url(#c)"/>'),
        clipped("units/obb-group", '<rect x="0" y="0" width="0.5" height="0.5"/>',
                attrs='clipPathUnits="objectBoundingBox"',
                body=f'<rect x="10" y="10" width="20" height="20" fill="{BLUE}"/>'
                     '<circle cx="44" cy="44" r="12" fill="#ef4444"/>'),
        # Several children.
        clipped("union/disjoint", '<rect x="4" y="4" width="24" height="24"/>'
                '<circle cx="44" cy="44" r="14"/>'),
        clipped("union/overlap", '<rect x="8" y="8" width="32" height="32"/>'
                '<circle cx="40" cy="40" r="18"/>'),
        clipped("union/mixed-rules", f'<polygon points="{star}" clip-rule="evenodd"/>'
                '<rect x="28" y="28" width="8" height="8"/>'),
        clipped("union/three", '<rect x="4" y="4" width="20" height="56"/>'
                '<rect x="22" y="22.5" width="20" height="20"/><circle cx="52" cy="32" r="10"/>'),
        clipped("union/transformed", '<rect x="8" y="8" width="24" height="24" transform="rotate(10 20 20)"/>'
                '<rect x="32" y="32" width="24" height="24"/>'),
        # Nested clips.
        clipped("nest/on-clippath", '<rect x="4" y="4" width="40" height="40"/>',
                attrs='clip-path="url(#d)"', defs=second),
        clipped("nest/on-child", '<rect x="4" y="4" width="40" height="40" clip-path="url(#d)"/>'
                '<rect x="44" y="44" width="16" height="16"/>', defs=second),
        clipped("nest/groups", '<rect x="4" y="4" width="44" height="44"/>', defs=second,
                body=f'<g clip-path="url(#d)">{content}</g>'),
        clipped("nest/chain", '<rect x="4" y="4" width="48" height="48"/>',
                attrs='clip-path="url(#d)"',
                defs='<clipPath id="d" clip-path="url(#e)"><circle cx="32" cy="32" r="26"/></clipPath>'
                     '<clipPath id="e"><rect x="16" y="0" width="48" height="64"/></clipPath>'),
        clipped("nest/union-on-clippath", '<rect x="4" y="4" width="24" height="24"/>'
                '<rect x="30" y="30" width="30" height="30"/>', attrs='clip-path="url(#d)"', defs=second),
        # Edges.
        clipped("edge/display-none", '<rect x="4" y="4" width="56" height="56" display="none"/>'
                '<circle cx="32" cy="32" r="16"/>'),
        clipped("edge/visibility-hidden", '<rect x="4" y="4" width="56" height="56" visibility="hidden"/>'
                '<circle cx="32" cy="32" r="16"/>'),
        clipped("edge/empty", ""),
        clipped("edge/missing", '<rect x="8" y="8" width="20" height="20"/>',
                target='clip-path="url(#nope)"'),
        clipped("edge/not-clippath", '<rect x="8" y="8" width="20" height="20"/>',
                defs='<rect id="r" x="8" y="8" width="20" height="20"/>', target='clip-path="url(#r)"'),
        clipped("edge/self", '<rect x="8" y="8" width="40" height="40"/>', attrs='clip-path="url(#c)"'),
        clipped("edge/cycle", '<rect x="8" y="8" width="40" height="40"/>', attrs='clip-path="url(#d)"',
                defs='<clipPath id="d" clip-path="url(#c)"><circle cx="32" cy="32" r="24"/></clipPath>'),
        clipped("edge/style", '<circle cx="32" cy="32" r="20"/>', target='style="clip-path:url(#c)"'),
        clipped("edge/offset", '<circle cx="32" cy="32" r="20.5"/>', offset=(0.5, 0.25)),
        clipped("edge/crisp-child", '<circle cx="32" cy="32" r="20" shape-rendering="crispEdges"/>'),
        clipped("edge/crisp-target", '<circle cx="32" cy="32" r="20"/>',
                target='clip-path="url(#c)" shape-rendering="crispEdges"'),
        clipped("edge/opacity", '<circle cx="32" cy="32" r="22"/>',
                target='clip-path="url(#c)" opacity="0.5"'),
        clipped("edge/hairline", '<rect x="8" y="8" width="0.4" height="48"/>'
                '<rect x="20.3" y="8" width="0.7" height="48"/>'),
        # Children that are not shapes: named gaps (text needs T1; `use` is V3d).
        clipped("child/text", '<text x="4" y="44" font-size="40" font-weight="700">Ab</text>'),
        clipped("child/use", '<use href="#r"/>',
                defs='<rect id="r" x="10" y="10" width="44" height="30"/>'),
        # Recharts: the plot-area clip on a `g`.
        case("chart/line-clip", f'<defs>{plot}</defs><g clip-path="url(#c)">'
             f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="3"/></g>', size=chart_size),
        case("chart/area-clip", f'<defs>{plot}</defs><g clip-path="url(#c)">'
             f'<path d="{line_d}L95,80L5,80Z" fill="{BLUE}" fill-opacity="0.4"/>'
             f'<path d="{line_d}" fill="none" stroke="{BLUE}" stroke-width="2"/></g>', size=chart_size),
        case("chart/bars-clip", f'<defs>{plot}</defs><g clip-path="url(#c)">'
             + "".join(f'<rect x="{x}" y="{y}" width="16" height="{80 - y}" fill="{BLUE}"/>'
                       for x, y in ((6, 30), (30, 2), (54, 44), (78, 22)))
             + "</g>", size=chart_size),
        case("chart/draw-in-clip", '<defs><clipPath id="c"><rect x="0" y="0" width="47.3" height="80"/>'
             f'</clipPath></defs><g clip-path="url(#c)"><path d="{line_d}" fill="none" stroke="{BLUE}" '
             'stroke-width="2"/></g>', size=chart_size),
        case("chart/dots-clip", f'<defs>{plot}</defs><g clip-path="url(#c)">'
             + "".join(f'<circle cx="{x}" cy="{y}" r="4" fill="#fff" stroke="{BLUE}" stroke-width="2"/>'
                       for x, y in ((5, 60), (30, 25), (55, 45), (80, 15), (95, 75)))
             + "</g>", size=chart_size),
    ])]


def scoped(c: dict, index: int) -> dict:
    """Ids are document-wide and every case of a sheet shares one page, so
    each case's ids (and its references to them) get the case's own prefix."""
    prefix = f"k{index}-"
    svg = (c["svg"].replace('id="', f'id="{prefix}').replace("url(#", f"url(#{prefix}")
           .replace('href="#', f'href="#{prefix}'))
    return {**c, "svg": svg}


SCALE_2 = {
    "dash/line", "dash/offset", "dash/dots-round", "dash/rect-grid", "dash/rect-rounded",
    "dash/circle", "dash/path-curve", "dash/pathlength-circle", "dash/hairline", "dash/viewbox",
    "crisp/rect-half", "crisp/rect-quarter", "crisp/circle", "crisp/stroke-1", "crisp/stroke-1-half",
    "crisp/diagonal", "crisp/dash-grid", "rendering/optimize-speed",
    "nonscaling/scale", "nonscaling/scale-nonuniform", "nonscaling/viewbox", "nonscaling/dash",
    "nonscaling/nested", "nonscaling/offset",
    "order/stroke", "chart/grid", "chart/line-dots", "chart/bars", "chart/draw-in", "chart/radial",
}


def sheets(corpus: str = "v1") -> list[dict]:
    """The cases of each sheet. v1: everything at 1x, the Lucide icons at 2x.
    v2: everything at 1x, the `SCALE_2` subset at 2x. v3: everything at both."""
    if corpus == "v3":
        everything = clip_cases()
        return [
            {"scale": 1, "cases": everything},
            {"scale": 2, "cases": [dict(c) for c in everything]},
        ]
    if corpus == "v2":
        everything = (dash_cases() + order_cases() + rendering_cases() + non_scaling_cases()
                      + chart_cases())
        ids = {c["id"] for c in everything}
        assert SCALE_2 <= ids, SCALE_2 - ids
        return [
            {"scale": 1, "cases": everything},
            {"scale": 2, "cases": [dict(c) for c in everything if c["id"] in SCALE_2]},
        ]
    everything = fill_cases() + stroke_cases() + transform_cases() + viewport_cases()
    lucide = lucide_cases()
    return [
        {"scale": 1, "cases": everything + lucide},
        {"scale": 2, "cases": [dict(c) for c in lucide]},
    ]


def layout(sheet: dict) -> None:
    """Give each case a fixed slot; size the sheet to hold them."""
    for index, c in enumerate(sheet["cases"]):
        assert c["width"] + 1 < SLOT and c["height"] + 1 < SLOT, c["id"]
        c["x"] = (index % COLUMNS) * SLOT
        c["y"] = (index // COLUMNS) * SLOT
    rows = (len(sheet["cases"]) + COLUMNS - 1) // COLUMNS
    sheet["width"] = COLUMNS * SLOT
    sheet["height"] = rows * SLOT


def page_html(sheet: dict) -> str:
    slots = "\n".join(
        f'<div style="left:{c["x"] + c["offset"][0]}px;top:{c["y"] + c["offset"][1]}px">'
        f'{c["svg"]}</div>'
        for c in sheet["cases"]
    )
    return ("<!doctype html><meta charset=utf-8>"
            f"<style>html{{overflow:hidden;background:#fff;color:{INK}}}"
            "body{margin:0}div{position:absolute}svg{display:block}</style>\n"
            f"{slots}\n")


def browser_version(chrome: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="div-svg-paint-", ignore_cleanup_errors=True) as tmp, ExitStack() as stack:
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    lst = sub.add_parser("list")
    lst.add_argument("--set", dest="corpus", choices=("v1", "v2", "v3"), default="v1")
    cap = sub.add_parser("capture")
    cap.add_argument("--set", dest="corpus", choices=("v1", "v2", "v3"), default="v1")
    cap.add_argument("--chrome", type=Path, required=True)
    cap.add_argument("--output-prefix", type=Path, required=True)
    args = parser.parse_args()

    all_sheets = sheets(args.corpus)
    for sheet in all_sheets:
        ids = [c["id"] for c in sheet["cases"]]
        if len(set(ids)) != len(ids):
            raise SystemExit("duplicate case ids")
        layout(sheet)
    if args.command == "list":
        for sheet in all_sheets:
            for c in sheet["cases"]:
                print(f'{sheet["scale"]}x\t{c["id"]}\t{c["svg"]}')
        print(f"{sum(len(s['cases']) for s in all_sheets)} captures", file=sys.stderr)
        return 0

    version = browser_version(args.chrome)
    prefix = args.output_prefix
    prefix.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="div-svg-paint-", ignore_cleanup_errors=True) as tmp:
        for sheet in all_sheets:
            page = Path(tmp) / f"sheet-{sheet['scale']}x.html"
            page.write_text(page_html(sheet), encoding="utf-8")
            png = prefix.parent / f"{prefix.name}-{sheet['scale']}x.png"
            for attempt in range(3):
                if pixel_runner.render_chrome(
                    str(page), str(png), str(args.chrome), str(args.chrome.parent),
                    logical_width=sheet["width"], logical_height=sheet["height"],
                    device_scale=float(sheet["scale"]),
                ):
                    break
                print(f"{sheet['scale']}x capture attempt {attempt + 1} failed", file=sys.stderr)
                time.sleep(1)
            else:
                raise SystemExit(f"could not capture the {sheet['scale']}x sheet")
            sheet["png"] = png.name
            sheet["png_sha256"] = hashlib.sha256(png.read_bytes()).hexdigest()
            sheet["method"] = pixel_runner.device_scale_method(sheet["scale"])

    corpus_text = json.dumps([{k: v for k, v in s.items() if not k.startswith("png")}
                              for s in all_sheets], sort_keys=True)
    header = {
        "schema": SCHEMA,
        "captured": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "chromium": version,
        "pinned": EXPECTED_CHROMIUM in version,
        "platform": sys.platform,
        "corpus_sha256": hashlib.sha256(corpus_text.encode()).hexdigest(),
    }
    lines = ["{"]
    lines += [f"{json.dumps(k)}: {json.dumps(v)}," for k, v in sorted(header.items())]
    lines.append('"sheets": [')
    sheet_texts = []
    for sheet in all_sheets:
        meta = {k: v for k, v in sheet.items() if k != "cases"}
        cases = ",\n".join(json.dumps(c, sort_keys=True) for c in sheet["cases"])
        sheet_texts.append(json.dumps(meta, sort_keys=True)[:-1] + ', "cases": [\n' + cases + "]}")
    lines.append(",\n".join(sheet_texts))
    lines.append("]}")
    out = prefix.parent / f"{prefix.name}.json"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    json.loads(out.read_text(encoding="utf-8"))
    print(f"captured {[len(s['cases']) for s in all_sheets]} cases from {version} into {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
