#!/usr/bin/env python3
"""Capture pinned Chromium 147's `<dialog>` and top-layer behavior from real input.

Item H4 (RFC 0015) gives Div the `dialog` element, `showModal()`, the top
layer, `::backdrop`, inertness and Escape. This oracle records what
Chrome-for-Testing 147.0.7727.50 does, on its own page, before Div
implements any of it:

- opening with `showModal()` and `show()`, and closing with `close()`,
  `requestClose()`, Escape and removal: the `beforetoggle`, `toggle`,
  `cancel` and `close` events, their targets and `cancelable`, and
  `returnValue`. The synchronous events are kept in order. `toggle` (a
  task) and `close` (fired with the next animation frame) race with each
  other and with the input events, so each step records them apart,
  sorted;
- where focus goes on opening (autofocus, the first focusable descendant,
  a dialog with nothing focusable) and on closing;
- Tab and Shift+Tab inside a modal dialog;
- Escape when its `keydown` or the `cancel` event is cancelled, a second
  Escape with and without a click in between, and Escape in a text field
  and on a non-modal dialog;
- clicks on inert content and on the backdrop while a modal dialog is
  open, nested modal dialogs, script focus of inert content, and the
  exceptions `show()` and `showModal()` throw;
- the dialog's box and its computed `::backdrop` style at three viewport
  sizes (no input), for a small, a tall and a wide dialog, modal and not.

It reuses the keyboard scrolling script's browser, input and settling code
(real XTEST keys and buttons on X11, `--ozone-platform=x11`) with this
page's own markup, scenarios and actions. Dialog behavior does not depend
on the device scale, so it captures at 1x only. Each capture runs twice and
must match. The test `crates/divui/tests/dialog_oracle.rs` replays the JSON
through Div; until Div has the `dialog` element, every scenario is listed
there as pending.
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

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fidelity/fixtures/dialog-v1"

# Outside the dialogs: absolutely placed buttons and a non-focusable box,
# below both the modal dialogs (centred) and the non-modal ones (at the
# top), so a click on them lands on the backdrop while a modal dialog is
# open and on them otherwise.
OUTSIDE = [
    ("opener", "button", 10, 300, 60, 24),
    ("outside", "button", 80, 300, 60, 24),
    ("plain", "div", 250, 330, 200, 60),
]
# After the dialogs in tree order, so a Tab from `outside` stays in the page.
TRAILING = [("after", "button", 150, 300, 60, 24)]
# Dialogs: (id, rows of children). Each child is (id, tag, width, height,
# extra attributes). Children sit in one flex row with no whitespace, so
# no text metrics enter a dialog's size.
DIALOGS = [
    ("d1", [("d1text", "span", 60, 24, ""), ("d1a", "button", 60, 24, ""),
            ("d1b", "button", 60, 24, ""), ("d1close", "button", 60, 24, "")]),
    ("d2", [("d2a", "button", 60, 24, "")]),
    ("dAuto", [("autoA", "button", 60, 24, ""), ("autoB", "button", 60, 24, " autofocus")]),
    ("dEmpty", [("emptyText", "span", 80, 24, "")]),
    ("dInput", [("dInputField", "input", 80, 24, "")]),
    ("dTall", [("tallContent", "div", 100, 600, "")]),
    ("dWide", [("wideContent", "div", 700, 20, "")]),
]

PAGE = """<!DOCTYPE html><html id="html"><head><style>
html, body { margin: 0; padding: 0 }
.abs { position: absolute; box-sizing: border-box; margin: 0 }
button, input { box-sizing: border-box; margin: 0; padding: 0; border: 1px solid #888;
  font: 12px sans-serif }
dialog { font: 12px sans-serif }
.row { display: flex }
.row > * { flex: none; box-sizing: border-box; margin: 0 }
</style></head><body id="body">
BOXES
<script>
window.oracle = {events: [], later: []};
// `toggle` is a task and `close` fires with the next animation frame, so
// their order between themselves and the input events races: they are kept
// apart and sorted.
const ASYNC = ["toggle", "close"];
const name = t => t === window ? "window" : t === document ? "document"
  : (t && (t.id || t.nodeName)) || null;
