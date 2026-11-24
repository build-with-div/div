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

## Oracles by CPU class (23 and 24 November 2026)

Pinned Chromium's own pixels differ between Intel and AMD CPUs on three
`gradient-composite` canvases (`color-dodge`, `hue`, `saturation`): 7 pixels,
each one level in one channel. This is a real variation in Chromium by CPU
vendor. Div matched the same machine's Chromium byte for byte in every run
that compared them. The likely mechanism is Skia's unrefined reciprocal
estimate (RCPPS), which these blend modes use and whose results split exactly
by vendor. That is strongly supported but not isolated.

So exact now means byte-identical to pinned Chromium on the same CPU class.
Those three canvases have one oracle per class, chosen by the machine's RCPPS
fingerprint. An unknown fingerprint fails, with no skip, fallback or
tolerance. Every other canvas keeps one oracle, which must match on both
classes. Two tests check that a fingerprint selects exactly one oracle, and
that the Intel and AMD oracles differ only at the 7 known pixels.

| Platform | Intel oracle | AMD oracle |
| --- | --- | --- |
| Linux | local capture, matched exactly by 4 CI captures (Xeon 6975P-C) | CI, 2 identical captures (EPYC 9V74) |
| Windows | **local capture** (Core i9-12900KF), see below | CI, 9 identical captures (EPYC 7763, 9V74, 9V45) |

Every capture was checked against the stored oracles. Intel captures had to
match them exactly. AMD captures had to differ only at the 7 pixels.
`composite-modes`, which reaches the same Skia stages, had to show no
difference by class, and did not.

**Windows Intel comes from a local capture of pinned Chromium, not a CI
runner.** No CI Windows Intel runner reached capture in 24 jobs: 3 of the 18
Windows jobs ran on Intel. Two failed on a workflow bug before capture, and
one failed while deleting Chrome's temporary profile.

The local capture was taken on 24 November 2026 at 05:05 UTC:
- CPU: 12th Gen Intel Core i9-12900KF, RCPPS fingerprint `5fc0454d9f4c2325`,
  the same as the CI Intel runners;
- Chromium for Testing 147.0.7727.50, `chrome.exe` sha256 `5a261cd9…8505`,
  headless;
- result: all 58 canvases and all observations of both traces matched the
  stored Windows oracle byte for byte.

The CI runs were 45329548510, 45329917393 and 45330160761, on a temporary
branch that has been deleted. Their receipts (CPU model, fingerprint, Chrome
sha256, run and job ids) and the local receipt are in `div-fidelity-data` at
`archive/docs/progress/evidence/canvas-cpu-classes-v1.json`, pinned by
`fidelity-data.lock`.

The Chromium build, flags and capture commands are in
`tests/fidelity/fixtures/canvas-traces-v1/README.md`. This is a focused
measurement of `div-canvas`, not a census or a claim of renderer
qualification.
