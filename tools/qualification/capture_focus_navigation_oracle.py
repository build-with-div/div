#!/usr/bin/env python3
"""Capture pinned Chromium 147's sequential focus navigation from real input.

Item I2b gives Div keyboard-focusable scrollers and the sequential focus
navigation starting point. The keyboard scrolling oracle
(`capture_keyboard_scroll_oracle.py`, `keyboard-scroll-v1`) measured the
basic case: a scroller without focusable descendants is in the Tab order, a
click does not focus it, and Tab after a click starts from the clicked node.
This oracle measures the rest on its own page:

- which scrollers are keyboard-focusable: one whose content fits (with
  `overflow: auto` and with `overflow: scroll`), one with `overflow:
  hidden`, and ones whose only focusable-looking descendant has
  `tabindex="-1"`, is disabled, is `display: none`, or is a `tabindex="0"`
  `div`;
- which of them `focus()` from script focuses;
- Tab and Shift+Tab after a click on non-focusable content, after the
  clicked node or its parent is removed, after `blur()`, and after the
  focused element is removed;
- that a scroller focused by Tab or by a click starts the keyboard scroll
  chain.

It reuses the keyboard scrolling script's browser, input and settling code
(the same real platform input: XTEST on Linux, posted messages to the
foreground window on Windows) with this page's own markup, scenarios and
actions. Focus does not depend on the device scale, so it captures at 1x only.
Each capture runs twice and must match. The test
`crates/divui/tests/focus_navigation_oracle.rs` replays the JSON through Div.
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
FIXTURES = ROOT / "tests/fidelity/fixtures/focus-navigation-v1"

# Scrollers: (id, left, top, extra style, content width, content height,
# markup inside the content). Content elements are `<id>Content`. Every
# scroller is 100x60, so a vertical scrollbar leaves an 85px-wide content
# box and 80px-wide content never overflows horizontally.
SCROLLERS = [
    ("fits", 10, 10, "overflow:auto", 80, 40, ""),
    ("hidden", 120, 10, "overflow:hidden", 80, 200, ""),
    ("scrollFits", 230, 10, "overflow:scroll", 40, 20, ""),
    ("negChild", 340, 10, "overflow:auto", 80, 200,
     '<button id="negChildBtn" tabindex="-1" style="left:5px;top:5px;width:50px;'
     'height:20px">n</button>'),
    ("disabledChild", 10, 90, "overflow:auto", 80, 200,
     '<button id="disabledChildBtn" disabled style="left:5px;top:5px;width:50px;'
     'height:20px">d</button>'),
    ("hiddenChild", 120, 90, "overflow:auto", 80, 200,
     '<button id="hiddenChildBtn" style="display:none;left:5px;top:5px;width:50px;'
     'height:20px">h</button>'),
    ("negScroller", 230, 90, "overflow:auto", 80, 200, ""),
    ("plain", 340, 90, "overflow:auto", 80, 200, ""),
    ("divChild", 230, 170, "overflow:auto", 80, 200,
     '<div id="divChildInner" tabindex="0" style="left:5px;top:5px;width:50px;'
     'height:20px"></div>'),
    # Overflows only on its hidden axis.
    ("crossAxis", 340, 170, "overflow-x:auto;overflow-y:hidden", 80, 200, ""),
]
BUTTONS = [f"b{i}" for i in range(1, 7)]

PAGE = """<!DOCTYPE html><html id="html"><head><style>
html, body { margin: 0; padding: 0 }
div, button { position: absolute; box-sizing: border-box; margin: 0 }
button { padding: 0; border: 1px solid #888; font: 12px sans-serif }
</style></head><body id="body">
BOXES
<script>
window.oracle = {events: [], scrolled: new Set()};
const name = t => t === document ? "root" : (t.id || t.nodeName);
for (const type of ["keydown", "keyup", "click"]) {
  addEventListener(type, e => oracle.events.push({type: e.type,
    key: e.key, code: e.code, shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey,
    meta: e.metaKey, target: name(e.target)}), {capture: true});
}
addEventListener("scroll", e => oracle.scrolled.add(name(e.target)), {capture: true});
window.oracleRead = () => {
  const out = {events: oracle.events.splice(0), scrolled: [...oracle.scrolled].sort()};
  oracle.scrolled.clear();
  return out;
};
window.oracleState = () => {
  const offsets = {root: [scrollX, scrollY]};
  for (const id of IDS) {
    const element = document.getElementById(id);
    if (element) offsets[id] = [element.scrollLeft, element.scrollTop];
  }
  const active = document.activeElement;
  return {offsets, active: active ? (active.id || active.nodeName) : null};
};
</script></body></html>"""


def scroller_markup(spec) -> str:
    name, left, top, style, content_width, content_height, inner = spec
    tabindex = ' tabindex="-1"' if name == "negScroller" else ""
    return (f'<div id="{name}"{tabindex} style="left:{left}px;top:{top}px;width:100px;'
            f'height:60px;{style}"><div id="{name}Content" style="left:0;top:0;'
            f'width:{content_width}px;height:{content_height}px">{inner}</div></div>')


def page() -> str:
    boxes = [scroller_markup(spec) for spec in SCROLLERS
             if spec[0] not in ("divChild", "crossAxis")]
    # A non-focusable wrapper and its child, between the scrollers and the
    # buttons in tree order.
    boxes.append('<div id="gapWrap" style="left:10px;top:170px;width:200px;height:40px">'
                 '<div id="gap" style="left:0;top:0;width:200px;height:40px"></div></div>')
    boxes.extend(scroller_markup(spec) for spec in SCROLLERS
                 if spec[0] in ("divChild", "crossAxis"))
    for index, button in enumerate(BUTTONS):
        boxes.append(f'<button id="{button}" style="left:{10 + 70 * index}px;top:250px;'
                     f'width:60px;height:24px">{button}</button>')
    ids = [spec[0] for spec in SCROLLERS]
    return (PAGE.replace("BOXES", "\n".join(boxes)).replace("IDS", json.dumps(ids)))


def focus_action(element: str) -> str:
    return (f"document.activeElement && document.activeElement.blur(); "
            f"document.getElementById('{element}').focus()")


ACTIONS = {
    # Named page actions; the Div test implements each by name.
    "focus-fits": focus_action("fits"),
    "focus-hidden": focus_action("hidden"),
    "focus-scrollFits": focus_action("scrollFits"),
    "focus-negChild": focus_action("negChild"),
    "focus-negScroller": focus_action("negScroller"),
    "focus-plain": focus_action("plain"),
    "focus-divChild": focus_action("divChild"),
    "focus-crossAxis": focus_action("crossAxis"),
    "remove-gap": "document.getElementById('gap').remove()",
    "remove-gapWrap": "document.getElementById('gapWrap').remove()",
    "remove-b3": "document.getElementById('b3').remove()",
    "blur-active": "document.activeElement.blur()",
    "focus-b3": focus_action("b3"),
    "cancel-mousedown": "addEventListener('mousedown', e => e.preventDefault())",
}

keys = base.keys

SCENARIOS = {
    # Eight Tabs: the page has at least eight stops (plain, divChildInner
    # and six buttons), so focus never leaves the page for the browser.
    "tab_order": keys(*["Tab"] * 8),
    "script_focus": [("do", f"focus-{name}") for name in (
        "fits", "hidden", "scrollFits", "negChild", "negScroller", "plain", "divChild",
        "crossAxis")],
    "script_focus_then_tab": [("do", "focus-negScroller")] + keys("Tab", "Tab"),
    "tab_to_scroller_down": [("click", "negScrollerContent")] + keys("Tab", "Down"),
    "click_content_tab": [("click", "plainContent")] + keys("Tab", "Down"),
    "click_gap_tab": [("click", "gap")] + keys("Tab"),
    "removed_start": [("click", "gap"), ("do", "remove-gap")] + keys("Tab"),
    "removed_start_parent": [("click", "gap"), ("do", "remove-gapWrap")] + keys("Tab"),
    "blurred": [("click", "b3"), ("do", "blur-active")] + keys("Tab"),
    "focused_removed": [("click", "b3"), ("do", "remove-b3")] + keys("Tab"),
    # Script focus, then blur: does focus set the starting point?
    "script_blurred": [("do", "focus-b3"), ("do", "blur-active")] + keys("Tab"),
    # With focus kept by a cancelled `mousedown`, does the click's starting
    # point or the focused element lead?
    "focused_cancelled_click": keys("Tab") + [("do", "cancel-mousedown"), ("click", "gap")]
    + keys("Tab"),
    # A click on non-focusable content blurs the focused button.
    "click_away": [("click", "b3"), ("click", "gap")] + keys("Tab"),
    # Linux only: Shift+Tab. From b6 there are at least seven stops back.
    "shift_tab_order": [("click", "b6")] + [("key", "Tab", "s")] * 7,
    "click_content_shift_tab": [("click", "plainContent"), ("key", "Tab", "s")],
    "click_gap_shift_tab": [("click", "gap"), ("key", "Tab", "s")],
    "removed_start_parent_shift_tab": [("click", "gap"), ("do", "remove-gapWrap"),
                                       ("key", "Tab", "s")],
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, required=True,
                        help="the pinned Chrome-for-Testing 147.0.7727.50 binary")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--only", help="capture these comma-separated scenarios once and print")
    args = parser.parse_args()
    base.page = page
    base.ACTIONS = ACTIONS
    windows = os.name == "nt"
    if windows:
        import windows_chrome

        chrome = windows_chrome.verify_chrome(args.chrome)
        name, capture = "chromium-windows.json", base.capture_windows
        method = ("WM_KEYDOWN/WM_KEYUP, WM_MOUSEMOVE and WM_LBUTTON{DOWN,UP} posted to the "
                  "pinned browser's foreground window; unmodified keys only")
        scenarios = {k: v for k, v in SCENARIOS.items() if not base.has_modifiers(v)}
    else:
        chrome = args.chrome.resolve()
        name, capture = "chromium-linux-x11.json", base.capture_x11
        method = "X11 keys, modifiers, motion and buttons through XTEST, --ozone-platform=x11"
        scenarios = dict(SCENARIOS)
    if args.only:
        wanted = args.only.split(",")
        scenarios = {k: v for k, v in scenarios.items() if k in wanted}
        print(json.dumps(capture(chrome, 1.0, scenarios, 0), indent=1))
        return 0

    started = time.monotonic()
    try:
        first = capture(chrome, 1.0, scenarios, 1)
        second = capture(chrome, 1.0, scenarios, 2)
    except base.Aborted as error:
        print(f"ABORTED after {time.monotonic() - started:.0f} s: {error}")
        return 2
    if first != second:
        differing = [k for k in first if first[k] != second.get(k)]
        raise RuntimeError(f"captures differ in {differing}")
    document = {
        "chromium": base.CHROMIUM_VERSION,
        "platform": "windows" if windows else "linux-x11",
        "host": platform.platform(),
        "input": method,
        "viewport": [base.WIDTH, base.HEIGHT],
        "scrollers": [list(spec) for spec in SCROLLERS],
        "buttons": BUTTONS,
        "actions": ACTIONS,
        "scenarios": first,
    }
    output = args.output or FIXTURES / name
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {output} in {time.monotonic() - started:.0f} s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
