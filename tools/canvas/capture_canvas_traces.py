#!/usr/bin/env python3
"""Capture Chromium oracles for canvas traces (RFC 0008, section 7 item 3).

Replays every `traces/*.jsonl` file of a fixture directory in Chromium for
Testing 147.0.7727.50 (`--headless --disable-gpu`, so canvases are software
rasterized) and writes, for the given platform:

- `chromium-<platform>.json`: per trace, its observations (every read,
  return value and exception, in order) and each canvas's size and the
  sha256 of its `getImageData` bytes;
- `chromium-<platform>/<trace>.<canvas>.png`: each canvas's `getImageData`
  bytes as an 8-bit RGBA PNG (non-premultiplied, exactly the bytes).

A canvas listed in the fixture's `cpu-classes.json` differs between CPU
classes, so it is written as `<trace>.<canvas>.<class>.png`, and its JSON
`sha256` maps each class to a hash. `--cpu-fingerprint` (from
`tools/canvas/rcpps_fingerprint.rs`) names this machine's class. An unknown
fingerprint stops the capture. The other classes' files and hashes are kept.
Every other canvas has one oracle for all classes, and the script lists any
whose hash changed from the stored capture.

`div-canvas`'s `chromium_traces` test replays the same files and compares
bytes and observations with `==`.

A trace's `fontFace` lines name fonts in `crates/div-text/fonts`; the page
loads them as `FontFace` objects from the embedded bytes. Text is rasterized
as Div's text oracles are: on Linux with the real-font Fontconfig profile and
Chromium's FreeType typeface path, on Windows after checking the RFC 0012
text raster pin (ClearType on, RGB).

Usage:
  python3 tools/canvas/capture_canvas_traces.py \
      --chrome <path to chrome or chrome.exe> --platform linux|windows \
      --cpu-fingerprint <hex> tests/fidelity/fixtures/canvas-traces-v1
"""

import argparse
import base64
import hashlib
import importlib.util
import json
import os
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FONTS = ROOT / "crates" / "div-text" / "fonts"
CHROMIUM_VERSION = "147.0.7727.50"
# Linux: Chromium's FreeType path for the faces, as the web font oracles.
LINUX_FLAGS = ["--enable-features=FontDataServiceLinux:typeface/Freetype"]
FLAGS = [
    "--headless",
    "--disable-gpu",
    "--no-sandbox",
    "--no-first-run",
    "--no-default-browser-check",
    "--remote-debugging-port=0",
]


