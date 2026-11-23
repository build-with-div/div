# Canvas 2D gradients, 22 November 2026

RFC 0008 §7 step 4, first half: `div-canvas` gains `CanvasGradient` with
`createLinearGradient`, `createRadialGradient` and `createConicGradient`, as
fill and stroke styles for paths, rectangles and text. Each call is compared
exactly with Chromium 147.0.7727.50 by replaying call traces, against a
separate capture on each platform. It needs no `<canvas>` element and changes
no crate other than `div-canvas`. Patterns are the second half of step 4 and
wait for `drawImage`.

## What the crate does

- **Factories:** Blink's order of checks. A negative radius is an
  `IndexSizeError` before anything else, and each coordinate is clamped to a
  float.
- **`addColorStop`:** an offset outside 0 to 1 is an `IndexSizeError`, checked
  before the colour. A colour that does not parse is a `SyntaxError`.
  `currentcolor` is black, as Blink resolves it for a gradient.
- **Stops:** kept in insertion order until a shader is needed, then stably
  sorted, so stops at one offset keep their order. Stops are padded to 0 and
  1 as `FillSkiaStops` does, and a gradient with no stops is transparent.
- **Shaders:** the same Skia gradients Blink builds: unpremultiplied sRGB
  interpolation, clamp tiling and dither. A degenerate gradient (equal points,
  or equal circles) is an empty shader and draws nothing.
- **Live objects:** clones of a `CanvasGradient` are one object. A stop added
  after assignment shows at the next draw, on every canvas that holds the
  gradient, and `fillStyle` reads back the same object.
- **Global alpha:** a gradient draws through a black paint carrying the global
  alpha, as `CanvasStyle::ApplyToFlags` does.

`fill_style()` and `stroke_style()` now return `CanvasStyleValue`, a colour
string or a gradient, because a style can be either. No other crate calls them.

`trace` gains the three factories with a `store` key, `gradient` lines for
`addColorStop`, and `{"gradient": name}` as a style value. A style read that
yields a gradient is observed by name, so the comparison checks identity.

## Not ported, or limited

- **CSS Color 4 gradient attributes:** `colorInterpolationMethod`,
  `hueInterpolationMethod` and `premultipliedAlpha` are behind the
  experimental `CanvasGradientCSSColor4` feature in Chromium 147, so script
  cannot reach them.
- **Non-legacy stop colours:** `div_style::parse_css_color` is still the
  parser for canvas colours. Blink chooses another interpolation space for a
  non-legacy colour, and that path (with `none` components) is not ported.
  No trace uses one. It belongs with C6 and the colour-parser gap already
  with the engine lane.
- **Sticky dither:** Blink leaves dither set on its flags after a gradient is
  replaced by a colour. Skia ignores dither on a solid colour without a mask
  or image filter, so nothing here can show it. The shadows step adds a mask
  filter and must measure this case.

## Sources

Each file's licence header was checked alone before its body was read. No
file was quarantined. Three files were fetched under the standing rule, from
`chromium.googlesource.com` at the pinned tag, and logged in `SOURCES.txt`:
`ui/gfx/geometry/clamp_float_geometry.h`,
`platform/geometry/skia_geometry_utils.h` and `platform/graphics/color.cc`.

| Blink file | Licence | Ported to |
| --- | --- | --- |
| `modules/canvas/canvas2d/canvas_gradient.{h,cc}` | Apple BSD | `src/style.rs` |
| `modules/canvas/canvas2d/canvas_style.{h,cc}` | Apple BSD | `src/style.rs` |
| `platform/graphics/gradient.{h,cc}` | Apple BSD | `src/style.rs` |
| `platform/graphics/color.cc` (`ToGradientStopSkColor4f`) | Apple BSD | `src/style.rs` |
| `ui/gfx/geometry/clamp_float_geometry.h` | Chromium BSD | `src/style.rs` |
| `platform/geometry/skia_geometry_utils.h` | Chromium BSD | `src/style.rs` |
| `modules/canvas/canvas2d/canvas_2d_recorder_context.cc` (factories, style setters) | Chromium BSD | `src/context.rs` |

Read for facts only, nothing ported: `canvas_gradient.idl`,
`canvas_2d_recorder_context.idl`, `canvas_rendering_context_2d_state.{h,cc}`
(when flags are synced), `cc/paint/paint_shader.cc` (which Skia gradient each
type makes), `platform/wtf/math_extras.h` (`Rad2deg`) and one entry of
`runtime_enabled_features.json5`.

## Verification

Nine traces join the corpus in
`tests/fidelity/fixtures/canvas-traces-v1/traces`, with 102 canvases and 123
observations:

- `gradient-linear`, `gradient-radial`, `gradient-conic`: geometry, stop
  order, equal offsets, padding, alpha, no stops, degenerate and huge values;
- `gradient-transform`: each kind under scale, rotation and skew, and a
  non-invertible transform;
- `gradient-shapes`: paths, even-odd fill, arcs, clips, strokes, dashes and
  `strokeRect`;
- `gradient-text`: `fillText` and `strokeText`, with global alpha;
- `gradient-composite`: all 27 distinct composite and blend modes, and four
  global alphas;
- `gradient-state`: identity, save and restore, live stops, a gradient shared
  by two canvases, and a return to a colour;
- `gradient-errors`: invalid offsets and colours, and factory errors.

The corpus is now 47 traces, 177 canvases and 381 observations. The capture
left every earlier trace's entry unchanged on both platforms.

| Platform | Canvases exact | Observations exact |
| --- | --- | --- |
| Linux, local | 176 of 177 | 381 of 381 |
| Windows | from the `windows-msvc` CI job | from the same job |

The one Linux canvas left is the known gap `text-transform-composite.c`
(scaled, skewed and rotated web-font text, engine track). All 102 gradient
canvases are exact on Linux. Windows has no known gap in the test, so a green
job means 177 of 177 and 381 of 381.

```bash
cargo test -p div-canvas
```

The Chromium build, flags and capture commands are in
`tests/fidelity/fixtures/canvas-traces-v1/README.md`. This is a focused
measurement of `div-canvas`, not a census or a claim of renderer
qualification.
