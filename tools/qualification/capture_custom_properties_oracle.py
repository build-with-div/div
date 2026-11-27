"""Record how pinned Chromium 147 computes custom properties and `var()`.

Each case in `tests/fidelity/fixtures/custom-properties-v1/` is a style
sheet, `<case>.css`, and the body of a page, `<case>.html`, in the format
of `cascade-v1`. Chromium loads the sheet in a `<style>` element and
reports `getComputedStyle` for every element with an `id`: the standard
probe properties, and every custom property named anywhere in the case,
read with `getPropertyValue('--name')` (an empty string when the element
has no value for it). The native test
`crates/divui/tests/custom_properties_oracle.rs` compiles the same sheet
with `stylesheet!`, builds the same tree and requires the same values,
except where the test names a Div gap.

The standard probes are properties whose resolved value is their computed
value, or whose used value equals it for the `px` lengths the cases use.

Windows only: it uses the pinned Windows Chrome-for-Testing.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CASES = ROOT / "tests/fidelity/fixtures/custom-properties-v1"
OUTPUT = CASES / "chromium-windows.json"

PROBES = [
    "background-color",
    "border-top-width",
    "color",
    "display",
    "font-size",
    "font-weight",
    "line-height",
    "margin-left",
    "margin-top",
    "opacity",
    "padding-left",
    "padding-top",
]

CUSTOM_NAME = re.compile(r"--[A-Za-z0-9_\-]+")

SCRIPT = """
(() => {
  const probes = %s;
  const elements = {};
  for (const element of document.body.querySelectorAll('[id]')) {
    const style = getComputedStyle(element);
    const values = {};
    for (const name of probes) values[name] = style.getPropertyValue(name);
    elements[element.id] = values;
  }
  return JSON.stringify(elements);
})()
"""


def page(css: str, body: str) -> str:
    return f"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"


def custom_names(css: str, body: str) -> list[str]:
    """Every `--name` in the case, as written (custom names are case-sensitive)."""
    return sorted(set(CUSTOM_NAME.findall(css)) | set(CUSTOM_NAME.findall(body)))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    names = sorted(path.stem for path in CASES.glob("*.css"))
    cases = {}
    with tempfile.TemporaryDirectory(prefix="div-custom-properties-oracle-") as directory:
        with windows_chrome.WindowsChrome(
            args.chrome or windows_chrome.DEFAULT_CHROME, 200, 200, 1.0
        ) as browser:
            for name in names:
                css = (CASES / f"{name}.css").read_text(encoding="utf-8")
                body = (CASES / f"{name}.html").read_text(encoding="utf-8")
                page_path = Path(directory) / f"{name}.html"
                page_path.write_text(page(css, body), encoding="utf-8")
                browser.load(page_path)
                probes = PROBES + custom_names(css, body)
                cases[name] = {
                    "custom": custom_names(css, body),
                    "elements": json.loads(browser.evaluate(SCRIPT % json.dumps(probes))),
                }
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "probes": PROBES,
        "cases": cases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    elements = sum(len(case["elements"]) for case in cases.values())
    print(f"{len(cases)} cases, {elements} elements")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
