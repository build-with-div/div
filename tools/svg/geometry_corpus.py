#!/usr/bin/env python3
"""Capture SVG geometry from pinned Chromium as a black-box oracle.

RFC 0009 writes the parts of SVG whose Blink source is LGPL (path data and
arcs, shape geometry, lengths, `viewBox` and `preserveAspectRatio`, hit
testing) from the W3C specifications. This corpus keeps that work honest:
Chromium 147 is queried only through web APIs, and `div-svg` must reproduce
every number.

For each case the page holds one `<svg>`; the probed element is marked
`data-probe`. The capture records, per probed element:

- `getTotalLength()` and `getPointAtLength()` at 33 evenly spaced lengths
  (geometry elements only);
- `getBBox()`;
- `getCTM()` and `getScreenCTM()`;
- `isPointInFill()` and `isPointInStroke()` on a 17×17 grid over the
  box grown by the stroke width (geometry elements only);
- the outer `<svg>`'s `getBoundingClientRect()`.

    geometry_corpus.py list
    geometry_corpus.py capture --chrome PATH --output out/svg/geometry-chromium.json

Run the evidence capture on Linux (WSL) with the census Chrome for Testing
147.0.7727.50. A Windows capture is a smoke test, not evidence.
"""

from __future__ import annotations

import argparse
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

SCHEMA = "div-svg-geometry-corpus-v1"
EXPECTED_CHROMIUM = "147.0.7727.50"
LENGTH_SAMPLES = 32
GRID = 17
SLOT = 256
STYLE = "fill:#000;stroke:#000;stroke-width:4"


def case(case_id: str, body: str, *, viewport: str = 'width="200" height="200"',
         style: str = STYLE) -> dict[str, str]:
    """One `<svg>` whose first `data-probe` element is measured."""
    head = body.split(">", 1)[0]
    tag_end = head.index(" ") if " " in head else len(head.rstrip("/"))
    probe = body[:tag_end] + f' data-probe style="{style}"' + body[tag_end:]
    return {"id": case_id, "svg": f"<svg {viewport}>{probe}</svg>"}


def path_cases() -> list[dict[str, str]]:
    paths = {
        # Every command, absolute and relative.
        "move-line": "M10 10 L190 10 L190 190",
        "move-line-rel": "m10 10 l180 0 l0 180",
        "implicit-lineto": "M10 10 50 50 90 10",
        "implicit-lineto-rel": "m10 10 40 40 40 -40",
        "hv": "M10 10 H190 V190 H10 Z",
        "hv-rel": "m10 10 h180 v180 h-180 z",
        "cubic": "M10 100 C40 10 160 10 190 100",
        "cubic-rel": "m10 100 c30 -90 150 -90 180 0",
        "smooth-cubic": "M10 100 C40 10 70 10 100 100 S160 190 190 100",
        "smooth-cubic-rel": "m10 100 c30 -90 60 -90 90 0 s60 90 90 0",
        "smooth-cubic-no-previous": "M10 100 S100 10 190 100",
        "quad": "M10 100 Q100 10 190 100",
        "quad-rel": "m10 100 q90 -90 180 0",
        "smooth-quad": "M10 100 Q55 10 100 100 T190 100",
        "smooth-quad-rel": "m10 100 q45 -90 90 0 t90 0",
        "smooth-quad-no-previous": "M10 100 T190 100",
        "close-then-line": "M10 10 H100 V100 Z L190 190",
        "close-then-move": "M10 10 H100 V100 Z M120 120 H190 V190 Z",
        "two-closes": "M10 10 H100 V100 Z Z",
        # Number syntax.
        "no-separators": "M10-10L190.5.5L-1e1 1E2",
        "exponents": "M1e1 1e1 L1.9e2 1e+1 L1.9E2 19e1",
        "commas": "M 10 , 10 L 190 , 10 , 190 , 190",
        "leading-dot": "M.5 .5 L190.25.75",
        # Fractions that each single-precision accumulation order, and
        # correct rounding, read differently.
        "fractions": "M1.354 1.771 L5.33 5.33 L1.834 10.67",
        # Error handling: render up to the error.
        "error-bad-command": "M10 10 L190 10 X L190 190",
        "error-truncated": "M10 10 L190 10 L190",
        "error-no-move": "L190 10 L190 190",
        "error-trailing-comma": "M10 10 L190 10,",
        "empty": "",
        "move-only": "M100 100",
        "zero-length": "M100 100 L100 100",
        "zero-length-close": "M100 100 Z",
    }
    out = [case(f"path/{k}", f'<path d="{d}"/>') for k, d in paths.items()]
    # Arcs: each flag pair, rotation, scaled-up radii and degenerate forms.
    for large in (0, 1):
        for sweep in (0, 1):
            out.append(case(f"arc/flags-{large}{sweep}",
                            f'<path d="M60 100 A50 30 0 {large} {sweep} 140 100"/>'))
    for angle in (0, 30, 45, 90, 135, -60, 400):
        out.append(case(f"arc/rotation-{angle}",
                        f'<path d="M60 100 A60 30 {angle} 0 1 140 100"/>'))
    arcs = {
        "radii-too-small": "M20 100 A10 10 0 0 1 180 100",
        "radii-zero-rx": "M20 100 A0 40 0 0 1 180 100",
        "radii-negative": "M60 100 A-50 -30 0 0 1 140 100",
        "same-endpoint": "M100 100 A50 50 0 1 1 100 100",
        "flags-packed": "M60 100a40 40 0 00 80 0",
        "flags-packed-coords": "M60 100a40 40 0 1180 0",
        "near-full": "M100 20 A80 80 0 1 1 99.9 20",
        "half": "M20 100 A80 80 0 0 1 180 100",
        "quarter": "M100 20 A80 80 0 0 1 180 100",
        "tiny-span": "M100 20 A80 80 0 0 1 104 20.1",
        "rel-chain": "m20 100 a40 40 0 0 1 80 0 a40 40 0 0 0 80 0",
        "pie-slice": "M100 100 L180 100 A80 80 0 0 1 156.57 156.57 Z",
        "donut": "M100 20 A80 80 0 1 1 99.99 20 Z M100 60 A40 40 0 1 0 100.01 60 Z",
    }
    out += [case(f"arc/{k}", f'<path d="{d}"/>') for k, d in arcs.items()]
    return out


