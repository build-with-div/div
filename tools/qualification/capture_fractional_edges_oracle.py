#!/usr/bin/env python3
"""Capture pinned Chromium 147 frames of box edges at fractional device positions.

The pages are tests/fidelity/fixtures/fractional-edges-v1/pages.txt: a
single box, two abutting boxes, a block split across two columns, a table
caption split across two columns and an absolutely positioned box whose
child overflows into the next column, each with an edge at CSS x 45 and a
top edge at CSS y 21. They are captured at
device scales 1, 1.25, 1.5 and 2 with the census harness's Chromium launcher,
which uses real zoom above scale 1 (--force-device-scale-factor and a
viewport-only CDP override in headless Chromium; RFC 0010). The frames are
written as <page>-<scale>.png next to the spec, with manifest.json recording
each file's SHA-256.

These frames describe Chromium as the census oracle captures it. At real
zoom Blink lays out in device pixels, so every edge lands on a whole device
pixel.

The native test crates/divui/tests/native_fractional_edges.rs builds the
same pages through the public API and compares every frame exactly.

    python3 tools/qualification/capture_fractional_edges_oracle.py \
        --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome
    python3 tools/qualification/capture_fractional_edges_oracle.py --chrome ... --check
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = ROOT / "tests/fidelity/fixtures/fractional-edges-v1"
SPEC = EVIDENCE / "pages.txt"
EXPECTED_CHROMIUM = "147.0.7727.50"
WIDTH = 120
HEIGHT = 80
SCALES = ["1", "1.25", "1.5", "2"]


def pages() -> dict[str, list[tuple[int, str, str]]]:
    result: dict[str, list[tuple[int, str, str]]] = {}
    for line in SPEC.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        page, depth, tag, declarations = (part.strip() for part in line.split("|", 3))
        result.setdefault(page, []).append((int(depth), tag, declarations))
    return result


def document(boxes: list[tuple[int, str, str]]) -> str:
    body = []
    open_tags: list[str] = []
    for depth, tag, declarations in boxes:
        if depth > len(open_tags):
            raise SystemExit(f"box at depth {depth} has no parent")
        while len(open_tags) > depth:
            body.append(f"</{open_tags.pop()}>")
        body.append(f'<{tag} style="{declarations}">')
        open_tags.append(tag)
    while open_tags:
        body.append(f"</{open_tags.pop()}>")
    return (
        "<!DOCTYPE html><html><head><style>"
        "html,body{margin:0;overflow:hidden;background:white}"
        "</style></head><body>" + "".join(body) + "</body></html>"
    )


def chromium_version(chrome: Path) -> str:
    output = subprocess.run([str(chrome), "--version"], capture_output=True, text=True).stdout
    return output.strip().split()[-1] if output.strip() else ""


def capture(page: Path, chrome: Path, scale: str, output: Path) -> None:
    harness_path = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", harness_path)
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    if not harness.render_chrome(
        str(page), str(output), str(chrome), str(chrome.parent),
        logical_width=WIDTH, logical_height=HEIGHT, device_scale=float(scale),
    ):
        raise SystemExit(f"Chromium capture of {page.name} failed at scale {scale}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True)
    parser.add_argument("--check", action="store_true", help="fail unless a recapture matches")
    args = parser.parse_args()
    version = chromium_version(args.chrome)
    if version != EXPECTED_CHROMIUM:
        raise SystemExit(f"expected Chromium {EXPECTED_CHROMIUM}, found {version!r}")
    with tempfile.TemporaryDirectory(prefix="div-fractional-edges-oracle-") as directory:
        directory = Path(directory)
        files = {}
        for name, boxes in pages().items():
            page = directory / f"{name}.html"
            page.write_text(document(boxes), encoding="utf-8")
            for scale in SCALES:
                frame = f"{name}-{scale}.png"
                capture(page, args.chrome, scale, directory / frame)
                files[frame] = sha256(directory / frame)
        manifest = {
            "chromium": EXPECTED_CHROMIUM,
            "platform": "linux",
            "device_scale": "real zoom above scale 1: --force-device-scale-factor "
            "and a viewport-only CDP override, headless (RFC 0010, "
            "tools/accountability/run_all_pixel_comparisons.py)",
            "viewport": [WIDTH, HEIGHT],
            "scales": SCALES,
            "files": files,
        }
        if args.check:
            committed = json.loads((EVIDENCE / "manifest.json").read_text(encoding="utf-8"))
            if committed != manifest:
                raise SystemExit("recapture differs from the committed fixtures")
            print(f"recapture matches the committed fixtures ({len(files)} frames)")
            return 0
        for name in files:
            shutil.copyfile(directory / name, EVIDENCE / name)
    (EVIDENCE / "manifest.json").write_text(
        json.dumps(manifest, indent=1) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"wrote {len(files)} frames to {EVIDENCE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
