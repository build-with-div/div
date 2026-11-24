#!/usr/bin/env python3
"""Capture pinned Chromium 147's focus fixup from real input.

When the focused element stops being rendered or stops being focusable,
HTML's focus fixup makes the document's viewport the focused area. Div
keeps such an element focused (H4, RFC 0015: the dialog oracle's
`remove_open_attribute`). This oracle records what Chrome-for-Testing
147.0.7727.50 does, on its own page, for these causes:

- `display: none` on the focused element, and on an ancestor of it;
- `visibility: hidden` on the focused element;
- `disabled` on a focused button;
- removing `tabindex` from a focused `div`, which stays rendered;
- removing `open` from a non-modal and from a modal dialog that holds focus;
- removing the focused element from the document;
- showing a hidden element again, which does not bring focus back.

Each mutation runs from script. It records `document.activeElement` right
after the mutation (`sync`), after a forced layout (`layout`) and in the
second animation frame's callbacks (`frames`), as `mark` records among the
`focus`, `blur`, `focusin` and `focusout` events, so the record shows
whether those events fire and when. A Tab after the
fixup shows where sequential navigation resumes.

It reuses the keyboard scrolling script's browser, input and settling code
(real XTEST keys and buttons on X11, `--ozone-platform=x11`) with this
page's own markup, scenarios and actions, at 1x only. The capture runs
twice and both runs must match.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_keyboard_scroll_oracle as base  # noqa: E402
import capture_dialog_oracle as dialog_capture  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fidelity/fixtures/focus-fixup-v1"

# (id, tag, left, top, width, height, extra attributes), absolutely placed
# in one row below the non-modal dialog, in tree order. `w` sits inside
# `wrap`, whose box it fills.
ROW = [
    ("opener", "button", 10, 300, 60, 24, ""),
    ("a", "button", 80, 300, 60, 24, ""),
    ("b", "button", 150, 300, 60, 24, ""),
    ("wrap", "div", 220, 300, 60, 24, ""),
    ("t", "div", 290, 300, 60, 24, ' tabindex="0"'),
    ("c", "button", 360, 300, 60, 24, ""),
]
WRAPPED = ("w", "button", 60, 24)
# Dialogs after the row in tree order: (id, child button).
DIALOGS = [("d1", "d1a"), ("d2", "d2a")]

PAGE = """<!DOCTYPE html><html id="html"><head><style>
html, body { margin: 0; padding: 0 }
.abs { position: absolute; box-sizing: border-box; margin: 0 }
button { box-sizing: border-box; margin: 0; padding: 0; border: 1px solid #888;
  font: 12px sans-serif }
dialog { font: 12px sans-serif }
dialog > button { display: block; width: 60px; height: 24px }
#t { border: 1px solid #888 }
</style></head><body id="body">
BOXES
<script>
window.oracle = {events: []};
const name = t => t === window ? "window" : t === document ? "document"
  : (t && (t.id || t.nodeName)) || null;
const D = id => document.getElementById(id);
for (const type of ["keydown", "keyup", "mousedown", "mouseup", "click",
                    "focus", "blur", "focusin", "focusout"]) {
  addEventListener(type, e => {
    const record = {type: e.type, target: name(e.target)};
    if (e.type.startsWith("key")) Object.assign(record, {key: e.key, shift: e.shiftKey});
    if (e.type.startsWith("focus") || e.type === "blur") {
      record.related = name(e.relatedTarget);
    }
    oracle.events.push(record);
  }, {capture: true});
}
const active = () => document.activeElement ? name(document.activeElement) : null;
// A mutation, then the focused element right after it, after a forced
// layout, and in the second animation frame's callbacks, recorded in order
// with the events. A blur from the fixup lands on either side of the first
// frame's callbacks (measured: it varies between runs), so that frame has
// no mark.
const mark = at => oracle.events.push({type: "mark", at, active: active()});
window.fix = f => {
  f();
  mark("sync");
  void document.body.offsetWidth;
  mark("layout");
  requestAnimationFrame(() => requestAnimationFrame(() => mark("frames")));
};
window.oracleRead = () => ({events: oracle.events.splice(0)});
window.oracleState = () => ({active: active(),
  dialogs: Object.fromEntries(DIALOG_IDS.map(id => [id, {open: D(id).hasAttribute("open"),
                                                         modal: D(id).matches(":modal")}]))});