def _accountability():
    path = ROOT / "tools" / "accountability" / "run_all_pixel_comparisons.py"
    spec = importlib.util.spec_from_file_location("div_accountability", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The replayer. It mirrors `div_canvas::trace::replay`: the same decoding of
# special numbers, the same observation shapes.
REPLAY_JS = r"""
async (lines) => {
  const SPECIAL = {"NaN": NaN, "Infinity": Infinity, "-Infinity": -Infinity, "-0": -0};
  const METRICS = ["width", "actualBoundingBoxLeft", "actualBoundingBoxRight",
                   "fontBoundingBoxAscent", "fontBoundingBoxDescent",
                   "actualBoundingBoxAscent", "actualBoundingBoxDescent",
                   "hangingBaseline",
                   "alphabeticBaseline", "ideographicBaseline"];
  const num = (v) => (typeof v === "string" && v in SPECIAL) ? SPECIAL[v] : v;
  const enc = (v) => {
    if (typeof v !== "number") return v;
    if (Number.isNaN(v)) return "NaN";
    if (v === Infinity) return "Infinity";
    if (v === -Infinity) return "-Infinity";
    if (Object.is(v, -0)) return "-0";
    return v;
  };
  const arg = (a) => {
    if (Array.isArray(a)) return a.map(arg);
    if (a !== null && typeof a === "object") {
      if ("imageData" in a) return images.get(a.imageData);
      if ("gradient" in a) return gradients.get(a.gradient);
      const p = {};
      for (const k of Object.keys(a)) p[k] = num(a[k]);
      return p;
    }
    return num(a);
  };
  const imageValue = (d) => ({
    width: d.width, height: d.height,
    data: Array.from(d.data, (b) => b.toString(16).padStart(2, "0")).join(""),
  });
  // A gradient read back is listed by the name it was stored under, which
  // also checks that the same object came back.
  const readValue = (v) => {
    if (v instanceof CanvasGradient) {
      for (const [name, g] of gradients) if (g === v) return {gradient: name};
      return {gradient: null};
    }
    return enc(v);
  };
  const contexts = new Map();
  const images = new Map();
  const gradients = new Map();
  const order = [];
  const observations = [];
  // Faces a previous replay added.
  document.fonts.clear();
  for (const e of lines) {
    if ("fontFace" in e) {
      const bytes = Uint8Array.from(atob(e.data), (c) => c.charCodeAt(0));
      const face = new FontFace(e.fontFace, bytes,
                                {weight: e.weight || "400", style: e.style || "normal"});
      await face.load();
      document.fonts.add(face);
      continue;
    }
    if ("imageData" in e) {
      try {
        let image;
        if ("data" in e) {
          const bytes = Uint8ClampedArray.from(e.data.match(/../g) || [],
                                               (h) => parseInt(h, 16));
          image = "height" in e ? new ImageData(bytes, e.width, e.height)
                                : new ImageData(bytes, e.width);
        } else {
          image = new ImageData(e.width, e.height);
        }
        images.set(e.imageData, image);
      } catch (err) {
        observations.push({imageData: e.imageData, throws: err.name});
      }
      continue;
    }
    if ("gradient" in e) {
      try {
        gradients.get(e.gradient).addColorStop(num(e.addColorStop[0]), e.addColorStop[1]);
      } catch (err) {
        observations.push({gradient: e.gradient, call: "addColorStop", throws: err.name});
      }
      continue;
    }
    if ("width" in e) {
      const c = document.createElement("canvas");
      c.width = e.width;
      c.height = e.height;
      contexts.set(e.canvas, c.getContext("2d"));
      order.push(e.canvas);
      continue;
    }
    const ctx = contexts.get(e.canvas);
    if ("call" in e) {
      try {
        const r = ctx[e.call](...(e.args || []).map(arg));
        if (e.call === "getTransform") {
          observations.push({canvas: e.canvas, call: e.call,
                             return: [r.a, r.b, r.c, r.d, r.e, r.f].map(enc)});
        } else if (e.call === "measureText") {
          // TextMetrics keeps its values in prototype getters, which
          // JSON.stringify skips: list the twelve metrics explicitly.
          const m = {};
          for (const k of METRICS) m[k] = enc(r[k]);
          observations.push({canvas: e.canvas, call: e.call, return: m});
        } else if (r instanceof ImageData) {
          observations.push({canvas: e.canvas, call: e.call, return: imageValue(r)});
          if ("store" in e) images.set(e.store, r);
        } else if (r instanceof CanvasGradient) {
          observations.push({canvas: e.canvas, call: e.call, return: "CanvasGradient"});
          if ("store" in e) gradients.set(e.store, r);
        } else if (r !== undefined) {
          observations.push({canvas: e.canvas, call: e.call,
                             return: Array.isArray(r) ? r.map(enc) : enc(r)});
        }
      } catch (err) {
        observations.push({canvas: e.canvas, call: e.call, throws: err.name});
      }
    } else if ("set" in e) {
      ctx[e.set] = arg(e.value);
    } else if ("get" in e) {
      observations.push({canvas: e.canvas, get: e.get, value: readValue(ctx[e.get])});
    }
  }
  const canvases = {};
  for (const id of order) {
    const ctx = contexts.get(id);
    const w = ctx.canvas.width, h = ctx.canvas.height;
    const data = ctx.getImageData(0, 0, w, h).data;
    let s = "";
    for (let i = 0; i < data.length; i += 0x8000) {
      s += String.fromCharCode.apply(null, data.subarray(i, i + 0x8000));
    }
    canvases[id] = {width: w, height: h, rgba: btoa(s)};
  }
  // As text: JSON.stringify prints each double in its shortest round-trip
  // form, while CDP's own value serialization rounds to fewer digits.
  return JSON.stringify({observations, canvases});
}
"""


def png_rgba(width, height, rgba):
    """A minimal 8-bit RGBA PNG (filter 0 on every row)."""
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw.extend(rgba[y * stride:(y + 1) * stride])

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b""))


def start_chrome(acc, chrome, profile_dir, platform):
    extra = LINUX_FLAGS if platform == "linux" else []
    cmd = [chrome, *FLAGS, *extra, f"--user-data-dir={profile_dir}", "about:blank"]
    if platform == "linux":
        env = acc.chrome_environment(str(Path(chrome).parent), use_real_font=True)
    else:
        env = os.environ.copy()
    process = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, start_new_session=(os.name == "posix"))
    try:
        port = acc._wait_for_devtools_endpoint(profile_dir, process, timeout=30)
        client = acc._CdpWebSocket(acc._page_websocket_url(port, timeout=30))
    except Exception:
        acc._stop_chromium_process(process)
        raise
    return process, client


def version(client):
    product = client.command("Browser.getVersion").get("product", "")
    return product.split("/", 1)[-1]


def embed_fonts(lines):
    """Adds each `fontFace` line's file as base64 `data`."""
    out = []
    for line in lines:
        if "fontFace" in line:
            name = line["file"]
            if Path(name).name != name:
                sys.exit(f"fontFace file {name!r} must be a file in {FONTS}")
            line = dict(line, data=base64.b64encode((FONTS / name).read_bytes()).decode("ascii"))
        out.append(line)
    return out


