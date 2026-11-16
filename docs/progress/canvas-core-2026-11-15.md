# Canvas 2D core state and paths, 15 November 2026

RFC 0008, migration step 2: the new `div-canvas` crate implements the
core of `CanvasRenderingContext2D`. Each call is compared exactly with
Chromium 147.0.7727.50 by replaying call traces.

## What the crate does

`div_canvas::CanvasRenderingContext2D` owns a raster backing store (N32,
premultiplied, as Blink's software canvas). It implements:

- **State:** `save` and `restore`, including Blink's re-expression of the
  current path in the restored user space.
- **Transforms:** `scale`, `rotate`, `translate`, `transform`,
  `setTransform`, `resetTransform` and `getTransform`. As in Blink, the
  state keeps a double-precision affine transform while Skia's float matrix
  receives the same operations separately. A non-invertible transform maps
  points and suppresses drawing exactly as Blink does.
- **Compositing:** `globalAlpha` and all 26 `globalCompositeOperation`
  names. `source-in`, `source-out`, `destination-in` and `destination-atop`
  draw through Blink's composited layer; `copy` clears first.
- **Styles:** colour `fillStyle` and `strokeStyle`, with Blink's
  serialization. Line styles: `lineWidth`, `lineCap`, `lineJoin`,
  `miterLimit`, `setLineDash`, `getLineDash` and `lineDashOffset`.
- **Paths:** `CanvasPath` with Blink's single-line and single-arc fast paths
  (`drawLine` and `drawArc`). `CanvasUsesArcPaintOp` is `stable` in 147, so
  the arc path is on. The path calls are `moveTo`, `lineTo`,
  `quadraticCurveTo`, `bezierCurveTo`, `arcTo`, `arc`, `ellipse` (with the
  degenerate-ellipse lines), `rect`, `roundRect` (1–4 radii, points, and
  negative sizes) and `closePath`.
- **Drawing:** `fill` (nonzero and evenodd), `stroke`, `clip`, `fillRect`,
  `strokeRect` (with the one-dimension line case), `clearRect`,
  `isPointInPath` and `isPointInStroke`.
- **Draw skipping:** Blink's dirty-rect check that skips a draw whose
  inflated bounds miss the clip, including its effect with a `miterLimit`
  below 1.
- **Readback:** the whole bitmap as `getImageData` returns it.

`div_canvas::trace` implements the trace format of RFC 0008 §7 item 3 and
replays a trace into its observations and bitmaps.

## Sources

Every ported file was read from Chromium 147.0.7727.50, fetched read-only
under the user's approvals of 15 November 2026. Each file's licence header
was checked before its body was read. All are BSD-style; none is copyleft.

| Blink file | Licence | Ported to |
| --- | --- | --- |
| `modules/canvas/canvas2d/canvas_path.{h,cc}` | Apple / Adobe BSD | `path.rs` |
| `platform/geometry/path_builder.cc` | Chromium BSD | `path.rs` |
| `modules/canvas/canvas2d/canvas_2d_recorder_context.{h,cc}` | Chromium BSD | `context.rs` |
| `modules/canvas/canvas2d/canvas_rendering_context_2d_state.cc` | Chromium BSD | `state.rs` |
| `modules/canvas/canvas2d/canvas_style.{h,cc}` | Apple BSD | `state.rs` |
| `modules/canvas/canvas2d/clip_list.cc` | Chromium BSD | `context.rs` |
| `platform/geometry/stroke_data.cc` | Google BSD | `context.rs` |
| `platform/geometry/path.cc` (`StrokeContains`) | Apple BSD | `context.rs` |
| `platform/transforms/affine_transform.{h,cc}` | Apple / Google BSD | `affine.rs` |
| `platform/wtf/math_extras.h` | Apple BSD | `math.rs` |
| `platform/geometry/float_rounded_rect.h` (`operator SkRRect`) | Adobe BSD | `path.rs` |
| `platform/runtime_enabled_features.json5` (`CanvasUsesArcPaintOp` only) | Chromium BSD | `path.rs` |

Their notices are reproduced in `crates/div-canvas/THIRD_PARTY.md`.

The following were written without reading their sources:

- `WebCoreFloatNearlyEqual` (in `skia_utils.h`) was written from Skia's
  `SkScalarNearlyEqual`.
- `gfx::QuadF::Contains` was written for `RectContainsTransformedRect`.
  Both of its branches draw the same thing, so it affects no pixel.

## Verification

The traces are in `tests/fidelity/fixtures/canvas-traces-v1/traces`: 27
traces, 54 canvases and 108 observations.

**Oracles.** The oracles were captured with
`tools/canvas/capture_canvas_traces.py` from Chromium for Testing
147.0.7727.50 with `--headless --disable-gpu`:

- on Windows 11;
- on Linux, in WSL 2 Ubuntu.

The two captures are identical, in every bitmap hash and every
observation.

**Command (Windows):**

```bash
cargo test -p div-canvas --test chromium_traces -- --nocapture
```

**Result on this branch, on Windows:**

```text
27 traces, 54 canvases, 108 observations checked against Chromium (windows)
arcs-closed-full.dashed: 171 of 1600 pixels differ ...: known gap
arcs-closed-full.stroke: 180 of 1600 pixels differ ...: known gap
test result: ok. 1 passed
```

So 52 canvases are exact byte for byte, and all 108 observations are
exact.

**Linux.** CI's `rust-parity` job now runs the `div-canvas` tests, and
`windows-msvc` runs them too. The Linux run of the Rust comparison happens
there; it was not run locally.

The comparison found and fixed three problems in the comparison itself:

- **Doubles.** CDP's value serialization rounds doubles, so the capture
  returns `JSON.stringify` text instead.
- **Parsing.** serde_json's default float parser is not correctly rounded,
  so `div-canvas` enables `float_roundtrip`.
- **Alpha.** Alpha serialization uses CSS Color 4's two-or-three-decimal
  rule, as Blink does.

## Known gaps

1. **Closed full-turn arcs on the `drawArc` fast path.** This is
   `arc(...)` on an empty path with radius ≥ 1, a sweep of a full turn and
   a start other than 0, followed by `closePath()`.
   - Blink records a cc `DrawArcOp` with `setArcClosed(true)`. Its
     playback is in `cc/paint/paint_op.cc`, which was not in the approved
     sources.
   - `div-canvas` draws it the way Skia's `drawArc` builds the arc path, then
     closes it. That matches Chromium for the closed arcs in `arcs` (a
     start of 0, and partial sweeps). It does not match for the two
     canvases of `arcs-closed-full`: 180 and 171 of 1,600 pixels differ.
   - The test keeps both canvases in `KNOWN_GAPS`, reports them, and fails
     once they match.
   - Owner: this lane. The fix needs `cc/paint/paint_op.cc` and
     `paint_flags.{h,cc}`, which were requested from the parent session on
     15 November.
2. **Colour syntax.** `hsl()`/`hsla()` and the space-separated `rgb()` syntax
   are not parsed by `div-style`, for CSS as well as for canvas. The traces
   do not use them.
   - Owner: engine track, `div-style` colour parsing
     (`crates/div-style/src/property.rs`, `legacy_srgb_color` and
     `color_function`).
   - Canvas reaches it through the new `div_style::parse_css_color`.
3. **Not in this step.** These are later steps of RFC 0008's migration:
   - gradients, patterns, text, images, shadows, filters and `Path2D`;
   - `getImageData` of a sub-rectangle, and `putImageData`;
   - the `alpha: false` context.

## Files outside the new crate

- `crates/div-style/src/property.rs`: an engine-track file. It gains a
  public `parse_css_color`, a wrapper over the existing `color` parser.
  Nothing else in `div-style` changes.
- `Cargo.toml`, `Cargo.lock`: the new workspace member. No new crates.
- `.github/workflows/ci.yml`: `div-canvas` is added to both headless test
  lists.
- `tools/canvas/capture_canvas_traces.py`: new. It reuses the CDP client in
  `tools/accountability/run_all_pixel_comparisons.py` without changing it.