def shape_cases() -> list[dict[str, str]]:
    rects = {
        "plain": 'x="20" y="30" width="160" height="120"',
        "rx": 'x="20" y="30" width="160" height="120" rx="20"',
        "ry": 'x="20" y="30" width="160" height="120" ry="20"',
        "rx-ry": 'x="20" y="30" width="160" height="120" rx="30" ry="10"',
        "rx-clamped": 'x="20" y="30" width="160" height="120" rx="500"',
        "rx-negative": 'x="20" y="30" width="160" height="120" rx="-10"',
        "rx-auto": 'x="20" y="30" width="160" height="120" rx="auto" ry="15"',
        "zero-width": 'x="20" y="30" width="0" height="120"',
        "negative-height": 'x="20" y="30" width="160" height="-5"',
        "percent": 'x="10%" y="15%" width="80%" height="60%" rx="5%"',
        "units": 'x="2mm" y="1pc" width="1in" height="72pt"',
    }
    out = [case(f"rect/{k}", f"<rect {a}/>") for k, a in rects.items()]
    others = {
        "circle/plain": '<circle cx="100" cy="100" r="60"/>',
        "circle/zero": '<circle cx="100" cy="100" r="0"/>',
        "circle/percent": '<circle cx="50%" cy="50%" r="25%"/>',
        "ellipse/plain": '<ellipse cx="100" cy="100" rx="80" ry="40"/>',
        "ellipse/rx-auto": '<ellipse cx="100" cy="100" rx="auto" ry="40"/>',
        "ellipse/zero-ry": '<ellipse cx="100" cy="100" rx="80" ry="0"/>',
        "line/plain": '<line x1="20" y1="20" x2="180" y2="160"/>',
        "line/zero": '<line x1="100" y1="100" x2="100" y2="100"/>',
        "polyline/plain": '<polyline points="20,20 180,20 100,180"/>',
        "polyline/odd": '<polyline points="20,20 180,20 100"/>',
        "polyline/separators": '<polyline points="20 20,180-20 100 180"/>',
        "polygon/plain": '<polygon points="20,20 180,20 100,180"/>',
        "polygon/star": '<polygon points="100,10 40,190 190,70 10,70 160,190"/>',
    }
    out += [case(k, v) for k, v in others.items()]
    return out


