# SVG text, glyph painting (17 November 2026)

SVG lane, RFC 0009 V2: painting the glyphs that V2a lays out. It uses the
engine lane's T1, `ShapeResult::to_positioned_text_blob` (#80).

## What landed

- **`div_svg::text::paint`** (`crates/div-svg/src/text.rs`) paints a laid-out
  `<text>`, fragment by fragment. It is ported from Blink's BSD
  `core/paint/text_fragment_painter.cc` and `core/paint/text_painter.cc`.
  For each fragment it:
  - builds a positioned blob over the fragment's part of the shaped run (T1).
    Each glyph sits at the advances before it, summed in single precision,
    plus its offset;
  - draws it under `scale(1 / sf)` and the fragment's rotation
    (`BuildSvgTransformForPaint` without `lengthAdjust`);
  - places it at the box origin truncated to 1/64 px plus the integer
    ascent;
  - fills and strokes in `paint-order`, always anti-aliased
    (`shape-rendering` does not apply to text);
  - scales the stroke width and dashes by the scaling factor, unless
    `vector-effect` is `non-scaling-stroke` (`PrepareStrokeGeometry`).
- **Fragments keep their shaped run.** `TextFragment` gains `shape` and
  `shape_characters`.
- **`TextFont` carries the raster configuration.** It is the computed
  style's `RasterConfiguration`, as for HTML text. `TextLayout` keeps it, and
  painting picks the raster policy and LCD origin the way `div-paint` does
  for author text. `div-svg` now depends on the workspace crate
  `div-geometry`, which defines it.
- **The pixel capture.** `tools/svg/text_corpus.py pixels` takes the 131
  position cases plus 14 paint cases (`paint/`) at scales 1 and 2. Fixtures:
  `tests/fidelity/fixtures/svg-text-v1/pixels-{linux,windows}.json` and
  their PNG sheets (about 1.6 MB).
- **The pixel test.** `crates/div-svg/tests/text_corpus.rs` gains
  `text_pixels_match_chromium_147`. Its pixel helpers (tiling, the root
  box, the sheet comparison and the gap check) are in
  `tests/support/pixels.rs`, which `paint_corpus.rs` now uses as well.

## Verification

Command: `cargo test --locked --package div-svg`, on Windows (MSVC) and
Linux (WSL, CfT oracle).

- **Text positions:** unchanged at 24,056 numbers per OS with the new raster
  configuration. The listed T3 and T4 gaps are the same.
- **Text pixels, Linux:** 244 of 290 exact. Every 1x stroke, opacity, dash,
  `paint-order`, `currentColor` and `crispEdges` case is exact.
- **Text pixels, Windows:** 0 of 290 exact. All differences are named gaps
  (below).
- **The other corpora are unchanged:** geometry 76/125, paint v1 147/147,
  paint v2 115/115, paint v3 104/108 (on `paint_corpus.rs` moved to the
  shared helpers).

## Named gaps (`svg-text-v1/pixel-gaps.txt`)

Every gap is owned outside the SVG lane, and `div-svg` does not work around
it.

| Gap | Linux | Windows | Owner |
| --- | --- | --- | --- |
| **T5, LCD size limit** | 18 | – | engine, `div-text` `web_font_outlines.rs` |
| **T6, hinting under rotation and skew** | 24 | – | engine, `div-text` `web_font_outlines.rs` |
| **T4 positions** | 4 | in the 13 below | engine, `div-text` |
| **T3/T4 positions plus Windows raster** | – | 13 | engine, then fidelity |
| **Windows text raster** | – | 277 | fidelity lane |

- **T5, LCD size limit.** Skia masks glyphs larger than 48 device px
  (`SK_MAX_SIZE_FOR_LCDTEXT`) in grayscale, and Chromium's are grayscale
  (not one coloured pixel). Div draws a Linux web font through a unit-size
  font whose typeface holds outlines at the physical size, so Skia sees size
  1 and keeps the LCD mask.
  - Experiment (local, not committed): grayscale edging for the web-font run
    above 48 px made all 18 captures exact.
  - HTML text in a web font above 48 device px takes the same path.
- **T6, hinting under rotation and skew.** `web_font_outlines` hints outlines at
  the physical size whatever the transform. Chromium's Fontations scaler
  leaves glyphs unhinted when the device matrix is not a scale: a `rotate`
  glyph, a rotated or skewed CTM, synthetic italic.
  - Experiment (local, not committed): unhinted outlines everywhere made
    `run/italic` exact and cut the rotated cases to a few edge pixels
    (`list/rotate` 1x: 483 to 66 pixels).
  - The axis-aligned glyph in `list/rotate-negative` then went wrong, which
    fits the rule.
  - The residue is for the engine lane to find.
- **Windows text raster.** Both sides paint these web-font glyphs with LCD
  masks.
  - In the cases inspected, the ink, bounds and placement match.
  - 269 of the 277 differ in tone only, by at most 16 per channel.
  - Grayscale masks (glyphs above 48 px) differ by up to 9.
  - Stroked glyphs at 1x differ by up to 67; the same strokes are exact on
    Linux.

## What the corpus taught (Chromium 147)

- **LCD on both OSes.** Chromium paints this SVG text with LCD subpixel
  masks on Linux as well as Windows. It uses the raster configuration of
  HTML text: before that configuration was passed, only 17 of 290 Linux
  captures were exact.
- **Strokes.**
  - The stroke width follows the scaled font: `paint/stroke-scaled` (scaling
    factor 1.5) is exact at 1x.
  - `non-scaling-stroke` keeps the unscaled width: `paint/non-scaling` is
    exact at 1x.
  - Dashes are scaled with the width, as `PrepareStrokeGeometry` passes the
    factor. This is not measured on its own: `paint/stroke-dash` is exact at
    1x, where the factor is 1, and at 2x it is in the LCD size gap.

## For the engine lane

- **Two `div-text` requests from this PR**, both in `web_font_outlines.rs`,
  now engine tasks T5 and T6, each to be fixed with a census:
  - T5: keep Skia's 48 px LCD cutoff for unit-size web-font runs;
  - T6: leave outlines unhinted when the device matrix is not a scale.
  Both need the size or transform the blob is drawn at. Until they land
  they stay named gaps here.
- **T4.** SVG text already hands `div-text` its scaled size
  (`TextFont::description`), so T4's truncation will apply.

## Sources

No new fetch. The BSD files `core/paint/text_fragment_painter.cc`,
`core/paint/text_painter.cc` and `core/layout/inline/fragment_item.cc` were
fetched for V2a (`SOURCES-svg-text-2026-11-17.txt`). Each header was checked
again on its own before reading. No LGPL file was read past its header.
