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

There are seven sets, each with its own fixtures and gap list: `v1` (fills,
strokes, transforms, viewports and the Lucide icons), `v2` (dashes,
`pathLength`, `paint-order`, `shape-rendering`, `vector-effect` and
chart-shaped compositions), `v3` (`clipPath`), `v4` (paint servers:
gradients and patterns), `v5` (`mask`), `v6` (`use` and `symbol`) and
`v7` (markers). Every case of `v3` to `v7` is taken at both scales.

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


def server_cases() -> list[dict]:
    """Paint servers (set v4): `linearGradient`, `radialGradient` and `pattern`
    as `fill` and `stroke`, written from SVG 2 §14 and checked against
    Chromium's pixels. Colours differ in every channel so that the
    interpolation, premultiplication and dithering all show."""
    two = '<stop offset="0" stop-color="#2563eb"/><stop offset="1" stop-color="#f97316"/>'
    three = ('<stop offset="0" stop-color="#2563eb"/><stop offset="0.5" stop-color="#facc15"/>'
             '<stop offset="1" stop-color="#dc2626"/>')
    rect = '<rect x="4" y="4" width="56" height="56" fill="url(#g)"/>'

    def lin(case_id: str, stops: str = two, *, attrs: str = "", body: str = rect,
            defs: str = "", **kw) -> dict:
        return case(case_id, f'<defs><linearGradient id="g"{" " + attrs if attrs else ""}>{stops}'
                    f'</linearGradient>{defs}</defs>{body}', **kw)

    def rad(case_id: str, stops: str = two, *, attrs: str = "", body: str = rect,
            defs: str = "", **kw) -> dict:
        return case(case_id, f'<defs><radialGradient id="g"{" " + attrs if attrs else ""}>{stops}'
                    f'</radialGradient>{defs}</defs>{body}', **kw)

    tile = (f'<rect x="0" y="0" width="8" height="8" fill="{BLUE}"/>'
            '<circle cx="12" cy="12" r="3.5" fill="#f97316"/>')

    def pat(case_id: str, content: str = tile, *, attrs: str = 'width="16" height="16" '
            'patternUnits="userSpaceOnUse"', body: str = rect, defs: str = "", **kw) -> dict:
        return case(case_id, f'<defs><pattern id="g" {attrs}>{content}</pattern>{defs}</defs>'
                    f'{body}', **kw)

    stroke_body = ('<rect x="10" y="10" width="44" height="44" fill="none" stroke="url(#g)" '
                   'stroke-width="8"/>')
    wide = '<rect x="4" y="14" width="56" height="36" fill="url(#g)"/>'
    area = ("M4,56L4,40C14,40,18,18,28,18C38,18,42,34,52,30L60,26L60,56Z")
    return [scoped(c, i) for i, c in enumerate([
        # linearGradient: geometry and units.
        lin("lin/basic"),
        lin("lin/vertical", attrs='x2="0" y2="1"'),
        lin("lin/diagonal", attrs='x2="1" y2="1"', body=wide),
        lin("lin/fraction", attrs='x1="0.2" y1="0.1" x2="0.7" y2="0.4"'),
        lin("lin/percent", attrs='x1="10%" x2="90%" y2="25%"'),
        lin("lin/user", attrs='gradientUnits="userSpaceOnUse" x1="8.5" y1="4" x2="55.25" y2="60"'),
        lin("lin/user-percent", attrs='gradientUnits="userSpaceOnUse" x1="10%" x2="75%"'),
        case("lin/user-viewbox", '<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" '
             f'x1="0" x2="20">{two}</linearGradient></defs>'
             '<rect x="2" y="2" width="28" height="28" fill="url(#g)"/>', attrs='viewBox="0 0 32 32"'),
        # Stops.
        lin("stop/three", three),
        lin("stop/percent", '<stop offset="25%" stop-color="#2563eb"/>'
            '<stop offset="75%" stop-color="#f97316"/>'),
        lin("stop/unordered", '<stop offset="0.6" stop-color="#2563eb"/>'
            '<stop offset="0.3" stop-color="#facc15"/><stop offset="1" stop-color="#dc2626"/>'),
        lin("stop/out-of-range", '<stop offset="-0.5" stop-color="#2563eb"/>'
            '<stop offset="1.5" stop-color="#f97316"/>'),
        lin("stop/inner", '<stop offset="0.3" stop-color="#2563eb"/>'
            '<stop offset="0.6" stop-color="#f97316"/>'),
        lin("stop/hard", '<stop offset="0.5" stop-color="#2563eb"/>'
            '<stop offset="0.5" stop-color="#f97316"/>'),
        lin("stop/opacity", '<stop offset="0" stop-color="#dc2626"/>'
            '<stop offset="1" stop-color="#2563eb" stop-opacity="0"/>'),
        lin("stop/opacity-half", '<stop offset="0" stop-color="#dc2626" stop-opacity="0.5"/>'
            '<stop offset="1" stop-color="#2563eb" stop-opacity="0.8"/>'),
        lin("stop/style", '<stop offset="0" style="stop-color:#16a34a"/>'
            '<stop offset="1" style="stop-color:#9333ea;stop-opacity:0.6"/>'),
        lin("stop/current-color", '<stop offset="0" stop-color="currentColor"/>'
            '<stop offset="1" stop-color="#f97316"/>', attrs='color="#16a34a"'),
        lin("stop/one", '<stop offset="0.4" stop-color="#16a34a"/>'),
        lin("stop/none", ""),
        # Degenerate lines and spreadMethod.
        lin("lin/degenerate", attrs='x1="0.5" x2="0.5"'),
        lin("lin/degenerate-repeat", attrs='x1="0.5" x2="0.5" spreadMethod="repeat"'),
        lin("spread/pad", three, attrs='x1="0.3" x2="0.6"'),
        lin("spread/reflect", three, attrs='x1="0.3" x2="0.6" spreadMethod="reflect"'),
        lin("spread/repeat", three, attrs='x1="0.3" x2="0.6" spreadMethod="repeat"'),
        lin("spread/repeat-diagonal", attrs='x2="0.25" y2="0.25" spreadMethod="repeat"'),
        # gradientTransform.
        lin("xf/rotate", attrs='gradientTransform="rotate(30)"'),
        lin("xf/rotate-center", attrs='gradientTransform="rotate(45 0.5 0.5)"', body=wide),
        lin("xf/user", attrs='gradientUnits="userSpaceOnUse" x1="8" x2="40" '
            'gradientTransform="translate(4.5 0) skewX(20)"'),
        lin("xf/shape", attrs='x2="1"', body='<rect x="12" y="12" width="40" height="40" '
            'fill="url(#g)" transform="rotate(20 32 32)"/>'),
        # Paint properties.
        lin("paint/stroke", body=stroke_body),
        lin("paint/stroke-user", attrs='gradientUnits="userSpaceOnUse" x1="6" x2="58"',
            body=stroke_body),
        lin("paint/fill-opacity", body='<rect x="4" y="4" width="56" height="56" fill="url(#g)" '
            'fill-opacity="0.5"/>'),
        lin("paint/fill-opacity-odd", body='<rect x="4" y="4" width="56" height="56" '
            'fill="url(#g)" fill-opacity="0.37"/>'),
        lin("paint/circle", body='<circle cx="32" cy="32" r="26.5" fill="url(#g)"/>'),
        lin("paint/path", attrs='x2="0" y2="1"', body=f'<path d="{area}" fill="url(#g)"/>'),
        lin("paint/group", body='<g fill="url(#g)"><rect x="4" y="4" width="24" height="56"/>'
            '<rect x="34" y="20" width="26" height="20"/></g>'),
        lin("paint/both", body='<rect x="10" y="10" width="44" height="44" fill="url(#g)" '
            'stroke="url(#g)" stroke-width="6" paint-order="stroke"/>'),
        lin("paint/crisp", attrs='gradientTransform="rotate(20)"',
            body='<rect x="4.5" y="4.5" width="55" height="55" fill="url(#g)" '
            'shape-rendering="crispEdges"/>'),
        # References: href inheritance, fallbacks and empty boxes.
        lin("href/stops", "", attrs='href="#h" x2="0" y2="1"',
            defs=f'<linearGradient id="h">{three}</linearGradient>'),
        lin("href/attrs", two, attrs='href="#h"',
            defs='<linearGradient id="h" x1="0.3" x2="0.6" spreadMethod="reflect" '
            'gradientTransform="rotate(10)"/>'),
        lin("href/chain", "", attrs='href="#h"',
            defs=f'<linearGradient id="h" href="#i" x2="0.5"/><linearGradient id="i" '
            f'spreadMethod="repeat">{three}</linearGradient>'),
        lin("href/own-stops", two, attrs='href="#h"',
            defs=f'<linearGradient id="h">{three}</linearGradient>'),
        lin("href/xlink", "", attrs='xlink:href="#h"',
            defs=f'<linearGradient id="h">{three}</linearGradient>'),
        lin("href/cycle", two, attrs='href="#h" x2="0.5"',
            defs='<linearGradient id="h" href="#g" spreadMethod="reflect"/>'),
        rad("href/radial-from-linear", "", attrs='href="#h" r="0.4"',
            defs=f'<linearGradient id="h" x1="0.9" spreadMethod="reflect">{three}</linearGradient>'),
        case("ref/missing", '<rect x="4" y="4" width="56" height="56" fill="url(#nothing)"/>'),
        case("ref/missing-fallback", '<rect x="4" y="4" width="56" height="56" '
             f'fill="url(#nothing) {BLUE}"/>'),
        case("ref/not-a-server", f'<defs><rect id="g" width="4" height="4"/></defs>'
             f'<rect x="4" y="4" width="56" height="56" fill="url(#g) {BLUE}"/>'),
        lin("ref/empty-box", body='<line x1="4" y1="32" x2="60" y2="32" stroke="url(#g)" '
            'stroke-width="10"/>'),
        lin("ref/empty-box-fallback", body='<line x1="4" y1="32" x2="60" y2="32" '
            'stroke="url(#g) #dc2626" stroke-width="10"/>'),
        lin("ref/empty-box-user", attrs='gradientUnits="userSpaceOnUse" x1="4" x2="60"',
            body='<line x1="4" y1="32" x2="60" y2="32" stroke="url(#g)" stroke-width="10"/>'),
        lin("ref/no-stops-fallback", "", body='<rect x="4" y="4" width="56" height="56" '
            f'fill="url(#g) {BLUE}"/>'),
        # radialGradient.
        rad("rad/basic"),
        rad("rad/three", three),
        rad("rad/user", attrs='gradientUnits="userSpaceOnUse" cx="30.5" cy="33" r="25"'),
        rad("rad/wide", body=wide),
        rad("rad/focal", three, attrs='fx="0.3" fy="0.35"'),
        rad("rad/focal-edge", three, attrs='fx="0.05" fy="0.5" r="0.45"'),
        rad("rad/focal-outside", three, attrs='cx="0.6" fx="0.05" fy="0.5" r="0.3"'),
        rad("rad/fr", three, attrs='fr="0.2"'),
        rad("rad/fr-focal", three, attrs='fx="0.35" fy="0.4" fr="0.1" r="0.45"'),
        rad("rad/fr-larger", three, attrs='fr="0.5" r="0.3"'),
        rad("rad/zero-r", attrs='r="0"'),
        rad("rad/reflect", three, attrs='r="0.2" spreadMethod="reflect"'),
        rad("rad/repeat", three, attrs='r="0.15" fx="0.4" spreadMethod="repeat"'),
        rad("rad/transform", attrs='gradientTransform="translate(0.5 0.5) scale(1 0.5) '
            'rotate(30) translate(-0.5 -0.5)"'),
        rad("rad/stroke", three, body=stroke_body),
        rad("rad/circle", body='<circle cx="32" cy="32" r="26.5" fill="url(#g)"/>'),
        rad("rad/href-focal", "", attrs='href="#h" cx="0.6"',
            defs=f'<radialGradient id="h" fx="0.2" r="0.4">{three}</radialGradient>'),
        rad("rad/opacity", '<stop offset="0" stop-color="#facc15"/>'
            '<stop offset="1" stop-color="#2563eb" stop-opacity="0"/>'),
        # pattern.
        pat("pat/user"),
        pat("pat/user-offset", attrs='x="5" y="3" width="16" height="16" patternUnits="userSpaceOnUse"'),
        pat("pat/user-fraction", attrs='x="0.5" y="0.25" width="7.5" height="9.25" '
            'patternUnits="userSpaceOnUse"'),
        pat("pat/obb", attrs='width="0.25" height="0.25"'),
        pat("pat/obb-fraction", attrs='x="0.1" width="0.3" height="0.35"', body=wide),
        pat("pat/content-obb", f'<rect x="0" y="0" width="0.1" height="0.1" fill="{BLUE}"/>'
            '<circle cx="0.15" cy="0.15" r="0.05" fill="#f97316"/>',
            attrs='width="0.25" height="0.25" patternContentUnits="objectBoundingBox"', body=wide),
        pat("pat/viewbox", attrs='width="16" height="16" patternUnits="userSpaceOnUse" '
            'viewBox="0 0 10 10"', content=f'<rect x="0" y="0" width="5" height="5" fill="{BLUE}"/>'
            '<circle cx="7.5" cy="7.5" r="2" fill="#f97316"/>'),
        pat("pat/viewbox-slice", attrs='width="20" height="12" patternUnits="userSpaceOnUse" '
            'viewBox="0 0 10 10" preserveAspectRatio="xMidYMin slice"',
            content=f'<rect x="0" y="0" width="5" height="5" fill="{BLUE}"/>'
            '<circle cx="7.5" cy="7.5" r="2" fill="#f97316"/>'),
        pat("pat/viewbox-meet", attrs='width="20" height="12" patternUnits="userSpaceOnUse" '
            'viewBox="0 0 10 10"', content=f'<rect x="0" y="0" width="5" height="5" fill="{BLUE}"/>'
            '<circle cx="7.5" cy="7.5" r="2" fill="#f97316"/>'),
        pat("pat/transform", attrs='width="16" height="16" patternUnits="userSpaceOnUse" '
            'patternTransform="rotate(30)"'),
        pat("pat/transform-scale", attrs='width="16" height="16" patternUnits="userSpaceOnUse" '
            'patternTransform="translate(2.5 1) scale(1.5)"'),
        pat("pat/overflow", f'<circle cx="8" cy="8" r="9" fill="{BLUE}"/>'),
        pat("pat/overflow-fraction", f'<circle cx="4" cy="4" r="5" fill="{BLUE}"/>',
            attrs='width="7.5" height="7.5" patternUnits="userSpaceOnUse"'),
        pat("pat/stroke", body=stroke_body),
        pat("pat/fill-opacity", body='<rect x="4" y="4" width="56" height="56" fill="url(#g)" '
            'fill-opacity="0.5"/>'),
        pat("pat/circle", body='<circle cx="32" cy="32" r="26.5" fill="url(#g)"/>'),
        pat("pat/shape-transform", body='<rect x="12" y="12" width="40" height="40" '
                                        'fill="url(#g)" transform="rotate(20 32 32)"/>'),
        pat("pat/stroked-content", '<path d="M0,16L16,0" stroke="#dc2626" stroke-width="3"/>'),
        pat("pat/gradient-content", '<rect x="0" y="0" width="12" height="12" fill="url(#h)"/>',
            defs=f'<linearGradient id="h">{two}</linearGradient>'),
        pat("pat/nested", '<rect x="0" y="0" width="16" height="16" fill="url(#h)"/>',
            defs=f'<pattern id="h" width="4" height="4" patternUnits="userSpaceOnUse">'
            f'<rect width="2" height="2" fill="{BLUE}"/></pattern>'),
        pat("pat/href", "", attrs='href="#h" width="20" height="20" patternUnits="userSpaceOnUse"',
            defs=f'<pattern id="h" width="10" height="10">{tile}</pattern>'),
        pat("pat/href-own", f'<rect width="6" height="6" fill="#16a34a"/>', attrs='href="#h"',
            defs=f'<pattern id="h" width="12" height="12" patternUnits="userSpaceOnUse">{tile}</pattern>'),
        pat("pat/zero-width", attrs='width="0" height="16" patternUnits="userSpaceOnUse"'),
        pat("pat/zero-width-fallback", attrs='width="0" height="16" patternUnits="userSpaceOnUse"',
            body=f'<rect x="4" y="4" width="56" height="56" fill="url(#g) {BLUE}"/>'),
        pat("pat/empty", ""),
        pat("pat/self", f'<rect width="8" height="8" fill="url(#g) {BLUE}"/>'
            '<circle cx="12" cy="12" r="3.5" fill="#f97316"/>'),
        pat("pat/inherited-style", '<rect x="0" y="0" width="8" height="8"/>',
            attrs='width="16" height="16" patternUnits="userSpaceOnUse" fill="#16a34a"',
            body='<rect x="4" y="4" width="56" height="56" fill="url(#g)" stroke="#000"/>'),
        pat("pat/checker", f'<rect width="5" height="5" fill="{INK}"/>'
            f'<rect x="5" y="5" width="5" height="5" fill="{INK}"/>',
            attrs='width="10" height="10" patternUnits="userSpaceOnUse"'),
        # Chart-shaped compositions.
        case("chart/area", '<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">'
             f'<stop offset="5%" stop-color="{BLUE}" stop-opacity="0.8"/>'
             f'<stop offset="95%" stop-color="{BLUE}" stop-opacity="0"/></linearGradient></defs>'
             f'<path d="{area}" fill="url(#g)" stroke="{BLUE}" stroke-width="2"/>'),
        case("chart/bars", '<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">'
             '<stop offset="0" stop-color="#8b5cf6"/><stop offset="1" stop-color="#8b5cf6" '
             'stop-opacity="0.3"/></linearGradient></defs>'
             + "".join(f'<rect x="{x}" y="{y}" width="10" height="{60 - y}" fill="url(#g)"/>'
                       for x, y in ((6, 30), (20, 12), (34, 40), (48, 22)))),
        case("chart/radial", '<defs><radialGradient id="g" cx="0.5" cy="0.5" r="0.5">'
             '<stop offset="0.6" stop-color="#f97316"/><stop offset="1" stop-color="#facc15"/>'
             '</radialGradient></defs><path d="M32,32L32,4A28,28,0,0,1,58.6,40.7Z" fill="url(#g)"/>'
             '<circle cx="32" cy="32" r="12" fill="#fff"/>'),
        case("chart/hatch", f'<defs><pattern id="g" width="6" height="6" '
             'patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
             f'<rect width="2" height="6" fill="{BLUE}"/></pattern></defs>'
             f'<rect x="8" y="16" width="20" height="44" fill="url(#g)" stroke="{BLUE}"/>'
             '<rect x="36" y="28" width="20" height="32" fill="url(#g)"/>'),
    ])]