def stroke_cases() -> list[dict[str, str]]:
    """Stroke geometry for `isPointInStroke`: caps, joins and miters."""
    zigzag = '<polyline points="20,160 60,40 100,160 140,40 180,160"/>'
    out = []
    for cap in ("butt", "round", "square"):
        out.append(case(f"stroke/cap-{cap}", '<line x1="40" y1="100" x2="160" y2="100"/>',
                        style=f"fill:none;stroke:#000;stroke-width:24;stroke-linecap:{cap}"))
    for join in ("miter", "round", "bevel", "arcs", "miter-clip"):
        out.append(case(f"stroke/join-{join}", zigzag,
                        style=f"fill:none;stroke:#000;stroke-width:16;stroke-linejoin:{join}"))
    for limit in (1, 2, 4, 10):
        out.append(case(f"stroke/miterlimit-{limit}", zigzag,
                        style=f"fill:none;stroke:#000;stroke-width:16;stroke-miterlimit:{limit}"))
    out.append(case("stroke/dashed", '<line x1="20" y1="100" x2="180" y2="100"/>',
                    style="fill:none;stroke:#000;stroke-width:20;stroke-dasharray:30 10"))
    out.append(case("stroke/evenodd", '<polygon points="100,10 40,190 190,70 10,70 160,190"/>',
                    style="fill:#000;fill-rule:evenodd;stroke:none"))
    return out


def viewport_cases() -> list[dict[str, str]]:
    probe = '<rect x="0" y="0" width="100" height="50"/>'
    out = []
    for align in ("none", "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid",
                  "xMaxYMid", "xMinYMax", "xMidYMax", "xMaxYMax"):
        slices = ("meet",) if align == "none" else ("meet", "slice")
        for mode in slices:
            par = align if align == "none" else f"{align} {mode}"
            out.append(case(f"viewbox/{align}-{mode}", probe,
                            viewport=f'width="200" height="120" viewBox="0 0 100 50" '
                                     f'preserveAspectRatio="{par}"'))
    for name, attrs in {
        "offset": 'viewBox="-20 -10 100 50"',
        "zero-width": 'viewBox="0 0 0 50"',
        "negative-width": 'viewBox="0 0 -100 50"',
        "commas": 'viewBox="0,0,100,50"',
        "no-size": 'viewBox="0 0 100 50"',
    }.items():
        size = "" if name == "no-size" else 'width="200" height="120" '
        out.append(case(f"viewbox/{name}", probe, viewport=size + attrs))
    out.append(case("viewbox/nested", '<svg x="20" y="10" width="100" height="60" '
                    'viewBox="0 0 50 30"><rect data-inner x="0" y="0" width="50" height="30"/>'
                    "</svg>", viewport='width="200" height="120"'))
    for name, transform in {
        "matrix": "matrix(1 0.2 -0.2 1 30 10)",
        "translate-one": "translate(30)",
        "scale-one": "scale(1.5)",
        "rotate-center": "rotate(30 50 25)",
        "skew": "skewX(20) skewY(-10)",
        "list-commas": "translate(10,20),scale(2,0.5)",
        "invalid": "translate(10 20) bogus(3)",
    }.items():
        out.append(case(f"transform/{name}",
                        f'<rect transform="{transform}" x="0" y="0" width="100" height="50"/>'))
    return out


def corpus() -> list[dict[str, str]]:
    return path_cases() + shape_cases() + stroke_cases() + viewport_cases()


PAGE_SCRIPT = r"""
(() => {
  const SAMPLES = %d, GRID = %d;
  const m = (x) => x && [x.a, x.b, x.c, x.d, x.e, x.f];
  const out = {};
  for (const svg of document.querySelectorAll('svg[data-case]')) {
    const el = svg.querySelector('[data-inner]') || svg.querySelector('[data-probe]');
    const r = svg.getBoundingClientRect();
    const rec = {svg_rect: [r.x, r.y, r.width, r.height]};
    out[svg.dataset.case] = rec;
    try {
    const b = el.getBBox();
    rec.bbox = [b.x, b.y, b.width, b.height];
    rec.ctm = m(el.getCTM());
    rec.screen_ctm = m(el.getScreenCTM());
    if (el instanceof SVGGeometryElement) {
      const total = el.getTotalLength();
      rec.total_length = total;
      rec.points = [];
      for (let i = 0; i <= SAMPLES; i++) {
        try {
          const p = el.getPointAtLength(total * i / SAMPLES);
          rec.points.push([p.x, p.y]);
        } catch (e) { rec.points_error = String(e); rec.points = null; break; }
      }
      const sw = parseFloat(getComputedStyle(el).strokeWidth) || 0;
      const x0 = b.x - sw, y0 = b.y - sw, w = b.width + 2 * sw, h = b.height + 2 * sw;
      rec.grid = [x0, y0, w, h];
      let fill = '', stroke = '';
      for (let j = 0; j < GRID; j++) for (let i = 0; i < GRID; i++) {
        const pt = new DOMPoint(x0 + w * (i + 0.5) / GRID, y0 + h * (j + 0.5) / GRID);
        fill += el.isPointInFill(pt) ? '1' : '0';
        stroke += el.isPointInStroke(pt) ? '1' : '0';
      }
      rec.in_fill = fill; rec.in_stroke = stroke;
    }
    } catch (e) { rec.error = String(e); }
  }
  return out;
})()
""" % (LENGTH_SAMPLES, GRID)


