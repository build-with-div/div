"""Record what pinned Chromium 147's `matchMedia()` reports.

`tests/fidelity/fixtures/match-media-v1/queries.json` lists media query
texts: every `@media` prelude of `media-queries-v1` and edge cases of
parsing and serialization. For each environment of `media-queries-v1` (a
viewport size, a device scale factor and the color scheme and
reduced-motion preferences), Chromium reports `matchMedia(query).media`
and `.matches` for every text.

It also runs scripted change sequences. A sequence creates lists, changes
an input over CDP, reads `matches` and runs frames, and records the
`change` events each list receives. A `requestAnimationFrame` counter
proves that no frame ran between a change and a read that claims to come
before the next frame; a sequence where one did is run again.

The native test `crates/divui/tests/match_media_oracle.rs` replays both
parts and requires the same values, except where the test names a Div gap.

A device scale factor other than 1 is real zoom (`--force-device-scale-factor`,
RFC 0010). The preferences use CDP's `Emulation.setEmulatedMedia`, and the
sequences' viewport changes use `Emulation.setDeviceMetricsOverride`, as
the capture's viewport size does. Their device scale factor changes use the
same override, so they are CDP-emulated: real zoom cannot change while a page
runs. They pin when lists are evaluated, not how a scaled page renders. Pointer and hover features report the
capturing machine's input devices; the recorded machine has a mouse and no
touch screen.

Windows only: it uses the pinned Windows Chrome-for-Testing.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CASES = ROOT / "tests/fidelity/fixtures/match-media-v1"
OUTPUT = CASES / "chromium-windows.json"
sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture_media_oracle import ENVIRONMENTS  # noqa: E402

PAGE = "<!DOCTYPE html><html><head></head><body></body></html>"

SETUP = """
(() => {
  window.__frames = 0;
  (function tick() { window.__frames++; requestAnimationFrame(tick); })();
  window.__lists = [];
  window.__events = [];
  window.__make = query => {
    const list = matchMedia(query);
    const index = window.__lists.length;
    window.__lists.push(list);
    list.addEventListener('change', event =>
      window.__events.push([index, event.matches, event.media]));
    return [list.media, list.matches];
  };
  window.__frame = () => new Promise(resolve =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  return true;
})()
"""

READ = ("({frames: window.__frames, matches: window.__lists.map(list => list.matches),"
        " events: window.__events.length})")

# Steps: ["make", query], ["scheme", "light" | "dark"], ["motion", bool],
# ["width", px], ["read"], ["frame"]. A sequence starts in the "boundary"
# environment (600 x 400 at 1x, light, no reduced motion) after one frame.
# Every "read" must see the frame count the last "frame" (or the start)
# left, so it describes the state before the next frame.
SEQUENCES = {
    "change-then-frame": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["frame"],
        ["scheme", "light"], ["frame"],
    ],
    "read-before-frame": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["read"], ["frame"],
        ["read"], ["scheme", "light"], ["read"], ["frame"],
    ],
    "flip-and-back": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["scheme", "light"],
        ["read"], ["frame"],
    ],
    "flip-read-and-back": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["read"],
        ["scheme", "light"], ["read"], ["frame"],
    ],
    "creation-order": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(prefers-color-scheme: light)"],
        ["make", "(min-width: 1px)"], ["make", "(prefers-color-scheme: dark)"],
        ["make", "not (prefers-color-scheme: dark)"], ["scheme", "dark"], ["frame"],
        ["scheme", "light"], ["frame"],
    ],
    "created-after-change": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"],
        ["make", "(prefers-color-scheme: dark)"], ["make", "(prefers-color-scheme: light)"],
        ["read"], ["frame"], ["scheme", "light"], ["frame"],
    ],
    "created-before-frame-then-reverted": [
        ["scheme", "dark"], ["make", "(prefers-color-scheme: dark)"], ["scheme", "light"],
        ["read"], ["frame"],
    ],
    "reduced-motion": [
        ["make", "(prefers-reduced-motion: reduce)"], ["make", "(prefers-reduced-motion)"],
        ["motion", True], ["read"], ["frame"], ["motion", False], ["frame"],
    ],
    "viewport-width": [
        ["make", "(min-width: 700px)"], ["make", "(orientation: portrait)"],
        ["make", "(max-width: 599.98px)"], ["width", 800], ["read"], ["frame"],
        ["width", 300], ["frame"], ["width", 600], ["read"], ["frame"],
    ],
    "motion-flip-and-back": [
        ["make", "(prefers-reduced-motion: reduce)"], ["motion", True], ["motion", False],
        ["read"], ["frame"],
    ],
    "motion-flip-read-and-back": [
        ["make", "(prefers-reduced-motion: reduce)"], ["motion", True], ["read"],
        ["motion", False], ["read"], ["frame"],
    ],
    "width-flip-and-back": [
        ["make", "(min-width: 700px)"], ["width", 800], ["width", 600], ["read"], ["frame"],
    ],
    "width-flip-read-and-back": [
        ["make", "(min-width: 700px)"], ["width", 800], ["read"], ["width", 600], ["read"],
        ["frame"],
    ],
    "scheme-read-then-revert": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["read"], ["frame"],
        ["scheme", "light"], ["frame"],
    ],
    "scheme-and-motion-together": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(prefers-reduced-motion: reduce)"],
        ["media", "dark", True], ["read"], ["frame"],
    ],
    "scheme-then-width-read": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(min-width: 700px)"],
        ["scheme", "dark"], ["width", 800], ["read"], ["frame"],
    ],
    "width-then-scheme-read": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(min-width: 700px)"],
        ["width", 800], ["scheme", "dark"], ["read"], ["frame"],
    ],
    "unaffected-list-read": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(min-width: 700px)"],
        ["scheme", "dark"], ["read-one", 1], ["frame"],
    ],
    "same-query-read-one": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(prefers-color-scheme: dark)"],
        ["scheme", "dark"], ["read-one", 0], ["frame"],
    ],
    "created-after-width-change": [
        ["make", "(min-width: 700px)"], ["width", 800], ["make", "(min-width: 700px)"],
        ["frame"], ["width", 600], ["frame"],
    ],
    "scale-change": [
        ["make", "(min-resolution: 2dppx)"], ["make", "(-webkit-min-device-pixel-ratio: 1.5)"],
        ["make", "(resolution: 1dppx)"], ["scale", 2], ["read"], ["frame"],
        ["scale", 1], ["frame"],
    ],
    "scale-read-and-back": [
        ["make", "(min-resolution: 2dppx)"], ["scale", 2], ["read"], ["scale", 1], ["read"],
        ["frame"],
    ],
    "flip-and-back-no-read": [
        ["make", "(prefers-color-scheme: dark)"], ["scheme", "dark"], ["scheme", "light"],
        ["frame"],
    ],
    "two-changes-one-frame": [
        ["make", "(prefers-color-scheme: dark)"], ["make", "(min-width: 700px)"],
        ["scheme", "dark"], ["width", 800], ["read"], ["frame"],
    ],
}


def emulate_media(browser, scheme: str, reduced: bool) -> None:
    browser.client.command("Emulation.setEmulatedMedia", {"features": [
        {"name": "prefers-color-scheme", "value": scheme},
        {"name": "prefers-reduced-motion", "value": "reduce" if reduced else "no-preference"},
    ]})


def set_metrics(browser, width: int, scale: float | None) -> None:
    """The viewport width and, unless None, an emulated device scale factor."""
    override = dict(browser._viewport_override)
    override["width"] = width
    if scale is not None:
        override["deviceScaleFactor"] = scale
    browser.client.command("Emulation.setDeviceMetricsOverride", override)


def run_sequence(browser, page_path: Path, steps: list) -> dict | None:
    """One run of a sequence, or None when a frame ran before a read."""
    scheme, reduced = "light", False
    width, scale = browser.width, None
    emulate_media(browser, scheme, reduced)
    set_metrics(browser, width, scale)
    browser.load(page_path)
    browser.evaluate(SETUP)
    browser.evaluate("window.__frame()", await_promise=True)
    mark = browser.evaluate("window.__frames")
    out = []
    for step in steps:
        kind = step[0]
        if kind == "make":
            media, matches = browser.evaluate(f"window.__make({json.dumps(step[1])})")
            out.append({"make": step[1], "media": media, "matches": matches})
        elif kind == "scheme":
            scheme = step[1]
            emulate_media(browser, scheme, reduced)
            out.append({"scheme": scheme})
        elif kind == "motion":
            reduced = step[1]
            emulate_media(browser, scheme, reduced)
            out.append({"reduced_motion": reduced})
        elif kind == "media":
            scheme, reduced = step[1], step[2]
            emulate_media(browser, scheme, reduced)
            out.append({"scheme": scheme, "reduced_motion": reduced})
        elif kind == "read-one":
            read = browser.evaluate(f"({{frames: window.__frames, matches: window.__lists[{step[1]}].matches}})")
            if read["frames"] != mark:
                return None
            out.append({"read_one": step[1], "matches": read["matches"]})
        elif kind == "width":
            width = step[1]
            set_metrics(browser, width, scale)
            out.append({"width": width})
        elif kind == "scale":
            scale = step[1]
            set_metrics(browser, width, scale)
            out.append({"scale": scale})
        elif kind == "read":
            read = browser.evaluate(READ)
            if read["frames"] != mark:
                return None
            out.append({"read": read["matches"], "events_so_far": read["events"]})
        elif kind == "frame":
            before = browser.evaluate("window.__events.length")
            browser.evaluate("window.__frame()", await_promise=True)
            events = browser.evaluate("window.__events")[before:]
            mark = browser.evaluate("window.__frames")
            out.append({"frame": [{"list": i, "matches": m, "media": media}
                                  for i, m, media in events]})
        else:
            raise ValueError(step)
    return {"steps": out}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--attempts", type=int, default=20)
    args = parser.parse_args()
    import windows_chrome

    queries = json.loads((CASES / "queries.json").read_text(encoding="utf-8"))
    script = ("(() => JSON.stringify(" + json.dumps(queries)
              + ".map(q => { const l = matchMedia(q); return [l.media, l.matches]; })))()")
    media = None
    environments = []
    sequences = {}
    with tempfile.TemporaryDirectory(prefix="div-match-media-oracle-") as directory:
        page_path = Path(directory) / "page.html"
        page_path.write_text(PAGE, encoding="utf-8")
        for name, width, height, scale, scheme, reduced in ENVIRONMENTS:
            with windows_chrome.WindowsChrome(
                args.chrome or windows_chrome.DEFAULT_CHROME, width, height, scale
            ) as browser:
                emulate_media(browser, scheme, reduced)
                browser.load(page_path)
                values = json.loads(browser.evaluate(script))
            texts = [text for text, _ in values]
            if media is None:
                media = texts
            elif media != texts:
                raise SystemExit(f"{name}: serialization differs between environments")
            environments.append({
                "name": name, "width": width, "height": height, "scale": scale,
                "color_scheme": scheme, "reduced_motion": reduced,
                "matches": "".join("1" if matches else "0" for _, matches in values),
            })
        boundary = next(e for e in ENVIRONMENTS if e[0] == "boundary")
        with windows_chrome.WindowsChrome(
            args.chrome or windows_chrome.DEFAULT_CHROME, *boundary[1:4]
        ) as browser:
            for name, steps in SEQUENCES.items():
                for attempt in range(1, args.attempts + 1):
                    result = run_sequence(browser, page_path, steps)
                    if result is not None:
                        result["attempts"] = attempt
                        sequences[name] = result
                        break
                else:
                    raise SystemExit(f"{name}: a frame ran before a read in all "
                                     f"{args.attempts} attempts")
    result = {
        "chromium": windows_chrome.CHROMIUM_VERSION,
        "platform": "windows",
        "queries": [{"query": q, "media": m} for q, m in zip(queries, media)],
        "environments": environments,
        "sequences": sequences,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n",
                           encoding="utf-8", newline="\n")
    print(f"{len(queries)} queries, {len(environments)} environments, "
          f"{len(sequences)} sequences")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