def mask_cases() -> list[dict]:
    """`mask` (set v5): most cases mask the same content as `clip_cases`, a
    filled square crossed by a thick stroke, so the mask's coverage, its
    region and its edges all show."""
    content = (f'<rect x="0" y="0" width="64" height="64" fill="{BLUE}"/>'
               '<path d="M0,64L64,0" stroke="#ef4444" stroke-width="6"/>')
    star = "32,4 40,56 6,22 58,22 24,56"
    white = 'fill="#fff"'

    def masked(case_id: str, mask: str, *, attrs: str = "", defs: str = "",
               target: str = 'mask="url(#m)"', body: str = content, **kw) -> dict:
        return case(case_id, f'<defs><mask id="m"{" " + attrs if attrs else ""}>{mask}'
                    f'</mask>{defs}</defs><g {target}>{body}</g>', **kw)

    fade = ('<linearGradient id="g"><stop offset="0" stop-color="#fff"/>'
            '<stop offset="1" stop-color="#000"/></linearGradient>')
    chart_size = (100, 80)
    area_d = ("M5,60C15,60,20,25,30,25C40,25,45,45,55,45C65,45,70,15,80,15C88,15,92,30,95,30"
              "L95,80L5,80Z")
    return [scoped(c, i) for i, c in enumerate([
        # Luminance coverage (the initial mask-type).
        masked("lum/white", f'<rect x="8" y="8" width="48" height="40" {white}/>'),
        masked("lum/gray", '<rect x="8" y="8" width="48" height="48" fill="#808080"/>'),
        masked("lum/black", '<rect x="8" y="8" width="48" height="48" fill="#000"/>'),
        masked("lum/red", '<rect x="8" y="8" width="48" height="48" fill="#f00"/>'),
        masked("lum/green", '<rect x="8" y="8" width="48" height="48" fill="#0f0"/>'),
        masked("lum/blue", '<rect x="8" y="8" width="48" height="48" fill="#00f"/>'),
        masked("lum/colors", '<rect x="0" y="0" width="22" height="64" fill="#f97316"/>'
               '<rect x="22" y="0" width="21" height="64" fill="#22c55e"/>'
               '<rect x="43" y="0" width="21" height="64" fill="#a855f7"/>'),
        masked("lum/fill-opacity", f'<rect x="8" y="8" width="48" height="48" {white} '
               'fill-opacity="0.5"/>'),
        masked("lum/opacity", f'<rect x="8" y="8" width="48" height="48" {white} opacity="0.3"/>'),
        masked("lum/circle", f'<circle cx="32" cy="32" r="22.3" {white}/>'),
        masked("lum/star", f'<polygon points="{star}" {white}/>'),
        masked("lum/stroke", '<circle cx="32" cy="32" r="18" fill="none" stroke="#fff" '
               'stroke-width="8"/>'),
        masked("lum/overlap", f'<rect x="4" y="4" width="40" height="40" fill="#808080"/>'
               f'<circle cx="40" cy="40" r="20" {white} fill-opacity="0.6"/>'),
        masked("lum/cutout", f'<rect x="0" y="0" width="64" height="64" {white}/>'
               '<circle cx="48" cy="48" r="12" fill="#000"/>'),
        # mask-type.
        masked("type/alpha", '<rect x="8" y="8" width="48" height="48" fill="#000"/>',
               attrs='mask-type="alpha"'),
        masked("type/alpha-half", '<rect x="8" y="8" width="48" height="48" fill="#f00" '
               'fill-opacity="0.5"/>', attrs='mask-type="alpha"'),
        masked("type/alpha-style", '<rect x="8" y="8" width="48" height="48" fill="#000"/>',
               attrs='style="mask-type: alpha"'),
        masked("type/luminance", '<rect x="8" y="8" width="48" height="48" fill="#808080"/>',
               attrs='mask-type="luminance"'),
        # The mask region.
        masked("region/default", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               body=f'<rect x="12" y="12" width="40" height="40" fill="{BLUE}" '
                    'stroke="#ef4444" stroke-width="10"/>'),
        masked("region/obb", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='x="0.1" y="0.2" width="0.7" height="0.5"'),
        masked("region/obb-percent", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='x="15%" y="5%" width="60%" height="80%"'),
        masked("region/user", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='maskUnits="userSpaceOnUse" x="8.5" y="10.25" width="40" height="30.5"'),
        masked("region/user-percent", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='maskUnits="userSpaceOnUse" x="10%" y="20%" width="50%" height="50%"'),
        masked("region/user-default", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='maskUnits="userSpaceOnUse"'),
        masked("region/zero", f'<rect x="8" y="8" width="48" height="48" {white}/>',
               attrs='width="0"'),
        masked("region/negative", f'<rect x="8" y="8" width="48" height="48" {white}/>',
               attrs='height="-1"'),
        masked("region/rotated", f'<rect x="-20" y="-20" width="104" height="104" {white}/>',
               attrs='x="0.2" y="0.2" width="0.6" height="0.6"',
               target='mask="url(#m)" transform="rotate(20 32 32)"'),
        # maskContentUnits.
        masked("units/content-obb", f'<rect x="0.1" y="0.25" width="0.8" height="0.5" {white}/>',
               attrs='maskContentUnits="objectBoundingBox"'),
        masked("units/content-obb-circle", f'<circle cx="0.5" cy="0.5" r="0.4" {white}/>',
               attrs='maskContentUnits="objectBoundingBox"',
               body=f'<rect x="4" y="16" width="56" height="32" fill="{BLUE}"/>'),
        masked("units/content-obb-transform", f'<rect x="0.2" y="0.2" width="0.6" height="0.6" '
               f'{white} transform="rotate(10)"/>', attrs='maskContentUnits="objectBoundingBox"'),
        masked("units/content-user-region-user",
               f'<rect x="0" y="0" width="64" height="64" {white}/>',
               attrs='maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse" '
                     'x="16" y="16" width="32" height="32"'),
        # Gradients and patterns in the mask.
        masked("grad/fade", '<rect x="0" y="0" width="64" height="64" fill="url(#g)"/>',
               defs=fade),
        masked("grad/fade-vertical", '<rect x="0" y="0" width="64" height="64" fill="url(#g)"/>',
               defs='<linearGradient id="g" x2="0" y2="1"><stop offset="0.2" stop-color="#fff"/>'
                    '<stop offset="1" stop-color="#000"/></linearGradient>'),
        masked("grad/spot", '<rect x="0" y="0" width="64" height="64" fill="url(#g)"/>',
               defs='<radialGradient id="g"><stop offset="0.3" stop-color="#fff"/>'
                    '<stop offset="1" stop-color="#000"/></radialGradient>'),
        masked("grad/alpha-stops", '<rect x="0" y="0" width="64" height="64" fill="url(#g)"/>',
               attrs='mask-type="alpha"',
               defs='<linearGradient id="g"><stop offset="0" stop-color="#000"/>'
                    '<stop offset="1" stop-color="#000" stop-opacity="0"/></linearGradient>'),
        masked("grad/pattern", '<rect x="0" y="0" width="64" height="64" fill="url(#p)"/>',
               defs=f'<pattern id="p" width="8" height="8" patternUnits="userSpaceOnUse">'
                    f'<rect width="4" height="8" {white}/></pattern>'),
        # Targets.
        case("target/rect", f'<defs><mask id="m"><circle cx="32" cy="32" r="20" {white}/></mask>'
             f'</defs><rect x="4" y="4" width="56" height="56" fill="{BLUE}" mask="url(#m)"/>'),
        case("target/circle", '<defs><mask id="m"><rect x="0" y="0" width="64" height="32" '
             f'{white}/></mask></defs><circle cx="32" cy="32" r="26" fill="{BLUE}" '
             'mask="url(#m)"/>'),
        case("target/stroked", f'<defs><mask id="m"><rect x="-10" y="-10" width="84" height="84" '
             f'{white}/></mask></defs><rect x="14" y="14" width="36" height="36" fill="{BLUE}" '
             'stroke="#ef4444" stroke-width="12" mask="url(#m)"/>'),
        masked("target/transform", f'<rect x="8" y="8" width="40" height="30" {white}/>',
               target='mask="url(#m)" transform="translate(5.5 3.25) scale(0.8)"'),
        masked("target/offset", f'<circle cx="32" cy="32" r="20" {white}/>', offset=(0.5, 0.25)),
        # With other effects.
        masked("nest/mask-on-child", f'<rect x="0" y="0" width="64" height="64" {white} '
               'mask="url(#n)"/>',
               defs=f'<mask id="n"><circle cx="32" cy="32" r="22" {white}/></mask>'),
        masked("nest/clip-on-child", f'<rect x="0" y="0" width="64" height="64" {white} '
               'clip-path="url(#c)"/>',
               defs='<clipPath id="c"><circle cx="32" cy="32" r="22"/></clipPath>'),
        masked("nest/clip-and-mask", f'<rect x="0" y="0" width="40" height="64" {white}/>',
               defs='<clipPath id="c"><circle cx="32" cy="32" r="24"/></clipPath>',
               target='mask="url(#m)" clip-path="url(#c)"'),
        masked("nest/opacity", '<rect x="8" y="8" width="48" height="48" fill="#808080"/>',
               target='mask="url(#m)" opacity="0.5"'),
        masked("nest/nested-groups", f'<rect x="0" y="0" width="40" height="64" {white}/>',
               defs=f'<mask id="n"><rect x="0" y="24" width="64" height="40" {white}/></mask>',
               body=f'<g mask="url(#n)">{content}</g>'),
        # Edges.
        masked("edge/missing", '', target='mask="url(#nothing)"'),
        masked("edge/not-a-mask", '', defs='<rect id="r" width="10" height="10"/>',
               target='mask="url(#r)"'),
        masked("edge/empty", ''),
        masked("edge/hidden-child", f'<rect x="8" y="8" width="48" height="48" {white} '
               'visibility="hidden"/>'
               f'<rect x="20" y="20" width="24" height="24" {white} display="none"/>'
               f'<circle cx="32" cy="32" r="10" {white}/>'),
        masked("edge/self", f'<rect x="8" y="8" width="48" height="48" {white} mask="url(#m)"/>'
               f'<circle cx="32" cy="32" r="10" {white}/>'),
        masked("edge/style", f'<rect x="8" y="8" width="40" height="30" {white}/>',
               target='style="mask: url(#m)"'),
        masked("edge/crisp", f'<circle cx="32" cy="32" r="20.5" {white} '
               'shape-rendering="crispEdges"/>'),
        masked("edge/hairline", '<path d="M4,10H60M4,20.5H60M4,31.25H60" stroke="#fff" '
               'stroke-width="0.5"/>'),
        case("edge/line-target", f'<defs><mask id="m"><rect x="-10" y="-10" width="84" '
             f'height="84" {white}/></mask></defs><line x1="4" y1="32" x2="60" y2="32" '
             f'stroke="{BLUE}" stroke-width="6" mask="url(#m)"/>'),
        case("edge/line-target-user", '<defs><mask id="m" maskUnits="userSpaceOnUse">'
             f'<rect x="-10" y="-10" width="84" height="84" {white}/></mask></defs>'
             f'<line x1="4" y1="32" x2="60" y2="32" stroke="{BLUE}" stroke-width="6" '
             'mask="url(#m)"/>'),
        # Real-world shapes.
        case("chart/fade-area", f'<defs><linearGradient id="g" x2="0" y2="1">'
             '<stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/>'
             '</linearGradient><mask id="m"><rect x="0" y="0" width="100" height="80" '
             f'fill="url(#g)"/></mask></defs><path d="{area_d}" fill="{BLUE}" mask="url(#m)"/>',
             size=chart_size),
        case("icon/badge", f'<defs><mask id="m"><rect width="64" height="64" {white}/>'
             '<circle cx="50" cy="14" r="11" fill="#000"/></mask></defs>'
             f'<circle cx="32" cy="32" r="28" fill="{BLUE}" mask="url(#m)"/>'
             '<circle cx="50" cy="14" r="7" fill="#22c55e"/>'),
    ])]