def page_html(cases: list[dict[str, str]]) -> str:
    # Each case gets its own fixed slot, so getScreenCTM() does not depend on
    # text metrics or on the cases before it.
    rows = "\n".join(
        f'<div class="c" style="top:{SLOT * index}px">'
        + c["svg"].replace("<svg ", '<svg data-case="%s" ' % c["id"], 1)
        + "</div>"
        for index, c in enumerate(cases)
    )
    return ("<!doctype html><meta charset=utf-8>"
            "<style>body{margin:0;font:16px/1 monospace}.c{position:absolute;left:0}"
            "svg{display:block}</style>\n"
            f"{rows}\n")


def capture(chrome: Path, cases: list[dict[str, str]]) -> tuple[dict[str, dict], str]:
    """Return the per-case results and the browser's own version string."""
    html = page_html(cases)
    with tempfile.TemporaryDirectory(prefix="div-svg-geometry-") as tmp, ExitStack() as stack:
        page = Path(tmp) / "corpus.html"
        page.write_text(html, encoding="utf-8")
        profile = Path(tmp) / "profile"
        process = subprocess.Popen(
            [str(chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0",
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
        client.command("Page.enable")
        client.command("Page.navigate", {"url": page.as_uri()})
        deadline = time.monotonic() + 30
        while evaluate(client, "document.readyState") != "complete":
            if time.monotonic() > deadline:
                raise TimeoutError("corpus page did not load")
            time.sleep(0.05)
        return evaluate(client, PAGE_SCRIPT), version


def evaluate(client, expression: str):
    response = client.command(
        "Runtime.evaluate", {"expression": expression, "returnByValue": True})
    if "exceptionDetails" in response:
        raise RuntimeError(f"page script failed: {response['exceptionDetails']}")
    return response.get("result", {}).get("value")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list")
    cap = sub.add_parser("capture")
    cap.add_argument("--chrome", type=Path, required=True)
    cap.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    cases = corpus()
    ids = [c["id"] for c in cases]
    if len(set(ids)) != len(ids):
        raise SystemExit("duplicate case ids")
    if args.command == "list":
        for c in cases:
            print(f'{c["id"]}\t{c["svg"]}')
        print(f"{len(cases)} cases", file=sys.stderr)
        return 0

    for attempt in range(3):
        try:
            results, version = capture(args.chrome, cases)
            break
        except (RuntimeError, TimeoutError, OSError) as error:
            if attempt == 2:
                raise
            print(f"capture attempt {attempt + 1} failed ({error}); retrying", file=sys.stderr)
    missing = sorted(set(ids) - set(results or {}))
    if missing:
        raise SystemExit(f"no result for {len(missing)} cases, first {missing[0]}")
    corpus_text = json.dumps(cases, sort_keys=True)
    document = {
        "schema": SCHEMA,
        "captured": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "chromium": version,
        "pinned": EXPECTED_CHROMIUM in version,
        "platform": sys.platform,
        "corpus_sha256": hashlib.sha256(corpus_text.encode()).hexdigest(),
        "cases": [{**c, "chromium": results[c["id"]]} for c in cases],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # One case per line keeps the tracked oracle small and its diffs readable.
    header = {k: v for k, v in document.items() if k != "cases"}
    lines = ["{"]
    lines += [f"{json.dumps(k)}: {json.dumps(v)}," for k, v in sorted(header.items())]
    lines.append('"cases": [')
    lines.append(",\n".join(json.dumps(c, sort_keys=True) for c in document["cases"]))
    lines.append("]}")
    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    json.loads(args.output.read_text(encoding="utf-8"))
    print(f"{len(cases)} cases captured from {version} into {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