const D = id => document.getElementById(id);
for (const type of ["keydown", "keyup", "mousedown", "mouseup", "click",
                    "beforetoggle", "toggle", "cancel", "close"]) {
  addEventListener(type, e => {
    const record = {type: e.type, target: name(e.target)};
    if (e.type.startsWith("key")) Object.assign(record, {key: e.key, shift: e.shiftKey});
    if (e.type === "cancel" || e.type === "beforetoggle") record.cancelable = e.cancelable;
    if (e.oldState !== undefined) Object.assign(record, {oldState: e.oldState,
                                                         newState: e.newState});
    if (e.type === "close") record.returnValue = e.target.returnValue;
    (ASYNC.includes(e.type) ? oracle.later : oracle.events).push(record);
  }, {capture: true});
}
// A script call, with any exception it throws recorded as an event.
window.act = f => {
  try { f(); } catch (e) { oracle.events.push({type: "exception", name: e.name}); }
};
window.oracleRead = () => ({events: oracle.events.splice(0),
  later: oracle.later.splice(0).map(r => JSON.stringify(r)).sort().map(r => JSON.parse(r))});
window.oracleState = () => {
  const dialogs = {};
  for (const id of DIALOG_IDS) {
    const d = D(id);
    if (d) dialogs[id] = {open: d.hasAttribute("open"), modal: d.matches(":modal"),
                          returnValue: d.returnValue};
  }
  const active = document.activeElement;
  return {active: active ? (active.id || active.nodeName) : null, dialogs};
};
</script></body></html>"""


def child_markup(child) -> str:
    name, tag, width, height, extra = child
    style = f'style="width:{width}px;height:{height}px"'
    if tag == "input":
        return f'<input id="{name}" {style}{extra}>'
    label = name if tag == "button" else ""
    return f'<{tag} id="{name}" {style}{extra}>{label}</{tag}>'


def page() -> str:
    def placed(name, tag, left, top, width, height):
        label = name if tag == "button" else ""
        return (f'<{tag} id="{name}" class="abs" style="left:{left}px;top:{top}px;'
                f'width:{width}px;height:{height}px">{label}</{tag}>')

    boxes = [placed(*spec) for spec in OUTSIDE]
    for name, children in DIALOGS:
        row = "".join(child_markup(child) for child in children)
        boxes.append(f'<dialog id="{name}"><div class="row">{row}</div></dialog>')
    boxes.extend(placed(*spec) for spec in TRAILING)
    ids = [name for name, _ in DIALOGS]
    return PAGE.replace("BOXES", "\n".join(boxes)).replace("DIALOG_IDS", json.dumps(ids))


def act(expression: str) -> str:
    return f"act(() => {expression})"


ACTIONS = {
    # Named page actions; the Div test implements each by name.
    **{f"show-modal-{d}": act(f"D('{d}').showModal()")
       for d in ("d1", "d2", "dAuto", "dEmpty", "dInput", "dTall", "dWide")},
    "show-d1": act("D('d1').show()"),
    "close-d1": act("D('d1').close()"),
    "close-d1-ok": act("D('d1').close('ok')"),
    "request-close-d1": act("D('d1').requestClose()"),
    "request-close-d1-rc": act("D('d1').requestClose('rc')"),
    "prevent-cancel-d1": "D('d1').addEventListener('cancel', e => e.preventDefault())",
    "prevent-escape-keydown": ("addEventListener('keydown', e => { if (e.key === 'Escape') "
                               "e.preventDefault(); })"),
    "prevent-beforetoggle-d1": "D('d1').addEventListener('beforetoggle', e => e.preventDefault())",
    "remove-d1": "D('d1').remove()",
    "remove-open-d1": "D('d1').removeAttribute('open')",
    "focus-outside": act("D('outside').focus()"),
    "detached-show-modal-d2": act("{ const d = D('d2'); d.remove(); d.showModal(); }"),
}

keys = base.keys


def do(*names):
    return [("do", name) for name in names]


def click(element):
    return [("click", element)]


SHIFT_TAB = [("key", "Tab", "s")]

SCENARIOS = {
    # Opening and closing from script, with focus restored to the opener.
    "modal_open_close": click("opener") + do("show-modal-d1", "close-d1"),
    # Tab and Shift+Tab inside a modal dialog: d1 has three buttons. A Tab
    # from the last one leaves the document for the browser (measured: the
    # capture stops when the page loses focus), so the scenario stays inside.
    "modal_tab": click("opener") + do("show-modal-d1") + keys("Tab", "Tab")
    + SHIFT_TAB + SHIFT_TAB,
    "escape_closes": click("opener") + do("show-modal-d1") + keys("Escape"),
    # A cancelled `cancel`, then Escape again with no activation in between.
    "escape_cancel_prevented": click("opener") + do("prevent-cancel-d1", "show-modal-d1")
    + keys("Escape", "Escape"),
    # The same with a click inside the dialog between the Escapes.
    "escape_cancel_prevented_activation": click("opener")
    + do("prevent-cancel-d1", "show-modal-d1") + keys("Escape") + click("d1a")
    + keys("Escape", "Escape"),
    "escape_keydown_prevented": click("opener") + do("prevent-escape-keydown", "show-modal-d1")
    + keys("Escape"),
    "escape_in_field": click("opener") + do("show-modal-dInput") + keys("A", "Escape"),
    "escape_non_modal": click("opener") + do("show-d1") + click("d1a") + keys("Escape"),
    # Clicks on inert content and on the backdrop, then Tab.
    "click_inert": click("opener") + do("show-modal-d1") + click("outside") + click("plain")
    + keys("Tab"),
    # A click on non-focusable content inside the modal dialog.
    "click_dialog_content": click("opener") + do("show-modal-d1") + click("d1text")
    + keys("Tab"),
    "script_focus_inert": click("opener") + do("show-modal-d1", "focus-outside"),
    "autofocus": click("opener") + do("show-modal-dAuto"),
    # Nothing inside can take focus, so a Tab would leave the document.
    "no_focusable": click("opener") + do("show-modal-dEmpty"),
    # A modal dialog over another: clicks on the lower one, then Escapes.
    "nested": click("opener") + do("show-modal-d1") + click("d1b") + do("show-modal-d2")
    + click("d1a") + keys("Escape", "Escape"),
    "non_modal_focus": click("opener") + do("show-d1", "close-d1"),
    "non_modal_focus_outside": click("opener") + do("show-d1") + click("outside")
    + do("close-d1"),
    "non_modal_click_outside": click("opener") + do("show-d1") + click("outside")
    + keys("Tab"),
    "remove_open_modal": click("opener") + do("show-modal-d1", "remove-d1") + click("outside")
    + keys("Tab"),
    # Chromium 147 does not close a dialog whose `open` is removed
    # (`DialogCloseWhenOpenRemoved` is experimental). A Tab after it leaves
    # the document (measured), so the scenario has none.
    "remove_open_attribute": click("opener") + do("show-modal-d1", "remove-open-d1")
    + click("outside") + do("show-modal-d1"),
    "return_value": click("opener") + do("show-modal-d1", "close-d1-ok", "show-modal-d1")
    + keys("Escape") + do("show-modal-d1", "request-close-d1-rc"),
    "request_close_prevented": click("opener")
    + do("prevent-cancel-d1", "show-modal-d1", "request-close-d1", "request-close-d1"),
    "invalid_state": click("opener") + do("show-d1", "show-modal-d1", "close-d1",
                                          "show-modal-d1", "show-d1", "detached-show-modal-d2"),
    "beforetoggle_prevented": click("opener") + do("prevent-beforetoggle-d1", "show-modal-d1"),
}

# The box geometry: (viewport, dialog, modal) without input.
VIEWPORTS = [(480, 400), (800, 600), (375, 667)]
GEOMETRY_DIALOGS = [("d1", True), ("d1", False), ("dTall", True), ("dWide", True),
                    ("dWide", False)]
GEOMETRY_READ = """(() => {
  const d = document.getElementById(%s);
  const r = d.getBoundingClientRect();
  const s = getComputedStyle(d);
  const b = getComputedStyle(d, '::backdrop');
  const pick = (style, names) => Object.fromEntries(names.map(n => [n, style[n]]));
  return {rect: [r.left, r.top, r.width, r.height],
          client: [d.clientWidth, d.clientHeight, d.scrollWidth, d.scrollHeight],
          style: pick(s, ['position', 'display', 'top', 'right', 'bottom', 'left',
                          'marginTop', 'marginLeft', 'maxWidth', 'maxHeight', 'overflowY',
                          'paddingTop', 'borderTopWidth', 'borderTopStyle',
                          'backgroundColor', 'color']),
          backdrop: pick(b, ['position', 'display', 'top', 'right', 'bottom', 'left',
                             'backgroundColor'])};
})()"""


def capture_geometry(browser) -> list:
    records = []
    width, height = base.WIDTH, base.HEIGHT
    try:
        for viewport in VIEWPORTS:
            base.WIDTH, base.HEIGHT = viewport
            browser.client.command("Emulation.setDeviceMetricsOverride", {
                "width": viewport[0], "height": viewport[1], "deviceScaleFactor": 0,
                "mobile": False})
            for dialog, modal in GEOMETRY_DIALOGS:
                browser.load(f"geometry-{viewport[0]}x{viewport[1]}-{dialog}-{modal}")
                method = "showModal" if modal else "show"
                browser.evaluate(f"document.getElementById('{dialog}').{method}()")
                browser.settle()
                records.append({"viewport": list(viewport), "dialog": dialog, "modal": modal,
                                **browser.evaluate(GEOMETRY_READ % json.dumps(dialog))})
    finally:
        base.WIDTH, base.HEIGHT = width, height
        browser.client.command("Emulation.setDeviceMetricsOverride", {
            "width": width, "height": height, "deviceScaleFactor": 0, "mobile": False})
    return records


def calibrate_with_retry(original):
    """The base calibration, retried: under WSLg the first move after the
    window appears is sometimes not delivered. Each retry starts from a
    different pointer position, so the move is a real motion."""
    def calibrate(browser, move):
        for attempt in range(1, 4):
            try:
                return original(browser, move)
            except RuntimeError as error:
                if "no mousemove reached" not in str(error):
                    raise
                x, y = browser.guess()
                move(x + 37 * attempt, y + 23 * attempt)
                time.sleep(1.0)
        return original(browser, move)
    return calibrate


def run_with_geometry(original):
    def run(browser, inputs, scenarios, run_index):
        results = original(browser, inputs, scenarios, run_index)
        return {"scenarios": results, "geometry": capture_geometry(browser)}
    return run


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True,
                        help="the pinned Chrome-for-Testing 147.0.7727.50 binary")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--only", help="capture these comma-separated scenarios once and print")
    args = parser.parse_args()
    if os.name == "nt":
        raise SystemExit("captured on Linux X11 only: Shift+Tab needs a held modifier")
    base.page = page
    base.ACTIONS = ACTIONS
    base.run_scenarios = run_with_geometry(base.run_scenarios)
    base.Browser.calibrate = calibrate_with_retry(base.Browser.calibrate)
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
        differing = [k for k in first["scenarios"]
                     if first["scenarios"][k] != second["scenarios"].get(k)]
        if first["geometry"] != second["geometry"]:
            differing.append("geometry")
        raise RuntimeError(f"captures differ in {differing}")
    document = {
        "chromium": base.CHROMIUM_VERSION,
        "platform": "linux-x11",
        "host": platform.platform(),
        "input": method,
        "viewport": [base.WIDTH, base.HEIGHT],
        "outside": [list(spec) for spec in OUTSIDE],
        "trailing": [list(spec) for spec in TRAILING],
        "dialogs": [[name, [list(child) for child in children]] for name, children in DIALOGS],
        "actions": ACTIONS,
        "scenarios": first["scenarios"],
        "geometry": first["geometry"],
    }
    output = args.output or FIXTURES / "chromium-linux-x11.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {output} in {time.monotonic() - started:.0f} s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
