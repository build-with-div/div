# canvas-traces-v1

Canvas 2D call traces and their Chromium oracles (RFC 0008, section 7
items 3 and 4). `crates/div-canvas/tests/chromium_traces.rs` replays every
trace with `div_canvas::trace::replay` and compares the result with the
capture for the operating system it runs on:

- every observation (attribute reads, return values, exceptions) must be
  equal, numbers compared as exact doubles;
- every canvas's `getImageData` bytes must be equal, byte for byte.

## Files

- `traces/*.jsonl`: the traces. The format is documented in
  `crates/div-canvas/src/trace.rs`: one JSON object per line, creating a
  canvas, calling a method, setting or getting an attribute, or (text
  traces) loading a font face. A `fontFace` line names a file in
  `crates/div-text/fonts`, which both Chromium (`FontFace` from the bytes)
  and Div load as a web font, so text does not depend on installed fonts.
- `chromium-linux.json`, `chromium-windows.json`: per trace, the
  observations and each canvas's size and sha256 of its RGBA bytes.
- `chromium-linux/`, `chromium-windows/`: each canvas's `getImageData`
  bytes as an 8-bit RGBA PNG named `<trace>.<canvas>.png`. These are the
  exact bytes, non-premultiplied. A canvas listed in `cpu-classes.json` has
  one file per CPU class instead, `<trace>.<canvas>.<class>.png`, and its
  JSON `sha256` maps each class to a hash.
- `cpu-classes.json`: the canvases whose Chromium pixels differ by CPU class,
  and each class's RCPPS fingerprint.

## Oracle

Chromium for Testing 147.0.7727.50 with `--headless --disable-gpu`, so
canvases are rasterized in software by Skia, as in Div:

- Linux: `chrome-linux64` from
  `https://storage.googleapis.com/chrome-for-testing-public/147.0.7727.50/linux64/chrome-linux64.zip`,
  run in WSL 2 (Ubuntu) on 15 November 2026;
- Windows: `chrome-win64` from
  `https://storage.googleapis.com/chrome-for-testing-public/147.0.7727.50/win64/chrome-win64.zip`,
  run on Windows 11 on 15 November 2026.

The text traces were captured on 21 November 2026 with the same builds:

- Linux: with `--enable-features=FontDataServiceLinux:typeface/Freetype`
  and the real-font Fontconfig profile of the page census
  (`tools/accountability/data/fonts/sp16_real_font.conf`), as the page
  census runs Chrome;
- Windows: after `tools/qualification/windows_chrome.py` checks the text
  raster pin of RFC 0012 (ClearType on, RGB order).

The capture replays each trace twice in fresh canvases and keeps it only if
the two replays agree. The Linux and Windows captures of the core traces are
identical (every bitmap hash and every observation). The text traces are
not: every attribute read agrees, but text pixels differ on five of the
seven traces and some `measureText` bounds differ by one pixel, so each
platform is compared with its own capture.

The four readback traces (`readback-get`, `readback-put`,
`readback-roundtrip`, `image-data-create`) were added on 22 November 2026
with the same builds and flags. That capture left every earlier trace's
entry unchanged on both platforms.

The nine gradient traces (`gradient-*`) were added on 22 November 2026 with
the same builds and flags, and again every earlier trace's entry stayed
unchanged on both platforms. Of their 102 canvases, the 99 without text are
identical on Linux and Windows. The three in `gradient-text` differ, as other
text does.

## CPU classes

Pinned Chromium's own pixels differ between Intel and AMD CPUs on three
`gradient-composite` canvases: `color-dodge`, `hue` and `saturation`, 7
pixels, each by one level in one channel. This is a real variation in
Chromium by CPU vendor, not a Div difference. Div matches the same machine's
Chromium on both. The likely mechanism is Skia's unrefined reciprocal
estimate (RCPPS, `rcp_fast` in `SkRasterPipeline_opts.h`), which these
blend modes use and whose results split exactly by vendor. That mechanism is
strongly supported but was not isolated.

So exact means byte-identical to pinned Chromium on the same CPU class.
`tools/canvas/rcpps_fingerprint.rs` computes the class's fingerprint:
FNV-1a 64 over RCPPS of every float in [1, 2). The test selects the oracle
for this machine's class, and an unknown fingerprint fails with its value.
There is no skip, nearest match, fallback or tolerance. Every other canvas
has a single oracle, which must match on both classes.

| Class | Fingerprint | Measured on |
| --- | --- | --- |
| `intel` | `5fc0454d9f4c2325` | Core i9-12900KF, Xeon 6975P-C, Xeon 6973P-C, Xeon 8573C, Xeon 8370C |
| `amd` | `f7ba415b37cf2325` | EPYC 7763, EPYC 9V45, EPYC 9V74 |

The AMD oracles were captured on GitHub Actions runners (EPYC 9V74 on Linux,
EPYC 9V74 on Windows) on 23 November 2026. The Intel oracles are the local
captures above. Four CI Intel captures on Linux matched them exactly. **The
Windows Intel oracle is a local capture only**: no CI Windows Intel runner
reached capture in 24 jobs, so it was recaptured locally on 24 November 2026,
headless, and matched the stored bytes exactly. The receipts are in
`div-fidelity-data` at `archive/docs/progress/evidence/canvas-cpu-classes-v1.json`.

## Regenerating

```bash
rustc --edition 2021 -O tools/canvas/rcpps_fingerprint.rs -o out/rcpps_fingerprint
python3 tools/canvas/capture_canvas_traces.py --platform linux \
    --chrome <chrome-linux64>/chrome --cpu-fingerprint "$(out/rcpps_fingerprint)" \
    tests/fidelity/fixtures/canvas-traces-v1
python tools/canvas/capture_canvas_traces.py --platform windows \
    --chrome <chrome-win64>/chrome.exe --cpu-fingerprint <fingerprint> \
    tests/fidelity/fixtures/canvas-traces-v1
```

A new trace needs both captures before the test can pass. A capture writes
only its own class's files for the canvases in `cpu-classes.json`, and lists
any other canvas whose hash changed from the stored capture.
