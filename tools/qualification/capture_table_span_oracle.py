#!/usr/bin/env python3
"""Capture pinned Chromium 147 behavior for table span attributes.

This is a black-box capture: it runs pages in the browser and records what
script can observe. The value corpus is
tests/fidelity/fixtures/table-spans-v1/values.json. For each value the script
records the reflected colSpan, rowSpan and span IDL attributes, what the IDL
setters write back, and the geometry of small tables that use the value.

Linux needs the pinned binary through --chrome. Windows uses the pinned
Chrome-for-Testing build.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fidelity/fixtures/table-spans-v1"
CHROMIUM_VERSION = "147.0.7727.50"
VIEWPORT = (1200, 800)

# The grid is wide enough to see the 1000-column limit, plus a marker column.
# One-pixel columns keep every table inside the viewport, so these cases measure
# spans and not how an overflowing auto table shrinks.
WIDE_COLUMNS = 1002
NARROW_COLUMNS = 6
SECTION_ROWS = 5

PROBE = r"""
(corpus, wideColumns, narrowColumns, sectionRows) => {
  const make = (tag, parent, css) => {
    const element = document.createElement(tag);
    if (css) element.style.cssText = css;
    if (parent) parent.appendChild(element);
    return element;
  };
  const rect = element => {
    const box = element.getBoundingClientRect();
    return [box.x, box.y, box.width, box.height];
  };
  const put = (element, name, value) => {
    if (value !== null) element.setAttribute(name, value);
  };
  const newTable = () => make('table', document.body,
    'border-collapse:separate;border-spacing:0;table-layout:auto');
  const cellCss = 'padding:0;border:0';

  const reflection = corpus.attribute_values.map(value => {
    const entry = {value};
    for (const tag of ['td', 'th']) {
      const colspan = document.createElement(tag);
      put(colspan, 'colspan', value);
      const rowspan = document.createElement(tag);
      put(rowspan, 'rowspan', value);
      entry[tag] = {colSpan: colspan.colSpan, rowSpan: rowspan.rowSpan};
    }
    for (const tag of ['col', 'colgroup']) {
      const element = document.createElement(tag);
      put(element, 'span', value);
      entry[tag] = {span: element.span};
    }
    return entry;
  });

  const setters = corpus.idl_values.map(value => {
    const entry = {value};
    for (const tag of ['td', 'th']) {
      const colspan = document.createElement(tag);
      colspan.colSpan = value;
      const rowspan = document.createElement(tag);
      rowspan.rowSpan = value;
      entry[tag] = {
        colSpan: colspan.colSpan,
        colspanAttribute: colspan.getAttribute('colspan'),
        rowSpan: rowspan.rowSpan,
        rowspanAttribute: rowspan.getAttribute('rowspan'),
      };
    }
    for (const tag of ['col', 'colgroup']) {
      const element = document.createElement(tag);
      element.span = value;
      entry[tag] = {span: element.span, spanAttribute: element.getAttribute('span')};
    }
    return entry;
  });

  const columnsFor = reflected => reflected >= narrowColumns - 1 ? wideColumns : narrowColumns;

  const colspanLayout = (value, reflected) => {
    const table = newTable();
    const body = make('tbody', table);
    const first = make('tr', body);
    const target = make('td', first, cellCss + ';height:10px');
    put(target, 'colspan', value);
    const marker = make('td', first, cellCss + ';height:10px');
    const grid = make('tr', body);
    const columns = columnsFor(reflected);
    for (let i = 0; i < columns; i++) make('td', grid, cellCss + ';width:1px;height:1px');
    const result = {columns, table: rect(table), target: rect(target), marker: rect(marker)};
    table.remove();
    return result;
  };

  const rowspanLayout = value => {
    const table = newTable();
    const body = make('tbody', table);
    let target = null;
    for (let i = 0; i < sectionRows; i++) {
      const row = make('tr', body, 'height:10px');
      if (i === 0) {
        target = make('td', row, cellCss + ';width:10px');
        put(target, 'rowspan', value);
      }
      make('td', row, cellCss + ';width:10px;height:10px');
    }
    const next = make('tbody', table);
    const after = make('td', make('tr', next, 'height:10px'), cellCss + ';width:10px;height:10px');
    const result = {table: rect(table), target: rect(target), after: rect(after)};
    table.remove();
    return result;
  };

  const columnLayout = (kind, value, reflected) => {
    const table = newTable();
    if (kind === 'col') {
      const group = make('colgroup', table);
      put(make('col', group, 'width:1px'), 'span', value);
      make('col', group, 'width:7px');
    } else if (kind === 'colgroup') {
      put(make('colgroup', table, 'width:1px'), 'span', value);
      make('colgroup', table, 'width:7px');
    } else {
      // A colgroup with col children: the table model ignores its span.
      const group = make('colgroup', table, 'width:3px');
      put(group, 'span', value);
      make('col', group, 'width:5px');
      make('col', make('colgroup', table), 'width:7px');
    }
    const row = make('tr', make('tbody', table));
    const columns = columnsFor(reflected);
    const cells = [];
    for (let i = 0; i < columns; i++) cells.push(make('td', row, cellCss + ';height:4px'));
    const probe = Math.min(reflected, columns - 1);
    const result = {
      columns,
      probe,
      table: rect(table),
      first: rect(cells[0]),
      at_probe: rect(cells[probe]),
      last: rect(cells[columns - 1]),
    };
    table.remove();
    return result;
  };

  const layout = reflection.map(entry => ({
    value: entry.value,
    colspan: colspanLayout(entry.value, entry.td.colSpan),
    rowspan: rowspanLayout(entry.value),
    col: columnLayout('col', entry.value, entry.col.span),
    colgroup: columnLayout('colgroup', entry.value, entry.colgroup.span),
    colgroup_with_col: columnLayout('colgroup_with_col', entry.value, 1),
  }));

  return {reflection, setters, layout};
}
"""

PAGE = (
    "<!DOCTYPE html><html><head><style>html,body{margin:0;background:white}</style>"
    "</head><body></body></html>"
)


def harness():
    path = ROOT / "tools/accountability/run_all_pixel_comparisons.py"
    spec = importlib.util.spec_from_file_location("run_all_pixel_comparisons", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def evaluate(client, expression: str):
    result = client.command(
        "Runtime.evaluate",
        {"expression": expression, "awaitPromise": True, "returnByValue": True},
    )
    if "exceptionDetails" in result:
        raise SystemExit(json.dumps(result["exceptionDetails"])[:800])
    return result["result"]["value"]


def run_probe(client, page: Path, corpus: dict):
    version = client.command("Browser.getVersion").get("product", "")
    if CHROMIUM_VERSION not in version:
        raise SystemExit(f"unexpected browser version: {version}")
    client.command("Page.navigate", {"url": "file:" + urllib.request.pathname2url(str(page))})
    evaluate(
        client,
        "new Promise(resolve => { if (document.readyState === 'complete') resolve(true);"
        "else addEventListener('load', () => resolve(true), {once: true}); })",
    )
    arguments = ",".join(
        json.dumps(item) for item in (corpus, WIDE_COLUMNS, NARROW_COLUMNS, SECTION_ROWS)
    )
    return evaluate(client, f"({PROBE})({arguments})")


def capture_linux(page: Path, corpus: dict, chrome: Path):
    h = harness()
    env = h.chrome_environment(str(chrome.parent))
    flags, viewport = h.device_scale_launch(VIEWPORT[0], VIEWPORT[1], 1.0)
    with tempfile.TemporaryDirectory(prefix="div-span-oracle-") as profile:
        process = subprocess.Popen(
            [str(chrome), "--headless", "--disable-gpu", "--no-sandbox", "--no-first-run",
             "--no-default-browser-check", "--remote-debugging-port=0",
             f"--user-data-dir={profile}", *flags, "about:blank"],
            env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        client = None
        try:
            port = h._wait_for_devtools_endpoint(profile, process, timeout=30)
            client = h._CdpWebSocket(h._page_websocket_url(port, timeout=30))
            client.command("Page.enable")
            client.command("Emulation.setDeviceMetricsOverride", viewport)
            return run_probe(client, page, corpus)
        finally:
            if client is not None:
                h._close_chromium_client(client)
            h._stop_chromium_process(process)


def capture_windows(page: Path, corpus: dict, chrome: Path | None):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    with windows_chrome.WindowsChrome(
        chrome or windows_chrome.DEFAULT_CHROME, VIEWPORT[0], VIEWPORT[1], 1.0
    ) as browser:
        return run_probe(browser.client, page, corpus)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--platform", choices=["linux", "windows"], required=True)
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    corpus = json.loads((FIXTURE / "values.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="div-span-page-") as directory:
        page = Path(directory).resolve() / "spans.html"
        page.write_text(PAGE, encoding="utf-8")
        if args.platform == "linux":
            if args.chrome is None:
                raise SystemExit("--chrome is required on Linux")
            captured = capture_linux(page, corpus, args.chrome)
        else:
            captured = capture_windows(page, corpus, args.chrome)
    result = {
        "chromium": CHROMIUM_VERSION,
        "platform": args.platform,
        "viewport": [VIEWPORT[0], VIEWPORT[1], 1.0],
        "wide_columns": WIDE_COLUMNS,
        "narrow_columns": NARROW_COLUMNS,
        "section_rows": SECTION_ROWS,
        **captured,
    }
    output = args.output or FIXTURE / f"chromium-{args.platform}.json"
    output.write_text(
        json.dumps(result, ensure_ascii=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"wrote {len(captured['reflection'])} values to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
