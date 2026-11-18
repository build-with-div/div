# SVG masks, V3c: `mask` (18 November 2026)

SVG lane, RFC 0009 V3c, the third of the four V3 stages in the RFC's
17 November amendment.

## What landed

- **`div_svg::mask`.**
  - `MaskAttributes` holds what a `mask` element states; `resolve` turns
    it into a `Mask` with its region in `maskUnits`.
  - `Mask::region` and `Mask::content_transform` place the region and the
    content for a reference box. `Mask::paint_mask` draws the coverage.
  - `mask_content` paints content masked by a resolved mask.
- **Ported (BSD).**
  - `core/paint/svg_mask_painter.cc`: `PaintSVGMaskLayer` (the region
    clip, the luma colour filter for `mask-type: luminance`, the content
    transform) and `MaskToContentTransform`.
  - `core/paint/css_mask_painter.cc`: the mask clip of an SVG element
    (`ResourceBoundsForSVGChild`, the region united with the visual rect)
    and `HasSingleInvalidSVGMaskLayer` (an unresolved reference leaves the
    element unmasked).
- **Clean-room.** Blink's resource masker and `mask` element are LGPL and
  were not opened. The region, its initial values, both unit systems and
  the empty-region rule were written from SVG 2 and CSS Masking, then
  checked against Chromium's pixels.
- **`tools/svg/paint_corpus.py --set v5`.** 58 cases, each at scale 1 and
  at real-zoom scale 2 (116 captures). Sets v1 to v4 are unchanged.
- **Fixtures.** `tests/fidelity/fixtures/svg-paint-v5/`, one capture per
  OS (about 340 KB). The Linux and Windows sheets are byte-identical.
- **`crates/div-svg/tests/paint_corpus.rs`.**
  - It gains `paint_v5_matches_chromium_147`.
  - Its harness resolves `mask` references (a cycle is broken at the
    closing reference), records the mask's children, and leaves out
    `display: none` elements and `visibility: hidden` shapes.
- **`tests/support/pixels.rs`.** `visibility` cascades with the other
  inherited paint properties.

## Verification

Command: `cargo test --locked --package div-svg`, on Windows (MSVC) and
Linux (WSL, Ubuntu, CfT oracle, `CARGO_INCREMENTAL=0`, `-j 4`), with the
corpus tests initializing Skia (the V3b follow-up):

- v1: 147 of 147 exact;
- v2: 115 of 115 exact;
- v3: 104 of 108 exact, the four named gaps unchanged;
- v4: 203 of 206 exact on Windows and 204 on Linux, the named gaps
  unchanged;
- v5: 116 of 116 exact on both OSes; `known-gaps.txt` is empty;
- the 32 unit tests pass, including 3 new ones for `mask`;
- the geometry corpus (76/125) and the text corpus (positions and
  pixels) are unchanged.

`cargo fmt --all --check` and
`python tools/release/generate_v02_contract.py --check` pass.

## What the corpus taught

These are measurements against Chromium 147.0.7727.50.

- **The region clip is not anti-aliased.** With an anti-aliased clip,
  every `region/` case differs (128 to 382 pixels, up to 162 levels), as
  do `target/stroked` and `chart/fade-area`.
- **A self reference.** A child of a `mask` whose own `mask` refers back
  to that mask is painted unmasked (`edge/self`). Leaving the child out
  differs in about half the slot.
- **The mask clip is not observable here.** Leaving out the mask clip
  changes no capture: content outside the region is removed by the mask
  anyway. It stays, as ported, for the call sites; the visual rect the
  harness passes is an approximation (the fill box outset by half the
  stroke width).

## Sources

The 4 BSD files were fetched under the standing rule and checked by
header first. They are in `out\chromium-ref\147.0.7727.50` and listed in
`SOURCES-svg-mask-2026-11-18.txt`:

- `core/paint/svg_mask_painter.{cc,h}`;
- `core/paint/css_mask_painter.{cc,h}`.

`platform/graphics/graphics_context.cc` (BSD) was already there. The LGPL
`layout_svg_resource_masker.*` and `svg_mask_element.*` were not opened.
