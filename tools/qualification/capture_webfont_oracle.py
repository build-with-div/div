#!/usr/bin/env python3
"""Capture pinned Chromium 147 frames of web-font text on Linux or Windows.

The lines are tests/fidelity/fixtures/webfont-fontations-v1/lines.txt: each
names an OFL font file under tests/fidelity/components/assets/fonts, its
family, a weight, a size and a text. Each line is one absolutely positioned
24px row, loaded through @font-face, so Chromium rasterizes it with its
Fontations (skrifa) web-font path. The frames are captured at device scales
1, 1.25, 1.5 and 2 with the census harness's Chromium launcher and the
real-font Fontconfig profile, and written as chromium-linux-<scale>.png
next to the spec, with manifest.json recording each file's SHA-256.

With --platform windows, the pinned Windows Chrome-for-Testing build captures
the same page at scales 1 and 2 (real zoom) through windows_chrome.py, which
refuses unless the system matches RFC 0012's oracle pin (ClearType on, RGB
order, no per-display ClearType tuning). The frames are
chromium-windows-<scale>.png, recorded in manifest-windows.json.

The native test crates/divui/tests/native_webfont_fontations.rs builds the
same page through the public API and compares every row exactly.

    python3 tools/qualification/capture_webfont_oracle.py \
        --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome
    python3 tools/qualification/capture_webfont_oracle.py --chrome ... --check
    python tools/qualification/capture_webfont_oracle.py --platform windows \
        --chrome out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe
"""

from __future__ import annotations

import argparse
import hashlib
import html
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = ROOT / "tests/fidelity/fixtures/webfont-fontations-v1"
SPEC = EVIDENCE / "lines.txt"
FONTS = ROOT / "tests/fidelity/components/assets/fonts"
EXPECTED_CHROMIUM = "147.0.7727.50"
WIDTH = 400
ROW = 24
LEFT = 4
SCALES = ["1", "1.25", "1.5", "2"]
# RFC 0012 pins Windows at 1x and 2x.
WINDOWS_SCALES = ["1", "2"]


def lines() -> list[dict]:
    result = []
    for line in SPEC.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        font, family, weight, size, text = (part.strip() for part in line.split("|", 4))
        result.append(
            {"font": font, "family": family, "weight": int(weight), "size": int(size), "text": text}
        )
    return result


def document(items: list[dict]) -> str:
    faces = {}
    for item in items:
        faces[(item["family"], item["font"])] = (FONTS / item["font"]).resolve().as_uri()
    rules = [
        f"@font-face{{font-family:'{family}';src:url('{url}') format('woff2');"
        "font-weight:100 900;font-style:normal;font-display:block}"
        for (family, _), url in faces.items()
    ]
    rows = []
    for index, item in enumerate(items):
        rows.append(
            f'<div style="position:absolute;left:{LEFT}px;top:{index * ROW}px;'
            f"font-family:'{item['family']}';font-weight:{item['weight']};"
            f'font-size:{item["size"]}px;line-height:{ROW}px;white-space:pre">'
            f"{html.escape(item['text'])}</div>"
        )
    return (
        "<!DOCTYPE html><html><head><style>"
        + "".join(rules)
        + "html,body{margin:0;overflow:hidden;background:white;color:black}"
        "</style></head><body>" + "".join(rows) + "</body></html>"
    )


def chromium_version(chrome: Path) -> str:
    output = subprocess.run([str(chrome), "--version"], capture_output=True, text=True).stdout
    return output.strip().split()[-1] if output.strip() else ""


def capture(page: Path, chrome: Path, height: int, scale: str, output: Path) -> None:
    harness_path = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", harness_path)
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    if not harness.render_chrome(
        str(page), str(output), str(chrome), str(chrome.parent), use_real_font=True,
        logical_width=WIDTH, logical_height=height, device_scale=float(scale),
    ):
        raise SystemExit(f"Chromium capture failed at scale {scale}")


def capture_windows(page: Path, chrome: Path, height: int, scale: str, output: Path) -> None:
    import windows_chrome

    # Checks the pinned binary and the RFC 0012 text raster pin first.
    with windows_chrome.WindowsChrome(chrome, WIDTH, height, float(scale)) as browser:
        browser.load(page)
        output.write_bytes(browser.screenshot())


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True)
    parser.add_argument("--platform", choices=["linux", "windows"], default="linux")
    parser.add_argument("--check", action="store_true", help="fail unless a recapture matches")
    args = parser.parse_args()
    windows = args.platform == "windows"
    # windows_chrome.py checks the Windows binary's hash and reported version.
    if not windows:
        version = chromium_version(args.chrome)
        if version != EXPECTED_CHROMIUM:
            raise SystemExit(f"expected Chromium {EXPECTED_CHROMIUM}, found {version!r}")
    scales = WINDOWS_SCALES if windows else SCALES
    manifest_path = EVIDENCE / ("manifest-windows.json" if windows else "manifest.json")
    items = lines()
    height = len(items) * ROW
    with tempfile.TemporaryDirectory(prefix="div-webfont-oracle-") as directory:
        directory = Path(directory)
        page = directory / "lines.html"
        page.write_text(document(items), encoding="utf-8")
        files = {}
        for scale in scales:
            name = f"chromium-{args.platform}-{scale}.png"
            (capture_windows if windows else capture)(
                page, args.chrome, height, scale, directory / name
            )
            files[name] = sha256(directory / name)
        manifest = {"chromium": EXPECTED_CHROMIUM, "platform": args.platform}
        if windows:
            manifest["text_raster"] = (
                "ClearType on, RGB order, no per-display ClearType tuning (RFC 0012)"
            )
        else:
            manifest["fontconfig"] = (
                "real-font profile (tools/accountability/run_all_pixel_comparisons.py)"
            )
        manifest |= {
            "viewport": [WIDTH, height],
            "row": ROW,
            "left": LEFT,
            "scales": scales,
            "fonts": {
                item["font"]: sha256(FONTS / item["font"]) for item in items
            },
            "files": files,
        }
        if args.check:
            committed = json.loads(manifest_path.read_text(encoding="utf-8"))
            if committed != manifest:
                raise SystemExit("recapture differs from the committed fixtures")
            print(f"recapture matches the committed fixtures ({len(files)} frames)")
            return 0
        for name in files:
            shutil.copyfile(directory / name, EVIDENCE / name)
    manifest_path.write_text(
        json.dumps(manifest, indent=1) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"wrote {len(files)} frames of {len(items)} lines to {EVIDENCE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
