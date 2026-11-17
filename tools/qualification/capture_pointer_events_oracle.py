#!/usr/bin/env python3
"""Capture pinned Chromium 147's pointer and mouse event sequences.

RFC 0007 section 4 (Tier 0 item 0.12 B) gives Div Pointer Events. This
script records what pinned Windows Chrome-for-Testing 147.0.7727.50 fires for
a fixed page under CDP `Input.dispatchMouseEvent` and `Input.dispatchTouchEvent`,
which enter the browser's input pipeline as the platform's events do. Each
scenario loads the page afresh, optionally installs listeners that cancel or
capture, then sends its inputs. Every element logs each event it receives at
its target phase, in order: type, target, button, buttons, pointer id, pointer
type, `isPrimary`, client and offset coordinates, and pressure. Focus changes
and the results of capture calls are logged too. Each scenario runs twice and
must match. The test `crates/divui/tests/pointer_events_oracle.rs` replays the
inputs through Div and compares the logs.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fidelity/fixtures/pointer-events-v1/chromium-windows.json"
WIDTH, HEIGHT = 400, 300

# Absolutely placed boxes, so Div builds the same geometry from the same
# numbers. `inner` is inside `outer`; `transformed` is scaled by 2 about its
# top-left corner.
BOXES = {
    "outer": (20, 20, 200, 150, "body"),
    "inner": (60, 50, 80, 60, "outer"),
    "sibling": (240, 20, 120, 80, "body"),
    "transformed": (240, 140, 40, 30, "body"),
}
BUTTON = (20, 200, 80, 30)
FIELD = (120, 200, 100, 30)

EVENTS = [
    "pointerover", "pointerenter", "pointerdown", "pointermove", "pointerup",
    "pointercancel", "pointerout", "pointerleave", "gotpointercapture",
    "lostpointercapture", "mouseover", "mouseenter", "mousedown", "mousemove",
    "mouseup", "mouseout", "mouseleave", "click", "auxclick", "contextmenu",
    "focus", "blur",
]


def page() -> str:
    def box(name, x, y, w, h, extra=""):
        return (f'<div id="{name}" style="position:absolute;left:{x}px;top:{y}px;'
                f'width:{w}px;height:{h}px;{extra}">')

    outer = BOXES["outer"]
    inner = BOXES["inner"]
    inner_local = (inner[0] - outer[0], inner[1] - outer[1])
    sibling = BOXES["sibling"]
    transformed = BOXES["transformed"]
    return (
        '<!DOCTYPE html><html id="html"><head><style>'
        'html, body { margin: 0; padding: 0; width: 100%; height: 100% }'
        'button, input { position: absolute; margin: 0; padding: 0; border: 0;'
        ' box-sizing: border-box; font: 12px sans-serif }'
        '</style></head><body id="body">'
        + box("outer", *outer[:4]) + box("inner", *inner_local, *inner[2:4]) + "</div></div>"
        + box("sibling", *sibling[:4]) + "</div>"
        + box("transformed", *transformed[:4], "transform:scale(2);transform-origin:0 0")
        + "</div>"
        + f'<button id="button" style="left:{BUTTON[0]}px;top:{BUTTON[1]}px;'
          f'width:{BUTTON[2]}px;height:{BUTTON[3]}px">b</button>'
        + f'<input id="field" style="left:{FIELD[0]}px;top:{FIELD[1]}px;'
          f'width:{FIELD[2]}px;height:{FIELD[3]}px">'
        + "</body></html>"
    )


LOGGER = """
(() => {
  window.log = [];
  const ids = ["html", "body", "outer", "inner", "sibling", "transformed", "button", "field"];
  const record = e => {
    if (e.eventPhase !== Event.AT_TARGET || e.currentTarget !== e.target) return;
    const entry = {type: e.type,
                   target: e.target === document ? "document" : e.target.id};
    if ("button" in e) {
      Object.assign(entry, {button: e.button, buttons: e.buttons,
        client: [e.clientX, e.clientY], offset: [e.offsetX, e.offsetY]});
    }
    if ("pointerId" in e) {
      Object.assign(entry, {pointerId: e.pointerId, pointerType: e.pointerType,
        isPrimary: e.isPrimary, pressure: e.pressure});
    }
    window.log.push(entry);
  };
  for (const element of [document, ...ids.map(id => document.getElementById(id))]) {
    for (const type of %s) element.addEventListener(type, record);
  }
})()
""" % json.dumps(EVENTS)

# Setup scripts run after the logger, before the inputs.
SETUP = {
    "prevent_pointerdown": "inner.addEventListener('pointerdown', e => e.preventDefault())",
    "prevent_mousedown": "button.addEventListener('mousedown', e => e.preventDefault())",
    "prevent_pointerdown_focus":
        "button.addEventListener('pointerdown', e => e.preventDefault())",
    "prevent_mouseup": "inner.addEventListener('mouseup', e => e.preventDefault())",
    "capture": """inner.addEventListener('pointerdown',
                    e => inner.setPointerCapture(e.pointerId))""",
    "capture_removed": """inner.addEventListener('pointerdown',
                    e => inner.setPointerCapture(e.pointerId));
                  inner.addEventListener('gotpointercapture',
                    () => setTimeout(() => inner.remove()))""",
    "capture_errors": """
      const attempt = (label, f) => {
        try { f(); window.log.push({type: label, result: 'ok'}); }
        catch (error) { window.log.push({type: label, result: error.name}); }
      };
      inner.addEventListener('pointermove', e => {
        if (window.tried) return; window.tried = true;
        attempt('set-inactive-7', () => inner.setPointerCapture(7));
        attempt('set-mouse-no-buttons', () => inner.setPointerCapture(e.pointerId));
        window.log.push({type: 'has-capture', result: inner.hasPointerCapture(e.pointerId)});
        const detached = document.createElement('div');
        attempt('set-detached', () => detached.setPointerCapture(e.pointerId));
      });
      inner.addEventListener('pointerdown', e => {
        attempt('set-pressed', () => inner.setPointerCapture(e.pointerId));
        window.log.push({type: 'has-capture-now', result: inner.hasPointerCapture(e.pointerId)});
      })""",
}


def center(name):
    x, y, w, h = (BOXES[name][:4] if name in BOXES else
                  BUTTON if name == "button" else FIELD)
    return (x + w // 2, y + h // 2)


# Inputs: ("move", x, y, buttons), ("down"/"up", x, y, button, buttons),
# ("touch", phase, x, y), ("wait",). Coordinates are viewport CSS pixels.
INNER = center("inner")
SIBLING = center("sibling")
OUTER = (30, 30)
BLANK = (380, 280)
SCENARIOS = {
    "hover": [("move", *BLANK, 0), ("move", *OUTER, 0), ("move", *INNER, 0),
              ("move", *SIBLING, 0), ("move", *BLANK, 0)],
    "click": [("move", *INNER, 0), ("down", *INNER, "left", 1), ("up", *INNER, "left", 0)],
    "prevent_pointerdown": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                            ("move", INNER[0] + 5, INNER[1], 1),
                            ("up", INNER[0] + 5, INNER[1], "left", 0),
                            ("move", INNER[0] + 6, INNER[1], 0)],
    "focus_click": [("move", *center("button"), 0), ("down", *center("button"), "left", 1),
                    ("up", *center("button"), "left", 0), ("move", *center("field"), 0),
                    ("down", *center("field"), "left", 1), ("up", *center("field"), "left", 0)],
    "prevent_mousedown": [("move", *center("button"), 0),
                          ("down", *center("button"), "left", 1),
                          ("up", *center("button"), "left", 0)],
    "prevent_pointerdown_focus": [("move", *center("button"), 0),
                                  ("down", *center("button"), "left", 1),
                                  ("up", *center("button"), "left", 0)],
    "chorded": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                ("down", *INNER, "right", 3), ("up", *INNER, "right", 1),
                ("up", *INNER, "left", 0)],
    "right_click": [("move", *INNER, 0), ("down", *INNER, "right", 2),
                    ("up", *INNER, "right", 0)],
    "middle_click": [("move", *INNER, 0), ("down", *INNER, "middle", 4),
                     ("up", *INNER, "middle", 0)],
    "prevent_mouseup": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                        ("up", *INNER, "left", 0)],
    "drag_elsewhere": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                       ("move", *SIBLING, 1), ("up", *SIBLING, "left", 0)],
    "capture": [("move", *INNER, 0), ("down", *INNER, "left", 1), ("move", *SIBLING, 1),
                ("move", SIBLING[0] + 5, SIBLING[1], 1),
                ("up", SIBLING[0] + 5, SIBLING[1], "left", 0),
                ("move", SIBLING[0] + 6, SIBLING[1], 0)],
    "capture_removed": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                        ("move", *SIBLING, 1), ("wait",),
                        ("move", SIBLING[0] + 5, SIBLING[1], 1),
                        ("up", SIBLING[0] + 5, SIBLING[1], "left", 0)],
    "capture_errors": [("move", *INNER, 0), ("down", *INNER, "left", 1),
                       ("up", *INNER, "left", 0)],
    "touch_tap": [("touch", "touchStart", *INNER), ("touch", "touchEnd", *INNER)],
    "touch_move": [("touch", "touchStart", *INNER), ("touch", "touchMove", *SIBLING),
                   ("touch", "touchEnd", *SIBLING)],
    "transformed_offset": [("move", 250, 150, 0), ("down", 250, 150, "left", 1),
                           ("up", 250, 150, "left", 0)],
}


def send(browser, step) -> None:
    kind = step[0]
    if kind == "wait":
        browser.evaluate("new Promise(r => setTimeout(r, 50))", await_promise=True)
        return
    if kind == "touch":
        _, phase, x, y = step
        points = [] if phase == "touchEnd" else [{"x": x, "y": y, "id": 0}]
        browser.client.command("Input.dispatchTouchEvent", {"type": phase, "touchPoints": points})
        return
    if kind == "move":
        _, x, y, buttons = step
        # A move while a button is held names that button, as the platform's
        # does (a Windows WM_MOUSEMOVE with MK_LBUTTON is a left-button move).
        # A move with `button` "none" makes Blink reset mouse capture
        # (`EventHandler::HandleMouseMoveOrLeaveEvent`, crbug.com/527582).
        held = next((name for bit, name in ((1, "left"), (2, "right"), (4, "middle"))
                     if buttons & bit), "none")
        params = {"type": "mouseMoved", "x": x, "y": y, "buttons": buttons, "button": held}
    else:
        _, x, y, button, buttons = step
        params = {"type": "mousePressed" if kind == "down" else "mouseReleased",
                  "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": 1}
    browser.client.command("Input.dispatchMouseEvent", params)


def run(browser, directory: Path, name: str, steps) -> dict:
    path = directory / "page.html"
    path.write_text(page(), encoding="utf-8")
    browser.load(path)
    if any(step[0] == "touch" for step in steps):
        browser.client.command("Emulation.setTouchEmulationEnabled",
                               {"enabled": True, "maxTouchPoints": 1})
    browser.evaluate(LOGGER)
    if name in SETUP:
        browser.evaluate(SETUP[name])
    log = []
    for index, step in enumerate(steps):
        send(browser, step)
        browser.evaluate("new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
                         await_promise=True)
        entries = browser.evaluate("window.log.splice(0)")
        log.append({"input": list(step), "events": entries})
    focused = browser.evaluate("document.activeElement && document.activeElement.id")
    return {"steps": log, "focused": focused}


def capture(chrome: Path, directory: Path) -> dict:
    import windows_chrome

    result = {}
    for name, steps in SCENARIOS.items():
        with windows_chrome.WindowsChrome(chrome, WIDTH, HEIGHT, 1.0) as browser:
            result[name] = run(browser, directory, name, steps)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=FIXTURE)
    parser.add_argument("--only", help="capture one scenario and print it")
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    chrome = args.chrome or windows_chrome.DEFAULT_CHROME
    with tempfile.TemporaryDirectory(prefix="div-pointer-oracle-") as directory:
        if args.only:
            global SCENARIOS
            SCENARIOS = {args.only: SCENARIOS[args.only]}
            print(json.dumps(capture(chrome, Path(directory)), indent=1))
            return 0
        first = capture(chrome, Path(directory))
        second = capture(chrome, Path(directory))
    if first != second:
        raise RuntimeError("captures differ")
    document = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "input": "CDP Input.dispatchMouseEvent and Input.dispatchTouchEvent",
        "viewport": [WIDTH, HEIGHT],
        "scale": 1.0,
        "boxes": {name: list(box[:4]) + [box[4]] for name, box in BOXES.items()},
        "button": list(BUTTON),
        "field": list(FIELD),
        "scenarios": first,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
