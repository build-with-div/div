#!/usr/bin/env python3
"""Capture how pinned Chromium treats the CSS names Div has no lookup for.

For each name and value this records whether Chromium accepts the declaration
through `CSS.supports`, `style.setProperty`, a style attribute and a stylesheet.
Real properties are recorded too, as controls, so that the oracle cannot pass
because the page failed to parse anything.

    python tools/qualification/capture_css_dropped_names_oracle.py --chrome <chrome.exe>
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import windows_chrome  # noqa: E402

OUTPUT = ROOT / "tests/fidelity/fixtures/css-dropped-names-v1/chromium-windows.json"

DROPPED = {
    "block-ellipsis": ["auto", "no-ellipsis", '"x"'],
    "filter-blur": ["0", "2", "2px"],
    "filter-grayscale": ["0", "1", "0.5"],
    "font-synthesis-position": ["auto", "none"],
    "hanging-punctuation": ["none", "first", "last", "first last", "allow-end", "force-end"],
    "ruby-overhang": ["auto", "none"],
    "scroll-snap-align-block": ["none", "start", "center", "end"],
    "scroll-snap-align-inline": ["none", "start", "center", "end"],
    "scroll-snap-axis": ["none", "x", "y", "both", "block", "inline"],
    "scroll-snap-strictness": ["none", "proximity", "mandatory"],
    "scrollbar-thumb-color": ["auto", "red", "#123456"],
    "scrollbar-track-color": ["auto", "red", "#123456"],
}

CONTROLS = {
    "filter": ["none", "blur(2px)", "grayscale(1)"],
    "opacity": ["0.5"],
    "scroll-snap-align": ["none", "start"],
    "scroll-snap-type": ["none", "y mandatory"],
    "scrollbar-color": ["auto", "red blue"],
}

PROBE = r"""((groups) => {
  const element = document.getElementById('target');
  const declared = (style, name) => style.getPropertyValue(name) !== '';
  const row = (name, value) => {
    element.removeAttribute('style');
    element.style.setProperty(name, value);
    const setProperty = declared(element.style, name);
    element.removeAttribute('style');
    element.setAttribute('style', `${name}: ${value}`);
    const attribute = declared(element.style, name);
    element.removeAttribute('style');
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(`#target { ${name}: ${value}; }`);
    return {
      name,
      value,
      supports: CSS.supports(name, value),
      set_property: setProperty,
      attribute,
      sheet: declared(sheet.cssRules[0].style, name),
    };
  };
  const rows = (group) => Object.entries(group)
    .flatMap(([name, values]) => values.map((value) => row(name, value)));
  return { dropped: rows(groups.dropped), controls: rows(groups.controls) };
})(%s)"""


def capture(chrome: Path) -> dict:
    with tempfile.TemporaryDirectory() as directory:
        page = Path(directory) / "page.html"
        page.write_text("<!doctype html><div id=target></div>", encoding="utf-8")
        with windows_chrome.WindowsChrome(chrome, 200, 200, 1.0) as browser:
            browser.load(page)
            groups = json.dumps({"dropped": DROPPED, "controls": CONTROLS})
            return browser.evaluate(PROBE % groups)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chrome", type=Path, required=True)
    args = parser.parse_args()
    result = capture(args.chrome)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(json.dumps(result, indent=1, ensure_ascii=True) + "\n", encoding="utf-8", newline="\n")
    routes = ("supports", "set_property", "attribute", "sheet")
    accepted = sum(row[route] for row in result["dropped"] for route in routes)
    controls = sum(row[route] for row in result["controls"] for route in routes)
    print(f"dropped: {len(result['dropped'])} declarations, {accepted} of "
          f"{len(result['dropped']) * 4} route checks accepted")
    print(f"controls: {len(result['controls'])} declarations, {controls} of "
          f"{len(result['controls']) * 4} route checks accepted")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