def use_cases() -> list[dict]:
    """`use` and `symbol` (set v6), and `use` inside a `clipPath`. Most cases
    reuse a few small shapes so that placement, inheritance and the instance
    viewport each show on their own."""
    red = "#ef4444"
    green = "#22c55e"
    rect = '<rect id="r" x="4" y="6" width="30" height="20"/>'
    plain_rect = '<rect x="4" y="6" width="30" height="20"/>'
    star = '<polygon id="s" points="32,4 40,56 6,22 58,22 24,56"/>'
    # A 24-unit icon drawn with strokes, as icon sprites are.
    check = ('<symbol id="i" viewBox="0 0 24 24" fill="none" stroke="currentColor" '
             'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
             '<path d="M20 6 9 17l-5-5"/><circle cx="12" cy="12" r="10"/></symbol>')
    plus = ('<symbol id="p" viewBox="0 0 24 24" fill="none" stroke="currentColor" '
            'stroke-width="2" stroke-linecap="round"><path d="M5 12h14M12 5v14"/></symbol>')
    tile = (f'<symbol id="t" viewBox="0 0 20 10"><rect width="20" height="10" fill="{green}"/>'
            f'<circle cx="10" cy="5" r="7" fill="{red}"/></symbol>')
    two = (f'<g id="g" transform="translate(2 3)"><rect width="24" height="24" fill="{red}"/>'
           f'<circle cx="24" cy="24" r="12" fill="{green}"/></g>')

    def used(case_id: str, defs: str, uses: str, **kw) -> dict:
        return case(case_id, f'<defs>{defs}</defs>{uses}', **kw)

    def clipped(case_id: str, clip: str, *, defs: str = rect, attrs: str = "") -> dict:
        return case(case_id, f'<defs>{defs}<clipPath id="c"{" " + attrs if attrs else ""}>'
                    f'{clip}</clipPath></defs><g clip-path="url(#c)">'
                    f'<rect width="64" height="64" fill="{BLUE}"/>'
                    f'<path d="M0,64L64,0" stroke="{red}" stroke-width="6"/></g>')

    return [scoped(c, i) for i, c in enumerate([
        # Placement and inheritance.
        used("use/rect", rect, f'<use href="#r" fill="{BLUE}"/>'),
        used("use/xy", rect, f'<use href="#r" x="10.5" y="20.25" fill="{BLUE}"/>'),
        used("use/xy-percent", rect, f'<use href="#r" x="10%" y="25%" fill="{BLUE}"/>'),
        used("use/transform-xy", rect, f'<use href="#r" x="12" y="4" fill="{BLUE}" '
             'transform="rotate(20 32 32) scale(1.1)"/>'),
        used("use/own-fill", f'<rect id="r" x="4" y="6" width="30" height="20" fill="{red}"/>',
             f'<use href="#r" x="20" y="30" fill="{BLUE}"/>'),
        used("use/stroke", rect, f'<use href="#r" x="12" y="18" fill="none" stroke="{BLUE}" '
             'stroke-width="5" stroke-dasharray="6 3"/>'),
        used("use/xlink", rect, f'<use xlink:href="#r" x="8" y="8" fill="{BLUE}"/>'),
        used("use/twice", rect, f'<use href="#r" fill="{BLUE}"/>'
             f'<use href="#r" x="24" y="32" fill="{red}" opacity="0.7"/>'),
        used("use/group", two, '<use href="#g" x="6" y="4"/>'),
        used("use/nested", f'{rect}<g id="n"><use href="#r" x="4" fill="{red}"/>'
             '<circle cx="40" cy="40" r="10"/></g>', f'<use href="#n" y="6" fill="{BLUE}"/>'),
        used("use/current-color", '<circle id="o" cx="32" cy="32" r="20" fill="currentColor"/>',
             f'<use href="#o" color="{green}"/>'),
        used("use/opacity", two, '<use href="#g" x="6" y="6" opacity="0.5"/>'),
        case("use/before-target", f'<use href="#r" x="20" y="30" fill="{BLUE}"/>'
             f'<rect id="r" x="4" y="6" width="30" height="20" fill="{red}"/>'),
        case("use/in-place", f'<g fill="{red}"><rect id="r" x="4" y="6" width="30" height="20"/>'
             f'</g><use href="#r" x="24" y="30" fill="{BLUE}"/>'),
        used("use/style-fill", rect, f'<use href="#r" x="8" y="8" style="fill: {green}"/>'),
        used("use/target-transform", f'<rect id="r" x="4" y="6" width="30" height="20" '
             'transform="rotate(15)"/>', f'<use href="#r" x="16" y="16" fill="{BLUE}"/>'),
        # Effects on a use.
        used("effects/clip-obb", f'{star}<clipPath id="c" clipPathUnits="objectBoundingBox">'
             '<rect width="0.6" height="0.7"/></clipPath>',
             f'<use href="#s" x="4" y="2" fill="{BLUE}" clip-path="url(#c)"/>'),
        used("effects/clip-user", f'{star}<clipPath id="c"><rect width="36" height="40"/>'
             '</clipPath>', f'<use href="#s" x="10" y="8" fill="{BLUE}" clip-path="url(#c)"/>'),
        used("effects/mask", f'{star}<mask id="m"><rect width="40" height="64" fill="#fff"/>'
             '</mask>', f'<use href="#s" x="8" fill="{BLUE}" mask="url(#m)"/>'),
        used("effects/hidden-use", rect, f'<use href="#r" x="8" y="8" fill="{BLUE}" '
             'visibility="hidden"/>'),
        used("effects/visible-target", f'<rect id="r" x="4" y="6" width="30" height="20" '
             'visibility="visible"/>', f'<use href="#r" x="8" y="8" fill="{BLUE}" '
             'visibility="hidden"/>'),
        used("effects/display-none-target", f'<rect id="r" x="4" y="6" width="30" height="20" '
             f'display="none"/><circle id="o" cx="40" cy="40" r="12"/>',
             f'<use href="#r" fill="{BLUE}"/><use href="#o" fill="{green}"/>'),
        # symbol.
        used("symbol/basic", check, f'<use href="#i" x="8" y="8" width="48" height="48" '
             f'color="{BLUE}"/>'),
        used("symbol/no-size", check, f'<use href="#i" color="{BLUE}"/>'),
        used("symbol/percent-size", check, f'<use href="#i" width="50%" height="75%" '
             f'color="{BLUE}"/>'),
        used("symbol/fractional", check, f'<use href="#i" x="3.5" y="5.25" width="37.5" '
             f'height="37.5" color="{BLUE}"/>'),
        used("symbol/no-viewbox", f'<symbol id="t">{plain_rect}'
             f'<circle cx="30" cy="30" r="20" fill="{red}"/></symbol>',
             f'<use href="#t" x="8" y="8" width="30" height="24" fill="{BLUE}"/>'),
        used("symbol/overflow-visible", tile.replace('<symbol id="t"',
             '<symbol id="t" overflow="visible"'),
             '<use href="#t" x="12" y="20" width="40" height="20"/>'),
        used("symbol/clipped", tile, '<use href="#t" x="12" y="20" width="40" height="20"/>'),
        used("symbol/meet", tile, '<use href="#t" x="4" y="4" width="56" height="56"/>'),
        used("symbol/slice", tile.replace('viewBox="0 0 20 10"',
             'viewBox="0 0 20 10" preserveAspectRatio="xMinYMax slice"'),
             '<use href="#t" x="4" y="4" width="40" height="56"/>'),
        used("symbol/none", tile.replace('viewBox="0 0 20 10"',
             'viewBox="0 0 20 10" preserveAspectRatio="none"'),
             '<use href="#t" x="4" y="4" width="40" height="56"/>'),
        used("symbol/viewbox-offset", f'<symbol id="t" viewBox="10 5 20 20">'
             f'<rect width="40" height="40" fill="{green}"/><circle cx="20" cy="15" r="9" '
             f'fill="{red}"/></symbol>', '<use href="#t" x="8" y="8" width="48" height="48"/>'),
        used("symbol/own-size", f'<symbol id="t" width="30" height="20" viewBox="0 0 20 10">'
             f'<rect width="20" height="10" fill="{green}"/><circle cx="10" cy="5" r="7" '
             f'fill="{red}"/></symbol>', '<use href="#t" x="8" y="8"/>'),
        used("symbol/viewbox-zero", f'<symbol id="t" viewBox="0 0 0 10">'
             f'<rect width="20" height="10" fill="{green}"/></symbol>',
             '<use href="#t" x="4" y="4" width="40" height="40"/>'),
        case("symbol/direct", tile + '<rect x="8" y="8" width="20" height="20" '
             f'fill="{BLUE}"/>'),
        used("symbol/transform", check, f'<use href="#i" width="24" height="24" color="{BLUE}" '
             'transform="translate(32 4) rotate(30) scale(1.5)"/>'),
        used("symbol/stroke-scaled", plus, f'<use href="#p" x="2" y="2" width="60" height="60" '
             f'color="{red}" stroke-width="1"/>'),
        used("symbol/clip-obb", check + '<clipPath id="c" clipPathUnits="objectBoundingBox">'
             '<rect width="0.5" height="1"/></clipPath>',
             f'<use href="#i" x="8" y="8" width="48" height="48" color="{BLUE}" '
             'clip-path="url(#c)"/>'),
        # A referenced svg.
        used("svg/own-size", f'<svg id="v" x="4" y="6" width="30" height="40">'
             f'<rect width="50" height="50" fill="{green}"/><circle cx="15" cy="20" r="12" '
             f'fill="{red}"/></svg>', '<use href="#v" x="10" y="4"/>'),
        used("svg/override", f'<svg id="v" x="4" y="6" width="30" height="40" '
             f'viewBox="0 0 30 40"><rect width="50" height="50" fill="{green}"/>'
             f'<circle cx="15" cy="20" r="12" fill="{red}"/></svg>',
             '<use href="#v" x="2" width="56" height="24"/>'),
        # use inside a clipPath.
        clipped("clip/use", '<use href="#r"/>'),
        clipped("clip/use-xy", '<use href="#r" x="12.5" y="20.25"/>'),
        clipped("clip/use-transform", '<use href="#r" x="10" y="10" transform="rotate(20 32 32)"/>',
                defs='<rect id="r" x="4" y="6" width="30" height="20" transform="scale(1.4)"/>'),
        clipped("clip/use-rule", '<use href="#s" clip-rule="evenodd"/>', defs=star),
        clipped("clip/use-target-rule", '<use href="#s"/>',
                defs=star.replace('/>', ' clip-rule="evenodd"/>')),
        clipped("clip/use-and-shape", '<use href="#r" x="8" y="8"/><circle cx="44" cy="44" r="14"/>'),
        clipped("clip/use-obb", '<use href="#r" transform="scale(0.02)"/>',
                attrs='clipPathUnits="objectBoundingBox"'),
        clipped("clip/use-group", '<use href="#g"/>', defs=two),
        clipped("clip/use-symbol", '<use href="#t" width="40" height="40"/>', defs=tile),
        clipped("clip/use-circle-crisp", '<use href="#o" shape-rendering="crispEdges"/>',
                defs='<circle id="o" cx="32" cy="32" r="20.5"/>'),
        # Edges.
        case("edge/missing", f'<use href="#nothing" fill="{BLUE}"/><rect x="40" y="40" '
             'width="16" height="16"/>'),
        case("edge/no-href", f'<use x="8" y="8" fill="{BLUE}"/><rect x="40" y="40" '
             'width="16" height="16"/>'),
        case("edge/ancestor", f'<g id="a"><rect x="4" y="4" width="20" height="20" '
             f'fill="{BLUE}"/><use href="#a" x="24" y="24"/></g>'),
        used("edge/mutual", f'<g id="a"><rect width="20" height="20" fill="{red}"/>'
             '<use href="#b" x="20"/></g><g id="b"><use href="#a" y="20"/></g>',
             f'<use href="#a" x="8" y="8"/><rect x="40" y="40" width="16" height="16" '
             f'fill="{BLUE}"/>'),
        used("edge/cycle-deep", f'{rect}<g id="a"><use href="#r" fill="{red}"/>'
             '<use href="#a" x="20"/></g>',
             f'<use href="#a" x="8" y="20"/><use href="#r" x="24" y="0" fill="{BLUE}"/>'),
        used("edge/to-gradient", '<linearGradient id="l"><stop offset="0" stop-color="red"/>'
             '</linearGradient>', f'<use href="#l" fill="{BLUE}"/><rect x="40" y="40" '
             'width="16" height="16"/>'),
        used("edge/href-wins", f'{rect}<circle id="o" cx="32" cy="32" r="20"/>',
             f'<use href="#r" xlink:href="#o" x="8" y="8" fill="{BLUE}"/>'),
        used("edge/size-on-shape", rect, f'<use href="#r" x="8" y="8" width="10" height="10" '
             f'fill="{BLUE}"/>'),
        # Real-world shapes.
        used("icon/sprite", check + plus, f'<use href="#i" x="4" y="4" width="24" height="24" '
             f'color="{BLUE}"/><use href="#p" x="36" y="4" width="24" height="24" '
             f'color="{red}"/><use href="#i" x="4" y="36" width="16" height="16" '
             f'color="{green}"/><use href="#p" x="36" y="36" width="20" height="20" '
             f'color="{INK}" stroke-width="3"/>'),
        case("chart/legend", f'<defs><symbol id="d" viewBox="0 0 32 32"><path d="M0,16h10.67'
             f'A5.33,5.33,0,1,1,21.33,16H32M21.33,16A5.33,5.33,0,1,1,10.67,16" '
             f'stroke-width="4" fill="#fff" stroke="inherit"/></symbol></defs>'
             f'<use href="#d" x="4" y="8" width="14" height="14" stroke="{BLUE}"/>'
             f'<rect x="22" y="12" width="36" height="6" fill="{GRID}"/>'
             f'<use href="#d" x="4" y="36" width="14" height="14" stroke="{red}"/>'
             f'<rect x="22" y="40" width="28" height="6" fill="{GRID}"/>',
             size=(64, 64)),
    ])]