def replay(client, lines):
    expression = f"({REPLAY_JS})({json.dumps(embed_fonts(lines))})"
    result = client.command("Runtime.evaluate", {
        "expression": expression, "returnByValue": True, "awaitPromise": True,
    })
    if "exceptionDetails" in result:
        raise RuntimeError(json.dumps(result["exceptionDetails"])[:2000])
    return json.loads(result["result"]["value"])


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--chrome", required=True)
    parser.add_argument("--platform", required=True, choices=["linux", "windows"])
    parser.add_argument("--cpu-fingerprint", required=True,
                        help="this machine's RCPPS fingerprint, from tools/canvas/rcpps_fingerprint.rs")
    parser.add_argument("fixture", type=Path)
    args = parser.parse_args()

    cpu = json.loads((args.fixture / "cpu-classes.json").read_text(encoding="utf-8"))
    cpu_class = cpu["classes"].get(args.cpu_fingerprint.lower())
    if cpu_class is None:
        sys.exit(f"unknown RCPPS fingerprint {args.cpu_fingerprint}: no CPU class in cpu-classes.json")
    per_class = set(cpu["canvases"])
    other_classes = set(cpu["classes"].values()) - {cpu_class}
    target = args.fixture / f"chromium-{args.platform}.json"
    previous = json.loads(target.read_text(encoding="utf-8")) if target.exists() else {"traces": {}}

    traces = sorted((args.fixture / "traces").glob("*.jsonl"))
    if not traces:
        sys.exit(f"no traces under {args.fixture / 'traces'}")
    out_dir = args.fixture / f"chromium-{args.platform}"
    out_dir.mkdir(exist_ok=True)
    kept = {f"{key}.{other}.png" for key in per_class for other in other_classes}
    for old in out_dir.glob("*.png"):
        if old.name not in kept:
            old.unlink()

    acc = _accountability()
    flags = FLAGS[:2] + (LINUX_FLAGS if args.platform == "linux" else [])
    if args.platform == "windows":
        sys.path.insert(0, str(ROOT / "tools" / "qualification"))
        import windows_chrome

        # ClearType on, RGB, no tuning: the settings Div's Windows text
        # raster reproduces (RFC 0012).
        windows_chrome.check_text_raster_pin()
    oracle = {"chromium": None, "platform": args.platform, "flags": flags, "traces": {}}
    changed = []
    # Chrome can still hold its profile when it is deleted on Windows.
    with tempfile.TemporaryDirectory(prefix="div-canvas-chrome-", ignore_cleanup_errors=True) as profile:
        process, client = start_chrome(acc, args.chrome, profile, args.platform)
        try:
            oracle["chromium"] = version(client)
            if oracle["chromium"] != CHROMIUM_VERSION:
                sys.exit(f"expected Chromium {CHROMIUM_VERSION}, got {oracle['chromium']}")
            for trace in traces:
                lines = [json.loads(l) for l in trace.read_text(encoding="utf-8").splitlines() if l.strip()]
                # Two replays in fresh contexts must agree before one is kept.
                first = replay(client, lines)
                second = replay(client, lines)
                if first != second:
                    sys.exit(f"{trace.name}: two Chromium replays disagree")
                entry = {"observations": first["observations"], "canvases": {}}
                stored = previous["traces"].get(trace.stem, {}).get("canvases", {})
                for cid, canvas in first["canvases"].items():
                    rgba = base64.b64decode(canvas["rgba"])
                    assert len(rgba) == canvas["width"] * canvas["height"] * 4
                    digest = hashlib.sha256(rgba).hexdigest()
                    key = f"{trace.stem}.{cid}"
                    old = stored.get(cid, {}).get("sha256")
                    if key in per_class:
                        hashes = {k: v for k, v in (old or {}).items() if k in other_classes}
                        hashes[cpu_class] = digest
                        name = f"{key}.{cpu_class}.png"
                    else:
                        if old is not None and old != digest:
                            changed.append(key)
                        hashes = digest
                        name = f"{key}.png"
                    entry["canvases"][cid] = {
                        "width": canvas["width"], "height": canvas["height"],
                        "sha256": hashes,
                    }
                    (out_dir / name).write_bytes(
                        png_rgba(canvas["width"], canvas["height"], rgba))
                oracle["traces"][trace.stem] = entry
                print(f"{trace.stem}: {len(entry['canvases'])} canvas(es), "
                      f"{len(entry['observations'])} observation(s)", flush=True)
        finally:
            acc._close_chromium_client(client)
            acc._stop_chromium_process(process)
    target.write_text(json.dumps(oracle, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {target} ({cpu_class} class, fingerprint {args.cpu_fingerprint.lower()})")
    for key in changed:
        print(f"changed from the stored capture: {key}")


if __name__ == "__main__":
    main()
