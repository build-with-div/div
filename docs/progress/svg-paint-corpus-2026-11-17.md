# SVG paint core against Chromium 147 (17 November 2026)

RFC 0009 V1 stage 1, second PR: `div_svg::paint`, which paints `div-svg`
shapes onto a Skia canvas, and the pixel corpus it is checked against.
The engine does not call it yet. The call sites in `div-paint` wait on
integration requests E1 to E3 and F1: namespaces, SVG properties with a
presentation-hint level, replaced sizing of the outer `svg`, and `view!`.

**What landed.**

- `crates/div-svg/src/paint.rs`, ported from Blink's BSD-licensed painters
  at 147.0.7727.50:
  - `core/paint/svg_shape_painter`, `svg_object_painter`,
    `svg_model_object_painter`, `svg_root_painter` and
    `svg_container_painter`;
  - `platform/geometry/stroke_data`.

  It covers fill then stroke, `drawRect` and `drawOval` for plain rects
  and ellipses, the colour times opacity, stroke setup, the root
  transform to the pixel-snapped border box, and viewport clips. It does
  not flush, read back or assume the canvas is the final surface, so the
  call-site PR can record it into Div's display list. No LGPL code
  (`layout_svg_*`) was read: the painter takes `div-svg`'s own types.
- `Shape` now records how Chromium draws it (`Geometry::Rectangle`,
  `Ellipse` or `Path`) and whether it renders at all (`empty`).
- `tools/svg/paint_corpus.py` builds 107 cases at scale 1, plus the 20
  vendored Lucide icons at 16 and 24 px at scale 2 (real zoom,
  `--force-device-scale-factor` through `render_chrome`).
- `tests/fidelity/fixtures/svg-paint-v1/`: the Linux and Windows CfT
  captures (JSON and one PNG per scale, about 180 KB in all). The two
  OSes' sheets are pixel-identical.
- `crates/div-svg/tests/paint_corpus.rs` compares every slot byte for
  byte. It rasterizes as Div's renderer does: 256-px tiles stepping by
  254, the case in each tile's device space, on a full tile surface.
- `crates/div-svg/tests/support/` holds the markup scanner and shape
  helpers, now shared with the geometry corpus test.

**Verification.**

```sh
cargo test --locked --package div-svg
```

With the CI Skia flags on 17 November, Windows MSVC and Linux (WSL,
Ubuntu 26.04) each pass:
- 10 unit tests;
- the geometry corpus: 76 of 125 cases exact, unchanged;
- the paint corpus: 147 of 147 captures exact.

`known-gaps.txt` is empty.

**What the corpus taught.** On the first run, 110 of 147 captures were
exact. The painter port was already right; the test's raster setup was
not:

- Painting each case at the origin rather than at its place in a
  Chromium raster tile moved stroke and curve edges by a few levels.
  Skia strokes and maps points in single precision in device space.
- Rasterizing on a surface the size of the case rather than a 256-px tile
  changed anti-aliasing near tile edges. AAA's clipping depends on the
  device bounds.

With `div-paint`'s existing tile model (`render.rs`), every capture is
exact. On the way, the corpus ruled these out as causes: the blitter
colour space, anti-aliased clips, path convexity, and how the oval path
is constructed.

**Chromium sources fetched** (standing rule, one file per request, from
chromium.googlesource.com at 147.0.7727.50, read-only in
`out\chromium-ref`, listed in `SOURCES-svg-2026-11-17.txt`):

- the five `core/paint/svg_*_painter` pairs, all BSD;
- `DEPS`, `skia/BUILD.gn`, `skia/features.gni` and
  `skia/config/SkUserConfig.h`, to compare Chromium's Skia build with
  Div's.

`platform/geometry/stroke_data.{cc,h}` and `runtime_enabled_features.json5`
were already there. Of the latter, only the
`SvgInlineRootPixelSnappingScaleAdjustment` entry was read (status
`test`).

**Not claimed.**

- Anything about SVG on a page: the engine does not call the painter yet.
- Dashes, `paint-order`, markers, `vector-effect`, `shape-rendering`,
  paint servers, and scales other than 1 and 2.
