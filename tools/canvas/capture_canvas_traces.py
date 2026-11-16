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

`div-canvas`'s `chromium_traces` test replays the same files and compares
bytes and observations with `==`.

Usage:
  python3 tools/canvas/capture_canvas_traces.py \
      --chrome <path to chrome or chrome.exe> --platform linux|windows \
      tests/fidelity/fixtures/canvas-traces-v1
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
CHROMIUM_VERSION = "147.0.7727.50"
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
(lines) => {
  const SPECIAL = {"NaN": NaN, "Infinity": Infinity, "-Infinity": -Infinity, "-0": -0};
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
      const p = {};
      for (const k of Object.keys(a)) p[k] = num(a[k]);
      return p;
    }
    return num(a);
  };
  const contexts = new Map();
  const order = [];
  const observations = [];
  for (const e of lines) {
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
        } else if (r !== undefined) {
          observations.push({canvas: e.canvas, call: e.call,
                             return: Array.isArray(r) ? r.map(enc) : enc(r)});
        }
      } catch (err) {
        observations.push({canvas: e.canvas, call: e.call, throws: err.name});
      }
    } else if ("set" in e) {
      ctx[e.set] = num(e.value);
    } else if ("get" in e) {
      observations.push({canvas: e.canvas, get: e.get, value: enc(ctx[e.get])});
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


def start_chrome(acc, chrome, profile_dir):
    cmd = [chrome, *FLAGS, f"--user-data-dir={profile_dir}", "about:blank"]
    env = os.environ.copy()
    env["LD_LIBRARY_PATH"] = str(Path(chrome).parent) + ":" + env.get("LD_LIBRARY_PATH", "")
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


def replay(client, lines):
    expression = f"({REPLAY_JS})({json.dumps(lines)})"
    result = client.command("Runtime.evaluate", {
        "expression": expression, "returnByValue": True, "awaitPromise": False,
    })
    if "exceptionDetails" in result:
        raise RuntimeError(json.dumps(result["exceptionDetails"])[:2000])
    return json.loads(result["result"]["value"])


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--chrome", required=True)
    parser.add_argument("--platform", required=True, choices=["linux", "windows"])
    parser.add_argument("fixture", type=Path)
    args = parser.parse_args()

    traces = sorted((args.fixture / "traces").glob("*.jsonl"))
    if not traces:
        sys.exit(f"no traces under {args.fixture / 'traces'}")
    out_dir = args.fixture / f"chromium-{args.platform}"
    out_dir.mkdir(exist_ok=True)
    for old in out_dir.glob("*.png"):
        old.unlink()

    acc = _accountability()
    oracle = {"chromium": None, "platform": args.platform, "flags": FLAGS[:2], "traces": {}}
    with tempfile.TemporaryDirectory(prefix="div-canvas-chrome-") as profile:
        process, client = start_chrome(acc, args.chrome, profile)
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
                for cid, canvas in first["canvases"].items():
                    rgba = base64.b64decode(canvas["rgba"])
                    assert len(rgba) == canvas["width"] * canvas["height"] * 4
                    entry["canvases"][cid] = {
                        "width": canvas["width"], "height": canvas["height"],
                        "sha256": hashlib.sha256(rgba).hexdigest(),
                    }
                    (out_dir / f"{trace.stem}.{cid}.png").write_bytes(
                        png_rgba(canvas["width"], canvas["height"], rgba))
                oracle["traces"][trace.stem] = entry
                print(f"{trace.stem}: {len(entry['canvases'])} canvas(es), "
                      f"{len(entry['observations'])} observation(s)", flush=True)
        finally:
            acc._close_chromium_client(client)
            acc._stop_chromium_process(process)
    target = args.fixture / f"chromium-{args.platform}.json"
    target.write_text(json.dumps(oracle, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {target}")


if __name__ == "__main__":
    main()
