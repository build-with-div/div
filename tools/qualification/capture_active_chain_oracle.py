"""Record which elements pinned Chromium 147 matches with `:active`.

Real mouse input goes through the DevTools protocol
(`Input.dispatchMouseEvent`), so Chromium's own press handling sets the
active chain. After each step the page reports the ids of the elements
that match `:active`. The native test `openui/tests/active_chain.rs` builds
the same tree through the public API and checks `Element::is_active`
against the JSON this script writes.

`--removal` records the scenarios that remove or hide the pressed element
or one of its ancestors while the button is held, each on a fresh page.
Every step waits two frames before it reads the state: right after a
removal Blink's ancestors briefly match nothing, and its next frame gives
the chain to the parent. The scenarios run twice and must agree.

Windows only: it uses the pinned Windows Chrome-for-Testing.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "tests/fidelity/fixtures/active-chain-v1/chromium-windows.json"
REMOVAL_OUTPUT = ROOT / "tests/fidelity/fixtures/active-chain-removal-v1/chromium-windows.json"
VIEWPORT = 200

# Every element has an id so the page can report it. The geometry is
# absolute so the press points below are exact. The native test builds the
# same boxes; keep the two in step.
PAGE = """<!DOCTYPE html><html id="html"><head><style>
html, body { margin: 0 }
div, button, span { position: absolute; box-sizing: border-box; margin: 0;
  padding: 0; border: 0 }
</style></head><body id="body">
<div id="outer" style="left:0;top:0;width:200px;height:100px">
  <div id="middle" style="left:10px;top:10px;width:100px;height:80px">
    <div id="inner" style="left:10px;top:10px;width:40px;height:40px"></div>
  </div>
  <div id="sibling" style="left:130px;top:10px;width:40px;height:40px"></div>
</div>
<div id="controls" style="left:0;top:100px;width:200px;height:100px">
  <button id="enabled" style="left:10px;top:10px;width:60px;height:30px"></button>
  <button id="disabled" disabled style="left:100px;top:10px;width:60px;height:30px"></button>
