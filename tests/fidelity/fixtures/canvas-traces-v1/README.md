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
  exact bytes, non-premultiplied.

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

## Regenerating

```bash
python3 tools/canvas/capture_canvas_traces.py --platform linux \
    --chrome <chrome-linux64>/chrome tests/fidelity/fixtures/canvas-traces-v1
python tools/canvas/capture_canvas_traces.py --platform windows \
    --chrome <chrome-win64>/chrome.exe tests/fidelity/fixtures/canvas-traces-v1
```

A new trace needs both captures before the test can pass.
