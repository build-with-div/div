# SVG paint servers, V3b: gradients and patterns (18 November 2026)

SVG lane, RFC 0009 V3b, the second of the four V3 stages in the RFC's
17 November amendment.

## What landed

- **`div_svg::paint_server`.**
  - `GradientAttributes` and `PatternAttributes` hold what an element
    states, `inherit` fills the rest from an `href` target, and `resolve`
    turns them into a `Gradient` or a `Pattern`.
  - `Gradient::shader` and `Pattern::shader` build the Skia shader for a
    reference box and an extra local matrix. `PaintServer` is either one.
- **`div_svg::paint`.**
  - `SvgPaint::Server` holds a server and its fallback colour.
  - `prepare_paint` paints a server as `PreparePaint` does, with the
    shape's fill box as the reference box.
- **`div_svg::text`.** Glyph paint takes servers, with the text painter's
  shader transform (`EnsureShaderTransform`: the inverse of the
  fragment's rotation, then the zoom). `objectBoundingBox` servers on
  text fall back until the text's bounding box includes its glyph ink
  (V2b).
- **Ported (BSD).**
  - `platform/graphics/gradient.cc`: stop filling, tile modes,
    interpolation, dithering and the two-point conical radial gradient.
  - `core/paint/svg_object_painter.cc`: `PreparePaint` for a server.
  - `platform/graphics/paint_record_pattern.cc` and
    `cc/paint/paint_shader.cc`: the pattern tile as a recorded picture
    shader, rasterized at scale.
- **Clean-room.** Blink's gradient, pattern and paint-server resources
  and elements are LGPL and were not opened. `href` inheritance, unit
  resolution, the empty-box rule, the pattern tile, its content
  transform and both local matrices were written from SVG 2, then fitted
  to Chromium's pixels.
- **`tools/svg/paint_corpus.py --set v4`.** 103 cases, each at scale 1 and
  at real-zoom scale 2 (206 captures). Sets v1 to v3 are unchanged.
- **Fixtures.** `tests/fidelity/fixtures/svg-paint-v4/`, one capture per
  OS. The Linux and Windows sheets are byte-identical.
- **`crates/div-svg/tests/paint_corpus.rs`.**
  - It gains `paint_v4_matches_chromium_147`.
  - Its harness resolves `fill` and `stroke` references with their
    `href` chains (cycles stop at the closing reference) and records
    pattern content into a picture.
- **`tests/support/pixels.rs`.**
  - Tiles are sRGB-tagged, as `div-paint`'s `create_raster_surface`
    makes them.
  - Colour channels convert as Blink converts them, `r / 255`.
  - Neither changes a v1 to v3 or text result.

## Verification

Command: `cargo test --locked --package div-svg`.

Windows (MSVC) and Linux (WSL, Ubuntu, CfT oracle) give the same results:

- v1: 147 of 147 exact;
- v2: 115 of 115 exact;
- v3: 104 of 108 exact, the four named gaps unchanged;
- v4: 188 of 206 exact, the 18 others named gaps;
- the 29 unit tests pass, including 5 new ones for `paint_server`;
- the geometry corpus (76/125) and the text corpus (positions and
  pixels) are unchanged.

The Linux capture of `v4` was retaken after WSL recovered from a
read-only file system on 18 November. The retake is byte-identical to the
first Linux capture and to the Windows one, apart from its timestamp.
Linux was run with `CARGO_INCREMENTAL=0` and `-j 4`.

`cargo fmt --all --check` and
`python tools/release/generate_v02_contract.py --check` pass.

## Named gaps

*Update, follow-up PR:* the test binaries now initialize Skia at load
(below), and three gaps remain on Windows and two on Linux. As first
measured, `tests/fidelity/fixtures/svg-paint-v4/known-gaps.txt` listed 18
captures on each OS. They were the same on both, except that `rad/focal`
at scale 2 differed in 2 pixels on Linux and 1 on Windows. All differed
only where a value sits on a rounding boundary:

- one channel level in 1 to 6 pixels of radial gradients
  (`rad/*`, `href/radial-from-linear`) and of scaled pattern tiles
  (`pat/obb-fraction`, `pat/transform` at scale 2);
- the seam of `spread/repeat-diagonal`, 38 pixels at scale 1 and 73 at
  scale 2, where the repeating gradient's parameter is exactly 1:
  Chromium takes the last stop and Div the first.

`div-svg` hands Skia what cc does for these cases (the same points,
radii, stops, positions, local matrix, tile mode, interpolation,
dithering and surface colour space), and both build Skia m147. The
difference is below Div's code. Owner: the fidelity lane, with the setup
track for the Skia build.

### Skia initialization (follow-up)

The fidelity lane found the cause: `div-svg`'s corpus test binaries never
called `SkGraphics::Init`, so Skia's raster pipeline ran its baseline SSE
stages instead of the AVX2 and FMA ones Chromium selects at start-up. The
three corpus tests now call `div_text::initialize_skia_at_load!()`, the
macro `div-paint`'s tests use. Re-measured with
`cargo test --locked --package div-svg` on both OSes:

- v4: 203 of 206 exact on Windows, 204 on Linux. 15 gaps on Windows and
  16 on Linux became exact, `spread/repeat-diagonal`'s seam among them.
- Still open, one level each at scale 2: `rad/focal` (1 pixel on Windows,
  2 on Linux), `rad/fr-focal` (1 pixel, both OSes) and, on Windows only,
  `rad/repeat` (2 pixels).
- v1 to v3, geometry and text are unchanged on both OSes.

## What the corpus taught

These are measurements against Chromium 147.0.7727.50.

- **No stops.** A gradient with no stops applies as transparent. Its
  fallback colour is not used. Treating it as "does not apply" made
  `ref/no-stops-fallback` differ on every pixel.
- **Pattern filtering.** The tile shader filters linearly. Nearest
  filtering differed in every scaled tile.
- **Surface colour space.** An sRGB-tagged surface, as `div-paint` uses,
  made two more pattern cases exact; an untagged one left them off.
- **Channel conversion.** `r / 255` instead of `r * (1 / 255)` changes
  which dithered gradient pixels differ, and leaves fewer.
- **Interpolation.** Unpremultiplied sRGB interpolation, as
  `gradient.cc` sets for SVG, matches `stop/opacity` and
  `stop/opacity-half`.

## Sources

The 5 BSD files were fetched under the standing rule and checked by
header first. They are in `out\chromium-ref\147.0.7727.50` and listed in
`SOURCES-svg-paint-server-2026-11-18.txt`:

- `core/layout/svg/layout_svg_resource_paint_server.cc`;
- `platform/graphics/paint_record_pattern.{cc,h}`;
- `cc/paint/paint_shader.{cc,h}`.

`gradient.cc`, `svg_object_painter.cc` and `text_fragment_painter.cc`
were already there. The LGPL gradient and pattern resources and
elements, `*_attributes.h` and `layout_svg_resource_paint_server.h` were
not opened.
