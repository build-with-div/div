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

    paint_corpus.py list
    paint_corpus.py capture --chrome PATH --output-prefix \\
        tests/fidelity/fixtures/svg-paint-v1/chromium-linux
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


def sheets() -> list[dict]:
    """The cases of each sheet: everything at 1x, the Lucide icons at 2x."""
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
    sub.add_parser("list")
    cap = sub.add_parser("capture")
    cap.add_argument("--chrome", type=Path, required=True)
    cap.add_argument("--output-prefix", type=Path, required=True)
    args = parser.parse_args()

    all_sheets = sheets()
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
