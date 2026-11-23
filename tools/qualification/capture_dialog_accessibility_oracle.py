#!/usr/bin/env python3
"""Capture what pinned Chromium 147's accessibility tree reports for `<dialog>`.

Each case loads one page, runs its steps, and reads the dialog, the content
inside it and the content outside it with CDP's Accessibility domain. This is
Chromium's computed accessibility tree, not what a screen reader receives
from the operating system. The whole capture runs twice and must match. The
test `crates/div-engine/tests/dialog_accessibility_oracle.rs` replays the JSON
through Div.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests/fidelity/components/harness"))
from reference_harness import chromium_page, chromium_version  # noqa: E402

FIXTURE = ROOT / "tests/fidelity/fixtures/dialog-accessibility-v1/chromium-linux-x11.json"
EXPECTED_VERSION = "147.0.7727.50"

PAGE = """<!doctype html><meta charset="utf-8"><title>dialog accessibility</title>
<body><button id="outside">out</button>
<dialog id="d"><button id="inside">in</button><p id="text">body text</p></dialog>
<dialog id="second"><button id="second-inside">two</button></dialog>
<h2 id="heading">Heading name</h2><p id="desc">Description text</p></body>
"""
NODES = ["d", "inside", "text", "second", "outside", "heading"]

STEPS = {
    "show": "d.show();",
    "show-modal": "d.showModal();",
    "close": "d.close();",
    "remove-open": "d.removeAttribute('open');",
    "set-open": "d.setAttribute('open', '');",
    "move": "document.body.appendChild(d);",
    "second-show": "second.show();",
    "second-show-modal": "second.showModal();",
}


def attr(name: str, value: str) -> list[str]:
    return ["attr", name, value]


CASES = {
    "closed": [],
    "open_attribute": ["set-open"],
    "show": ["show"],
    "show_modal": ["show-modal"],
    "show_aria_label": [attr("aria-label", "Label name"), "show"],
    "modal_aria_label": [attr("aria-label", "Label name"), "show-modal"],
    "show_labelledby": [attr("aria-labelledby", "heading"), "show"],
    "modal_labelledby": [attr("aria-labelledby", "heading"), "show-modal"],
    "modal_title": [attr("title", "Title name"), "show-modal"],
    "modal_describedby": [attr("aria-describedby", "desc"), "show-modal"],
    "show_describedby": [attr("aria-describedby", "desc"), "show"],
    "modal_labelledby_inside": [attr("aria-labelledby", "text"), "show-modal"],
    "modal_describedby_inside": [attr("aria-describedby", "text"), "show-modal"],
    "modal_aria_modal_false": [attr("aria-modal", "false"), "show-modal"],
    "modal_aria_modal_true": [attr("aria-modal", "true"), "show-modal"],
    "show_aria_modal_true": [attr("aria-modal", "true"), "show"],
    "show_aria_modal_false": [attr("aria-modal", "false"), "show"],
    "modal_aria_modal_upper_false": [attr("aria-modal", "FALSE"), "show-modal"],
    "show_aria_modal_upper_true": [attr("aria-modal", "TRUE"), "show"],
    "modal_aria_modal_empty": [attr("aria-modal", ""), "show-modal"],
    "show_aria_modal_empty": [attr("aria-modal", ""), "show"],
    "modal_aria_modal_invalid": [attr("aria-modal", "maybe"), "show-modal"],
    "show_aria_modal_invalid": [attr("aria-modal", "maybe"), "show"],
    "modal_aria_modal_spaces": [attr("aria-modal", " false "), "show-modal"],
    "show_aria_modal_spaces": [attr("aria-modal", " false "), "show"],
    "show_role_alertdialog": [attr("role", "alertdialog"), "show"],
    "modal_role_alertdialog": [attr("role", "alertdialog"), "show-modal"],
    "show_role_none": [attr("role", "none"), "show"],
    "modal_role_none": [attr("role", "none"), "show-modal"],
    "modal_role_button": [attr("role", "button"), "show-modal"],
    "modal_closed": ["show-modal", "close"],
    "modal_open_removed": ["show-modal", "remove-open"],
    "modal_moved": ["show-modal", "move"],
    "two_modals": ["show-modal", "second-show-modal"],
    "show_under_modal": ["show-modal", "second-show"],
}


def script(steps: list) -> str:
    parts = []
    for step in steps:
        if isinstance(step, list):
            parts.append(f"d.setAttribute({json.dumps(step[1])}, {json.dumps(step[2])});")
        else:
            parts.append(STEPS[step])
    return "".join(parts)


def summary(node: dict) -> dict:
    properties = {p["name"]: p["value"].get("value") for p in node.get("properties", [])}
    return {
        "ignored": node.get("ignored", False),
        "ignoredReasons": sorted(r["name"] for r in node.get("ignoredReasons", [])),
        "role": (node.get("role") or {}).get("value"),
        "name": (node.get("name") or {}).get("value"),
        "description": (node.get("description") or {}).get("value"),
        "modal": properties.get("modal"),
    }


def evaluate(client, expression: str, **options) -> dict:
    return client.command(
        "Runtime.evaluate", {"expression": expression, "awaitPromise": True, **options}
    )["result"]


def capture(client, url: str) -> dict:
    results = {}
    for name, steps in CASES.items():
        client.command("Page.navigate", {"url": url})
        for _ in range(200):
            ready = evaluate(
                client,
                "document.readyState + ':' + !!document.getElementById('desc')",
                returnByValue=True,
            ).get("value")
            if ready == "complete:true":
                break
        else:
            raise SystemExit(f"{name}: the page did not load")
        evaluate(
            client,
            "(() => { const d = document.getElementById('d'); "
            "const second = document.getElementById('second'); " + script(steps) + " })()",
        )
        evaluate(client, "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))")
        record = {}
        for node in NODES:
            object_id = evaluate(client, f"document.getElementById('{node}')")["objectId"]
            nodes = client.command(
                "Accessibility.getPartialAXTree", {"objectId": object_id, "fetchRelatives": False}
            )["nodes"]
            record[node] = summary(nodes[0])
        results[name] = {"steps": steps, "nodes": record}
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True, help="pinned Chrome for Testing")
    parser.add_argument("--output", type=Path, default=FIXTURE)
    args = parser.parse_args()
    chrome = args.chrome.resolve()
    version = chromium_version(chrome)
    if version != EXPECTED_VERSION:
        raise SystemExit(f"expected Chromium {EXPECTED_VERSION}, found {version}")
    with tempfile.TemporaryDirectory() as directory:
        page = Path(directory) / "page.html"
        page.write_text(PAGE, encoding="utf-8", newline="\n")
        with chromium_page(chrome) as client:
            client.command(
                "Emulation.setDeviceMetricsOverride",
                {"width": 480, "height": 400, "deviceScaleFactor": 1, "mobile": False},
            )
            client.command("Accessibility.enable", {})
            first = capture(client, page.as_uri())
            second = capture(client, page.as_uri())
    if first != second:
        differing = [name for name in first if first[name] != second[name]]
        raise SystemExit(f"the two captures differ in {differing}")
    document = {"chromium": version, "page": PAGE, "nodes": NODES, "cases": first}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(first)} cases, both captures identical, wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
