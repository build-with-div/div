"""Record which media queries pinned Chromium 147 matches in a style sheet.

`tests/fidelity/fixtures/media-queries-v1/media.css` gives every element of
`media.html` an opacity of 0, and an `@media` rule per element makes it 1.
Chromium loads the sheet in a `<style>` element once per environment (a
viewport size, a device scale factor and the color scheme and
reduced-motion preferences) and reports each element's computed `opacity`.
The native test `crates/divui/tests/media_oracle.rs` compiles the same sheet
with `stylesheet!`, sets the same environment and requires the same values,
except where the test names a Div gap.

A device scale factor other than 1 is real zoom (`--force-device-scale-factor`,
RFC 0010), not CDP emulation. The preferences use CDP's
`Emulation.setEmulatedMedia`, as Div's are settings of the document.
Pointer and hover features report the capturing machine's input devices;
the recorded machine has a mouse and no touch screen.

Windows only: it uses the pinned Windows Chrome-for-Testing.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CASES = ROOT / "tests/fidelity/fixtures/media-queries-v1"
OUTPUT = CASES / "chromium-windows.json"

# name, width, height, device scale factor, color scheme, reduced motion
ENVIRONMENTS = [
    ("narrow", 200, 200, 1.0, "light", False),
    ("boundary", 600, 400, 1.0, "light", False),
    ("wide", 800, 600, 1.0, "light", False),
    ("portrait", 400, 600, 1.0, "light", False),
    ("zoom-125", 640, 480, 1.25, "light", False),
    ("zoom-200", 800, 600, 2.0, "light", False),
    ("dark-reduced", 600, 400, 1.0, "dark", True),
]

SCRIPT = """
(() => {
  const values = {};
  for (const element of document.body.querySelectorAll('[id]')) {
    values[element.id] = getComputedStyle(element).getPropertyValue('opacity');
  }
  return JSON.stringify(values);
})()
"""


def page(css: str, body: str) -> str:
    return f"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    css = (CASES / "media.css").read_text(encoding="utf-8")
    body = (CASES / "media.html").read_text(encoding="utf-8")
    environments = []
    with tempfile.TemporaryDirectory(prefix="div-media-oracle-") as directory:
        page_path = Path(directory) / "media.html"
        page_path.write_text(page(css, body), encoding="utf-8")
        for name, width, height, scale, scheme, reduced in ENVIRONMENTS:
            with windows_chrome.WindowsChrome(
                args.chrome or windows_chrome.DEFAULT_CHROME, width, height, scale
            ) as browser:
                browser.client.command("Emulation.setEmulatedMedia", {
                    "features": [
                        {"name": "prefers-color-scheme", "value": scheme},
                        {"name": "prefers-reduced-motion",
                         "value": "reduce" if reduced else "no-preference"},
                    ],
                })
                browser.load(page_path)
                values = json.loads(browser.evaluate(SCRIPT))
            environments.append({
                "name": name,
                "width": width,
                "height": height,
                "scale": scale,
                "color_scheme": scheme,
                "reduced_motion": reduced,
                "opacity": values,
            })
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "environments": environments,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    matched = sum(
        sum(value == "1" for value in environment["opacity"].values())
        for environment in environments
    )
    elements = len(environments[0]["opacity"])
    print(f"{len(environments)} environments, {elements} elements, {matched} matches")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
