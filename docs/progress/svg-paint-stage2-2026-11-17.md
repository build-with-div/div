# SVG paint, stage 2: dashes, paint order, shape rendering, non-scaling stroke (17 November 2026)

SVG lane, V1 stage 2 (RFC 0009). This builds on the paint core in #71.

## What landed

- **`div_svg::paint`:**
  - `paint_shape` paints the layers in `paint-order` (`PaintOrder`, with
    `parse`). Markers come in a later clean-room PR.
  - `shape-rendering` sets anti-aliasing (`ShapeRendering`).
  - `pathLength` scales the dash array and offset (`dash_scale_factor`,
    `Shape::path_length`).
  - `vector-effect: non-scaling-stroke` draws the path mapped into a
    caller-given host space, on a canvas carrying the inverse.
  - `Matrix::inverse` computes that inverse in double precision.
- **`tools/svg/paint_corpus.py --set v2`:** 85 cases at scale 1, and 30 of
  them again at real-zoom scale 2. They cover dashes, `pathLength`, paint
  order, shape rendering, non-scaling stroke, and nine chart compositions
  shaped like Recharts' output. The v1 set and its fixtures are unchanged:
  the generator's v1 corpus hash still matches the captured one.
- **Fixtures:** `tests/fidelity/fixtures/svg-paint-v2/`, one capture per OS,
  about 180 KB in total. The Linux and Windows sheets are pixel-identical.
- **`crates/div-svg/tests/paint_corpus.rs`:** one test per set, each with
  its own `known-gaps.txt`.

## Verification

Command: `cargo test --locked --package div-svg`, on Windows (MSVC) and on
Linux (WSL CfT oracle). Results:

- v1: 147 of 147 captures exact;
- v2: 115 of 115 captures exact;
- the 14 unit tests pass;
- the geometry corpus is unchanged at 76/125.

`cargo fmt --all --check` passes.

## What the corpus taught

Measured against Chromium 147.0.7727.50. These are the rules the clean-room
parts follow. Blink computes them in LGPL `layout_svg_shape` and
`svg_layout_support`, which were not opened.

- **Non-scaling host space.** The host space is the outer `svg`'s own
  coordinates before its paint offset and zoom. With the full device
  transform instead:
  - the scale-2 cases differ by up to 938 pixels, because the stroke loses
    its zoom;
  - `nonscaling/viewbox-none` and `nonscaling/nested` differ by a few
    levels at scale 1.
- **`pathLength`.** It scales by Skia's contour length (90.656 for the
  curve in `dash/pathlength-path`), not by Chromium's `getTotalLength`
  traversal for `<path>` (90.766), which differs by 136 pixels. Every
  other length tried between them was worse.
  - A negative `pathLength` is ignored.
  - Zero strokes solid.
- **Dash precision.** `stroke-dasharray`, even as a presentation attribute,
  is read in double precision and stored as a float, as CSS reads numbers.
  `div-svg`'s single-precision scanner leaves `0.05 0.03` three pixels off.
  - This is evidence for the engine's E2 precision question, for this
    property only. The test reads dash lengths that way (`css_length`).
- **Dash resolution.**
  - An array with a negative entry, or one summing to zero, draws solid.
  - Percentages resolve against the normalized diagonal.
  - Dashes start where the shape's Skia path starts, and run clockwise.

## Fetches

None. The painter files were already in `out\chromium-ref` from #71. The
header of each was rechecked alone (`head -n 30`) before reading:

- `svg_shape_painter.cc`: BSD, Chromium Authors;
- `stroke_data.cc`: BSD, Google 2013.

`transform_helper` was approved, but it was not needed and not fetched.

## Not claimed

- Markers.
- Paint servers, clipping and text.
- Element and group `opacity`, which `div-paint` applies at the call site.
- Fractional scales.
- The untiled path.
- Anything on a page: the painter is not called from `div-paint` yet.
