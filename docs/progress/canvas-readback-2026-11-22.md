# Canvas 2D readback, 22 November 2026

RFC 0008 §5, readback: `div-canvas` gains `ImageData`, `createImageData`,
`getImageData` and `putImageData`. Each call is compared exactly with
Chromium 147.0.7727.50 by replaying call traces, on Windows and on Linux,
each against its own capture. It needs no `<canvas>` element and changes no
crate other than `div-canvas`.

## What the crate does

- **`ImageData`:** `new ImageData(w, h)` and `new ImageData(data, w, h?)`,
  with `ValidateAndCreate`'s checks in Blink's order. A size that overflows
  is an `IndexSizeError` from a constructor and a `RangeError` from a 2D
  context, as in Blink.
- **`createImageData`:** from a size (absolute values) or another
  `ImageData`, always transparent black.
- **`getImageData`:** Blink's integer overflow checks, negative sizes, and
  transparent black outside the canvas. Pixels come from Skia's `readPixels`
  into unpremultiplied RGBA8888. The transform, clip and other state do not
  apply.
- **`putImageData`:** with or without a dirty rectangle, including Blink's
  negative-size rule (a negative size with a negative origin writes nothing).
  Pixels go through Skia's `writePixels`, as in Blink's unaccelerated canvas,
  so they replace the canvas's, and the premultiplied alpha round trip is
  Skia's.
- **Origin-clean:** `getImageData` checks the flag and returns
  `SecurityError` when it is clear, in Blink's order. Nothing clears it until
  H1 lands (RFC 0008 §5).

`trace` gains `imageData` lines, a `store` key on calls that return an
`ImageData`, and `{"imageData": name}` arguments. A returned `ImageData` is
observed as its size and its bytes in hex.

## Sources

Each file's licence header was checked alone before its body was read. No
fetch was needed, and no file was quarantined.

| Blink file | Licence | Ported to |
| --- | --- | --- |
| `modules/canvas/canvas2d/base_rendering_context_2d.cc` (readback) | Chromium BSD | `src/context/readback.rs` |
| `core/html/canvas/image_data.{h,cc}` | Apple BSD | `src/image_data.rs` |

Read for facts only, nothing ported: `base_rendering_context_2d.idl`,
`image_data.idl`, `CanvasRenderingContext2D::WritePixels` and
`CanvasResourceProvider::UnacceleratedWritePixels`.

## Verification

Four traces join the corpus in
`tests/fidelity/fixtures/canvas-traces-v1/traces`:

- `readback-get`: rectangles, signs, canvas edges, state that must not apply,
  and ten error cases;
- `readback-put`: placement, eleven dirty rectangles, and state that must not
  apply;
- `readback-roundtrip`: all 256 alphas through two put and get cycles;
- `image-data-create`: constructors, `createImageData` and 21 error cases.

The corpus is now 38 traces, 75 canvases and 258 observations.

**Oracles.** Captured with `tools/canvas/capture_canvas_traces.py` from
Chromium for Testing 147.0.7727.50, `--headless --disable-gpu`, on
22 November 2026, on Windows 11 and on Linux in WSL 2. Every earlier trace's
entry is unchanged on both platforms, and the four new traces' captures are
identical on both.

**Commands.**

```bash
# Linux (WSL), CI's Skia flags, 4 jobs
cargo test -p div-canvas
```

On Windows the same test runs in this PR's `windows-msvc` CI job, not
locally, to stay out of the contended census lock. Windows has no
`KNOWN_GAPS` entry, so that job passes only if every canvas and observation
is exact.

**Result on this branch, on Linux:**

```text
38 traces, 75 canvases, 258 observations checked against Chromium (linux)
text-transform-composite.c: 681 of 35200 pixels differ (max channel delta 255): known gap: ...
test result: ok. 1 passed
```

All 258 observations and 74 of 75 canvases are exact. The one canvas left is
the transformed web-font text gap recorded in
[the text record](canvas-text-2026-11-21.md), which readback does not touch.

## Known gaps

1. **`ImageDataSettings`:** colour spaces and the float pixel formats come
   with C6 (RFC 0008 §5).
2. **Tainting:** the check is in place, but no source clears the flag until
   H1, so `SecurityError` is not measured.
3. **Not measured (no trace, because each needs gigabytes):** an allocation
   failure's `RangeError`, and `NotSupportedError` for data over 4 GiB.
4. **Bindings:** the `TypeError` from `[EnforceRange]` belongs to the `divui`
   surface. The trace format refuses such a value instead of replaying it.
5. **Not in the crate yet:** `alpha: false` surfaces and `beginLayer`, which
   change `putImageData` and `getImageData` in Blink.

## Files outside the crate

- `tools/canvas/capture_canvas_traces.py`: the `imageData` lines, `store` and
  observing an `ImageData`.
- `tests/fidelity/fixtures/canvas-traces-v1`: the traces and captures.