</script></body></html>"""


def page() -> str:
    boxes = []
    for name, tag, left, top, width, height, extra in ROW:
        style = f'left:{left}px;top:{top}px;width:{width}px;height:{height}px'
        if name == "wrap":
            inner, _, w, h = WRAPPED
            label = (f'<button id="{inner}" style="width:{w}px;height:{h}px">{inner}</button>')
        else:
            label = name if tag == "button" else ""
        boxes.append(f'<{tag} id="{name}" class="abs" style="{style}"{extra}>{label}</{tag}>')
    for name, child in DIALOGS:
        boxes.append(f'<dialog id="{name}"><button id="{child}">{child}</button></dialog>')
    ids = [name for name, _ in DIALOGS]
    return PAGE.replace("BOXES", "\n".join(boxes)).replace("DIALOG_IDS", json.dumps(ids))


def fix(expression: str) -> str:
    return f"fix(() => {{ {expression}; }})"


ACTIONS = {
    # Named page actions; the Div test implements each by name.
    "hide-a": fix("D('a').style.display = 'none'"),
    "show-a": fix("D('a').style.display = ''"),
    "hide-wrap": fix("D('wrap').style.display = 'none'"),
    "invisible-a": fix("D('a').style.visibility = 'hidden'"),
    "disable-a": fix("D('a').disabled = true"),
    "untab-t": fix("D('t').removeAttribute('tabindex')"),
    "remove-a": fix("D('a').remove()"),
    "show-d1": "D('d1').show()",
    "show-modal-d2": "D('d2').showModal()",
    "remove-open-d1": fix("D('d1').removeAttribute('open')"),
    "remove-open-d2": fix("D('d2').removeAttribute('open')"),
}

keys = base.keys


def do(*names):
    return [("do", name) for name in names]


def click(element):
    return [("click", element)]


SCENARIOS = {
    "hide_self": click("a") + do("hide-a") + keys("Tab"),
    "hide_ancestor": click("w") + do("hide-wrap") + keys("Tab"),
    "visibility_hidden": click("a") + do("invisible-a") + keys("Tab"),
    "disabled": click("a") + do("disable-a") + keys("Tab"),
    # The div stays rendered but is no longer focusable.
    "tabindex_removed": click("t") + do("untab-t") + keys("Tab"),
    "hide_then_show": click("a") + do("hide-a", "show-a") + keys("Tab"),
    "remove_open_non_modal": click("opener") + do("show-d1") + click("d1a")
    + do("remove-open-d1") + keys("Tab"),
    # A Tab after this leaves the document (the dialog oracle measured it),
    # so the scenario has none.
    "remove_open_modal": click("opener") + do("show-modal-d2", "remove-open-d2"),
    "removed": click("a") + do("remove-a") + keys("Tab"),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True,
                        help="the pinned Chrome-for-Testing 147.0.7727.50 binary")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--only", help="capture these comma-separated scenarios once and print")
    args = parser.parse_args()
    if os.name == "nt":
        raise SystemExit("captured on Linux X11 only, as the dialog oracle is")
    base.page = page
    base.ACTIONS = ACTIONS
    base.Browser.calibrate = dialog_capture.calibrate_with_retry(base.Browser.calibrate)
    chrome = args.chrome.resolve()
    method = "X11 keys, modifiers, motion and buttons through XTEST, --ozone-platform=x11"
    scenarios = dict(SCENARIOS)
    if args.only:
        wanted = args.only.split(",")
        scenarios = {k: v for k, v in scenarios.items() if k in wanted}
        print(json.dumps(base.capture_x11(chrome, 1.0, scenarios, 0), indent=1))
        return 0

    started = time.monotonic()
    try:
        first = base.capture_x11(chrome, 1.0, scenarios, 1)
        second = base.capture_x11(chrome, 1.0, scenarios, 2)
    except base.Aborted as error:
        print(f"ABORTED after {time.monotonic() - started:.0f} s: {error}")
        return 2
    if first != second:
        differing = [k for k in first if first[k] != second.get(k)]
        raise RuntimeError(f"captures differ in {differing}")
    document = {
        "chromium": base.CHROMIUM_VERSION,
        "platform": "linux-x11",
        "host": platform.platform(),
        "input": method,
        "viewport": [base.WIDTH, base.HEIGHT],
        "row": [list(spec) for spec in ROW],
        "wrapped": list(WRAPPED),
        "dialogs": [list(spec) for spec in DIALOGS],
        "actions": ACTIONS,
        "scenarios": first,
    }
    output = args.output or FIXTURES / "chromium-linux-x11.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {output} in {time.monotonic() - started:.0f} s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