def marker_cases() -> list[dict]:
    """Markers (set v7): where they go and which way they point on each kind
    of segment, how the marker viewport maps and clips, which properties
    they take, and the arrowheads and data dots charts and diagrams draw."""
    red = "#ef4444"
    green = "#22c55e"
    # An arrow whose tip is its reference point, so position and direction
    # both show; a dot; and a notched square that is not symmetric about
    # either axis, so a mirror or a quarter turn shows too.
    arrow = (f'<marker id="a" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="4" '
             f'markerHeight="4" orient="auto"><path d="M0,0L10,5L0,10z" fill="{BLUE}"/></marker>')
    dot = (f'<marker id="d" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="3" '
           f'markerHeight="3"><circle cx="5" cy="5" r="4" fill="{red}"/></marker>')
    flag = (f'<marker id="f" viewBox="0 0 10 10" refX="0" refY="5" markerWidth="5" '
            f'markerHeight="5" orient="auto"><path d="M0,0H10V4H5V10H0z" fill="{green}"/>'
            '</marker>')
    all_three = 'marker-start="url(#f)" marker-mid="url(#d)" marker-end="url(#a)"'
    every = 'marker-start="url(#a)" marker-mid="url(#a)" marker-end="url(#a)"'
    line = f'fill="none" stroke="{INK}" stroke-width="2"'

    def marked(case_id: str, defs: str, body: str, **kw) -> dict:
        return case(case_id, f'<defs>{defs}</defs>{body}', **kw)

    def three(case_id: str, shape: str, **kw) -> dict:
        return marked(case_id, arrow + dot + flag, shape, **kw)

    def arrows(case_id: str, shape: str, **kw) -> dict:
        return marked(case_id, arrow, shape, **kw)

    def custom(case_id: str, attrs: str, content: str, shape: str = "", **kw) -> dict:
        shape = shape or f'<path d="M12,32H52" {line} marker-end="url(#m)"/>'
        return marked(case_id, f'<marker id="m" {attrs}>{content}</marker>', shape, **kw)

    square = (f'<rect width="10" height="10" fill="{BLUE}"/>'
              f'<rect width="5" height="5" fill="{red}"/>')
    big = (f'<rect x="-4" y="-4" width="18" height="18" fill="{green}"/>'
           f'<rect width="10" height="10" fill="{BLUE}"/>')
    blocker = f'<rect x="40" y="44" width="12" height="12" fill="{BLUE}"/>'
    plain = f'<path d="M12,32H52" {line} marker-end="url(#m)"/>'
    diagonal = f'<path d="M12,48L52,16" {line} marker-end="url(#m)"/>'
    zig = '<polyline points="8,52 22,14 38,44 56,10"'

    return [scoped(c, i) for i, c in enumerate([
        # Positions and directions on each kind of segment.
        three("pos/line", f'<line x1="10" y1="50" x2="50" y2="14" {line} {all_three}/>'),
        three("pos/polyline", f'{zig} {line} {all_three}/>'),
        three("pos/polygon", f'<polygon points="12,12 52,16 40,52 14,44" {line} {all_three}/>'),
        three("pos/path-lines", f'<path d="M8,12L56,12L8,52L56,52" {line} {all_three}/>'),
        three("pos/relative", f'<path d="m8,52 h20 v-30 l16,-10 H56 V52" {line} {all_three}/>'),
        three("pos/subpaths", f'<path d="M8,14L40,14M14,40L56,30L56,54" {line} {all_three}/>'),
        three("pos/closed", f'<path d="M14,14L50,14L50,50Z" {line} {all_three}/>'),
        three("pos/closed-explicit", f'<path d="M14,14L50,14L50,50L14,14Z" {line} {all_three}/>'),
        three("pos/closed-then-line", f'<path d="M10,10L40,10L40,40ZL54,54" {line} {all_three}/>'),
        three("pos/closed-subpaths", f'<path d="M8,8H28V28ZM36,36H56V56Z" {line} {all_three}/>'),
        three("pos/cubic", f'<path d="M8,52C8,8 56,56 56,12" {line} {all_three}/>'),
        three("pos/cubic-coincident", f'<path d="M8,52C8,52 40,8 56,12C56,12 56,40 30,56" '
              f'{line} {all_three}/>'),
        three("pos/cubic-degenerate", f'<path d="M10,50C10,50 10,50 50,14" {line} {all_three}/>'),
        three("pos/quad", f'<path d="M8,52Q32,-8 56,52" {line} {all_three}/>'),
        three("pos/smooth", f'<path d="M6,32C14,6 22,6 30,32S46,58 58,32" {line} {all_three}/>'),
        three("pos/smooth-quad", f'<path d="M6,40Q16,10 26,40T46,40T58,20" {line} {all_three}/>'),
        three("pos/arc", f'<path d="M10,40A22,22 0 0 1 54,40" {line} {all_three}/>'),
        three("pos/arc-large", f'<path d="M20,20A16,12 30 1 0 44,44" {line} {all_three}/>'),
        three("pos/arc-chain", f'<path d="M8,32A12,12 0 0 1 32,32A12,12 0 0 0 56,32" '
              f'{line} {all_three}/>'),
        three("pos/arc-degenerate", f'<path d="M10,50A0,10 0 0 1 50,14" {line} {all_three}/>'),
        three("pos/zero-length", f'<path d="M12,32L12,32L52,32L52,32" {line} {all_three}/>'),
        three("pos/zero-length-only", f'<path d="M32,32L32,32" {line} {all_three} '
              'stroke-linecap="round"/>'),
        three("pos/moveto-only", f'<path d="M32,32" {line} {all_three}/>{blocker}'),
        three("pos/reversal", f'<path d="M10,32L54,32L20,32" {line} {all_three}/>'),
        three("pos/sharp", f'<path d="M8,10L56,30L8,34L56,54" {line} {all_three}/>'),
        three("pos/error", f'<path d="M8,52L30,12L56,52L" {line} {all_three}/>'),
        # Directions at mid vertices, after zero-length segments and at moves,
        # with an arrow at every vertex.
        arrows("dir/polyline", f'{zig} {line} {every}/>'),
        arrows("dir/subpaths", f'<path d="M8,14L40,14M14,40L56,30L56,54" {line} {every}/>'),
        arrows("dir/closed", f'<polygon points="12,12 52,16 40,52 14,44" {line} {every}/>'),
        arrows("dir/curves", f'<path d="M6,50C6,20 20,10 24,30Q30,56 40,30A10,14 20 0 1 58,20" '
               f'{line} {every}/>'),
        arrows("dir/reversal", f'<path d="M10,20L54,20L20,20M10,44L54,44L54,44L20,44" '
               f'{line} {every}/>'),
        arrows("dir/zero-diagonal", f'<path d="M12,52L12,52L52,12L52,12" {line} {every}/>'),
        arrows("dir/zero-mid", f'<path d="M8,52L32,20L32,20L56,52" {line} {every}/>'),
        arrows("dir/zero-close", f'<path d="M14,14L50,14L50,50L14,14Z" {line} {every}/>'),
        arrows("dir/zero-curve", f'<path d="M10,50C10,50 10,50 10,50L50,14Q50,14 50,14" '
               f'{line} {every}/>'),
        arrows("dir/moves", f'<path d="M20,20M40,44L56,20" {line} {every}/>'),
        arrows("dir/move-pair", f'<path d="M20,12M44,52" {line} {every}/>'),
        arrows("dir/trailing-move", f'<path d="M10,50L30,20M50,40" {line} {every}/>'),
        arrows("dir/lone-move", f'<path d="M40,12" {line} {every}/>'),
        arrows("dir/close-move", f'<path d="M14,14L50,14L50,50ZM30,56L56,40" {line} {every}/>'),
        # Orientation.
        custom("orient/angle", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" orient="30"', square),
        custom("orient/units", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" orient="0.3rad"', square,
               f'<path d="M12,20H52" {line} marker-end="url(#m)"/>'
               f'<path d="M12,44H52" {line} marker-end="url(#n)"/>'
               f'<marker id="n" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               f'markerHeight="6" orient="-0.125turn">{square}</marker>'),
        custom("orient/auto-start-reverse", 'viewBox="0 0 10 10" refX="10" refY="5" '
               'markerWidth="5" markerHeight="5" orient="auto-start-reverse"',
               f'<path d="M0,0L10,5L0,10z" fill="{BLUE}"/>',
               f'<path d="M12,48L32,16L52,48" {line} marker-start="url(#m)" '
               'marker-end="url(#m)"/>'),
        custom("orient/invalid", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" orient="sideways"', square, diagonal),
        # Size, units and the viewport.
        custom("units/stroke-width", 'markerWidth="4" markerHeight="4" refX="2" refY="2"',
               square, f'<path d="M12,20H52" {line} marker-end="url(#m)"/>'
               f'<path d="M12,46H40" fill="none" stroke="{INK}" stroke-width="3.5" '
               'marker-end="url(#m)"/>'),
        custom("units/user-space", 'markerUnits="userSpaceOnUse" markerWidth="12" '
               'markerHeight="12" refX="5" refY="5"', square,
               f'<path d="M12,32H52" fill="none" stroke="{INK}" stroke-width="5" '
               'marker-end="url(#m)"/>'),
        custom("units/stroke-none", 'markerWidth="4" markerHeight="4"', square,
               '<path d="M12,32H44" fill="none" stroke="none" stroke-width="3" '
               'marker-end="url(#m)"/>'),
        custom("units/stroke-percent", 'markerWidth="4" markerHeight="4"', square,
               f'<path d="M12,32H44" fill="none" stroke="{INK}" stroke-width="5%" '
               'marker-end="url(#m)"/>'),
        custom("size/default", 'refX="0" refY="0"', square,
               f'<path d="M12,32H44" fill="none" stroke="{INK}" stroke-width="4" '
               'marker-end="url(#m)"/>'),
        custom("size/viewbox-meet", 'viewBox="0 0 10 10" markerWidth="8" markerHeight="4" '
               'overflow="visible"', big),
        custom("size/viewbox-slice", 'viewBox="0 0 10 10" markerWidth="8" markerHeight="4" '
               'preserveAspectRatio="xMaxYMax slice"', big),
        custom("size/viewbox-none", 'viewBox="0 0 10 10" markerWidth="8" markerHeight="4" '
               'preserveAspectRatio="none"', big),
        custom("size/viewbox-offset", 'viewBox="-5 -5 10 10" markerWidth="6" markerHeight="6"',
               big),
        custom("size/zero", 'viewBox="0 0 10 10" markerWidth="0" markerHeight="6"', square,
               plain + blocker),
        custom("size/negative", 'viewBox="0 0 10 10" markerWidth="-4" markerHeight="6"', square,
               plain + blocker),
        custom("size/empty-viewbox", 'viewBox="0 0 0 10" markerWidth="6" markerHeight="6"',
               square, plain + blocker),
        custom("ref/xy", 'viewBox="0 0 10 10" refX="2.5" refY="7.5" markerWidth="6" '
               'markerHeight="6" orient="auto"', square, diagonal),
        custom("ref/keywords", 'viewBox="0 0 10 10" refX="center" refY="bottom" '
               'markerWidth="6" markerHeight="6"', square),
        custom("ref/percent", 'viewBox="0 0 10 10" refX="50%" refY="25%" '
               'markerWidth="6" markerHeight="6"', square),
        custom("clip/hidden", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" orient="auto"', big, diagonal),
        custom("clip/visible", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" orient="auto" overflow="visible"', big, diagonal),
        custom("clip/auto", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" style="overflow:auto"', big),
        custom("clip/meet", 'viewBox="0 0 10 10" markerWidth="8" markerHeight="4" '
               'preserveAspectRatio="xMidYMid meet"', big),
        # Properties.
        custom("prop/inherit", f'markerWidth="6" markerHeight="6" fill="{green}"',
               '<rect width="3" height="3"/><rect x="3" y="3" width="3" height="3" '
               'fill="inherit"/>',
               f'<g fill="{red}" stroke="{red}"><path d="M12,32H44" stroke-width="2" '
               'marker-end="url(#m)"/></g>'),
        three("prop/css-shorthand", f'{zig} {line} style="marker:url(#d)"/>'),
        three("prop/attr-shorthand", f'{zig} {line} marker="url(#d)"/>'),
        three("prop/inherited", f'<g {all_three}>{zig} {line}/>'
              f'<path d="M8,58H30L56,58" {line} marker-mid="none"/></g>'),
        three("prop/other-shapes", f'<rect x="8" y="8" width="20" height="16" {line} '
              f'{all_three}/><circle cx="44" cy="20" r="10" {line} {all_three}/>'
              f'<ellipse cx="20" cy="46" rx="12" ry="8" {line} {all_three}/>'),
        three("prop/missing", f'<path d="M8,32H56" {line} marker-start="url(#nope)" '
              'marker-end="url(#a)"/>'),
        marked("prop/not-a-marker", f'<rect id="r" width="10" height="10" fill="{red}"/>{arrow}',
               f'<path d="M8,32H56" {line} marker-start="url(#r)" marker-end="url(#a)"/>'),
        custom("prop/context-paint", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="5" '
               'markerHeight="5"', '<rect width="10" height="10" fill="context-stroke" '
               'stroke="context-fill" stroke-width="3"/>',
               f'<path d="M12,20H52" fill="{red}" stroke="{BLUE}" stroke-width="2" '
               f'marker-end="url(#m)"/><path d="M12,46H52" fill="{green}" stroke="{INK}" '
               'stroke-width="2" marker-end="url(#m)"/>'),
        three("prop/opacity", f'{zig} {line} {all_three} opacity="0.5"/>'),
        custom("prop/content-opacity", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="8" '
               'markerHeight="8"', f'<g opacity="0.5"><rect width="7" height="7" fill="{BLUE}"/>'
               f'<rect x="3" y="3" width="7" height="7" fill="{red}"/></g>'),
        three("prop/paint-order", f'{zig} fill="none" stroke="{INK}" stroke-width="5" '
              f'{all_three} paint-order="markers"/>'),
        three("prop/shape-transform", f'{zig} {line} {all_three} '
              'transform="rotate(15 32 32) scale(0.9 1.1)"/>'),
        custom("prop/marker-transform", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" transform="rotate(45)"', square),
        custom("prop/display-none", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" '
               'markerHeight="6" style="display:none"', square),
        three("prop/visibility", f'<path d="M8,20H56" {line} {all_three} visibility="hidden"/>'
              f'<g visibility="hidden"><path d="M8,44H56" {line} {all_three} '
              'visibility="visible"/></g>'),
        three("prop/clip-path", '<clipPath id="c"><rect width="40" height="64"/></clipPath>'
              f'{zig} {line} {all_three} clip-path="url(#c)"/>'),
        custom("prop/content-clip", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="8" '
               'markerHeight="8"', '<clipPath id="c"><circle cx="5" cy="5" r="4"/></clipPath>'
               f'<rect width="10" height="10" fill="{BLUE}" clip-path="url(#c)"/>'),
        custom("prop/content-use", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="8" '
               'markerHeight="8"', '<use href="#q" x="2" y="2"/>',
               f'<defs><rect id="q" width="6" height="6" fill="{green}"/></defs>{plain}'),
        marked("prop/in-use", arrow + dot + flag + '<symbol id="s" viewBox="0 0 64 64">'
               f'{zig} {line} {all_three}/></symbol>',
               '<use href="#s" x="4" y="4" width="56" height="56"/>'),
        custom("prop/nested", 'viewBox="0 0 10 10" refX="5" refY="5" markerWidth="8" '
               'markerHeight="8"', f'<path d="M1,5H9" stroke="{BLUE}" stroke-width="1" '
               'marker-end="url(#m)" marker-start="url(#n)"/>',
               '<marker id="n" markerWidth="4" markerHeight="4" refX="2" refY="2">'
               f'<circle cx="2" cy="2" r="2" fill="{red}"/></marker>{plain}'),
        # Real-world markers.
        marked("chart/axis-arrows", '<marker id="x" viewBox="0 -5 10 10" refX="8" '
               'markerWidth="6" markerHeight="6" orient="auto"><path d="M0,-5L10,0L0,5" '
               f'fill="{INK}"/></marker>',
               f'<path d="M8,56H58" fill="none" stroke="{INK}" stroke-width="1.5" '
               f'marker-end="url(#x)"/><path d="M8,56V4" fill="none" stroke="{INK}" '
               'stroke-width="1.5" marker-end="url(#x)"/>'),
        marked("chart/data-dots", '<marker id="p" viewBox="0 0 10 10" refX="5" refY="5" '
               'markerWidth="5" markerHeight="5"><circle cx="5" cy="5" r="4" fill="#fff" '
               f'stroke="{BLUE}" stroke-width="2"/></marker>',
               '<polyline points="6,50 16,38 26,42 36,20 46,26 58,10" fill="none" '
               f'stroke="{BLUE}" stroke-width="1.5" marker-start="url(#p)" '
               'marker-mid="url(#p)" marker-end="url(#p)"/>'),
        marked("diagram/edges", '<marker id="e" viewBox="0 0 12 12" refX="11" refY="6" '
               'markerUnits="userSpaceOnUse" markerWidth="9" markerHeight="9" orient="auto">'
               f'<path d="M0,0L12,6L0,12L3,6z" fill="{INK}"/></marker>'
               '<marker id="o" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" '
               'markerWidth="7" markerHeight="7"><circle cx="5" cy="5" r="4" fill="#fff" '
               f'stroke="{INK}"/></marker>',
               f'<rect x="4" y="4" width="20" height="12" rx="2" fill="{GRID}"/>'
               f'<rect x="40" y="46" width="20" height="12" rx="2" fill="{GRID}"/>'
               f'<path d="M14,16C14,36 50,26 50,46" fill="none" stroke="{INK}" '
               'stroke-width="1.25" marker-start="url(#o)" marker-end="url(#e)"/>'
               f'<path d="M24,10H56V40" fill="none" stroke="{INK}" stroke-width="1.25" '
               'stroke-dasharray="3 2" marker-end="url(#e)"/>'),
    ])]


def scoped(c: dict, index: int) -> dict:
    """Ids are document-wide and every case of a sheet shares one page, so
    each case's ids (and its references to them) get the case's own prefix."""
    prefix = f"k{index}-"
    svg = (re.sub(r'(?<=\s)id="', f'id="{prefix}', c["svg"]).replace("url(#", f"url(#{prefix}")
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
    v2: everything at 1x, the `SCALE_2` subset at 2x. v3 to v7: everything at
    both."""
    if corpus in ("v3", "v4", "v5", "v6", "v7"):
        everything = {"v3": clip_cases, "v4": server_cases, "v5": mask_cases,
                      "v6": use_cases, "v7": marker_cases}[corpus]()
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
    lst.add_argument("--set", dest="corpus", choices=("v1", "v2", "v3", "v4", "v5", "v6", "v7"), default="v1")
    cap = sub.add_parser("capture")
    cap.add_argument("--set", dest="corpus", choices=("v1", "v2", "v3", "v4", "v5", "v6", "v7"), default="v1")
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