</div>
</body></html>"""

# (name, [(event, x, y)]) where event is press, move or release.
STEPS = [
    ("press inner", [("press", 30, 30)]),
    ("move to sibling while pressed", [("move", 150, 30)]),
    ("release over sibling", [("release", 150, 30)]),
    ("press enabled button", [("move", 40, 125), ("press", 40, 125)]),
    ("release enabled button", [("release", 40, 125)]),
    ("press disabled button", [("move", 130, 125), ("press", 130, 125)]),
    ("release disabled button", [("release", 130, 125)]),
]

# (name, [step]) on a fresh page. A step is (press | move | release, x, y),
# with the button held during a move after a press, or ("script", js),
# which may set `detached` to the element it takes out.
REMOVAL_SCENARIOS = [
    ("remove the pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('inner'); detached.remove()"),
        ("move", 60, 60),
        ("release", 60, 60),
    ]),
    ("remove an ancestor of the pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('middle'); detached.remove()"),
        ("release", 30, 30),
    ]),
    ("reinsert the removed pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('inner'); detached.remove()"),
        ("script", "document.getElementById('sibling').append(detached)"),
        ("release", 30, 30),
    ]),
    ("move the pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('inner');"
                   " document.getElementById('sibling').append(detached)"),
        ("release", 30, 30),
    ]),
    ("press again after the removal", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('inner'); detached.remove()"),
        ("release", 30, 30),
        ("press", 30, 30),
        ("release", 30, 30),
    ]),
    ("remove the pressed button", [
        ("move", 40, 125),
        ("press", 40, 125),
        ("script", "detached = document.getElementById('enabled'); detached.remove()"),
        ("release", 40, 125),
    ]),
    ("hide the pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('inner'); detached.style.display = 'none'"),
        ("release", 30, 30),
    ]),
    ("hide an ancestor of the pressed element", [
        ("press", 30, 30),
        ("script", "detached = document.getElementById('middle'); detached.style.display = 'none'"),
        ("release", 30, 30),
    ]),
]

FRAMES = "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))"

# The removal pages log each pointer and mouse event's target.
EVENT_LOG = (
    "window.detached = null; window.events = [];"
    "for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click'])"
    " document.addEventListener(type, e => events.push("
    "  type + ' ' + (e.target.id || e.target.nodeName.toLowerCase())), true);"
)

ACTIVE = (
    "Array.from(document.querySelectorAll('[id]'))"
    ".filter(e => e.matches(':active')).map(e => e.id)"
)


def mouse(browser, event: str, x: int, y: int) -> None:
    kind = {"press": "mousePressed", "move": "mouseMoved", "release": "mouseReleased"}[event]
    params = {"type": kind, "x": x, "y": y, "button": "left" if event != "move" else "none"}
    if event == "press":
        params.update(clickCount=1, buttons=1)
    elif event == "release":
        params.update(clickCount=1, buttons=0)
    else:
        params["buttons"] = 0
    browser.client.command("Input.dispatchMouseEvent", params)


def removal_scenarios(browser, page: Path) -> list:
    scenarios = []
    for name, steps in REMOVAL_SCENARIOS:
        browser.load(page)
        browser.evaluate(EVENT_LOG)
        browser.evaluate(FRAMES, await_promise=True)
        results = []
        pressed = False
        for step in steps:
            if step[0] == "script":
                browser.evaluate(step[1])
            elif step[0] == "move" and pressed:
                browser.client.command("Input.dispatchMouseEvent", {
                    "type": "mouseMoved", "x": step[1], "y": step[2],
                    "button": "left", "buttons": 1,
                })
            else:
                mouse(browser, *step)
                pressed = step[0] == "press"
            browser.evaluate(FRAMES, await_promise=True)
            results.append({
                "step": list(step),
                "active": browser.evaluate(ACTIVE),
                "detached_active": browser.evaluate(
                    "detached ? detached.matches(':active') : null"),
                "events": browser.evaluate("events.splice(0)"),
            })
        scenarios.append({"scenario": name, "steps": results})
    return scenarios


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--removal", action="store_true",
                        help="record the removal scenarios instead")
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import windows_chrome

    if args.removal:
        output = args.output or REMOVAL_OUTPUT
        with tempfile.TemporaryDirectory(prefix="openui-active-oracle-") as directory:
            page = Path(directory) / "active.html"
            page.write_text(PAGE, encoding="utf-8")
            with windows_chrome.WindowsChrome(
                args.chrome or windows_chrome.DEFAULT_CHROME, VIEWPORT, VIEWPORT, 1.0
            ) as browser:
                first = removal_scenarios(browser, page)
                second = removal_scenarios(browser, page)
        if first != second:
            raise SystemExit("the two removal captures differ")
        result = {
            "chromium": windows_chrome.CHROMIUM_VERSION,
            "platform": "windows",
            "viewport": [VIEWPORT, VIEWPORT, 1.0],
            "scenarios": first,
        }
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
        print(json.dumps(first, indent=1))
        return 0

    results = []
    with tempfile.TemporaryDirectory(prefix="openui-active-oracle-") as directory:
        page = Path(directory) / "active.html"
        page.write_text(PAGE, encoding="utf-8")
        with windows_chrome.WindowsChrome(
            args.chrome or windows_chrome.DEFAULT_CHROME, VIEWPORT, VIEWPORT, 1.0
        ) as browser:
            browser.load(page)
            pressed = False
            for name, events in STEPS:
                for event, x, y in events:
                    if event == "move" and pressed:
                        # A move with the left button held.
                        browser.client.command("Input.dispatchMouseEvent", {
                            "type": "mouseMoved", "x": x, "y": y,
                            "button": "left", "buttons": 1,
                        })
                    else:
                        mouse(browser, event, x, y)
                    pressed = event == "press" or (pressed and event == "move")
                results.append({
                    "step": name,
                    "events": [[event, x, y] for event, x, y in events],
                    "active": browser.evaluate(ACTIVE),
                })
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "viewport": [VIEWPORT, VIEWPORT, 1.0],
        "steps": results,
    }
    output = args.output or OUTPUT
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(results, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
