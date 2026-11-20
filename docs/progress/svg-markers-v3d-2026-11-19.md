# SVG markers, V3d part 2 (19 November 2026)

SVG lane, RFC 0009 V3d. V3d lands in two PRs: `use` and `symbol` (part 1),
then this one, markers.

## What landed

- **`div_svg::marker`.**
  - `marker_positions` gives the vertices of a path's segments, each with
    its marker kind (start, mid or end) and its `orient="auto"` direction.
  - `MarkerAttributes::place` gives a marker's transform, its viewport
    (which clips the content unless `overflow` is visible) and its content
    transform, for a vertex and a stroke width.
  - `Orient` parses `auto`, `auto-start-reverse` and angles in `deg`,
    `rad`, `grad` and `turn`; `MarkerUnits` is `markerUnits`.
  - The module doc records where Chromium's directions differ from SVG 2.
- **`div_svg::paint`.** `paint_shape` takes the markers as a callback,
  which it calls at the markers layer of `paint-order`.
- **Clean-room.** Blink's marker painting
  (`SVGShapePainter::PaintMarkers` and `PaintMarker`, BSD) is followed:
  save, concatenate the marker transform, clip to the viewport unless
  `overflow` is visible, paint the content. Blink's marker resource,
  marker element and marker position code are LGPL and were not opened
  (`svg_marker_data.h` is not in `out\chromium-ref` and was not fetched).
  Positions, directions and the transform come from SVG 2 and black-box
  observation of Chromium 147.
- **`tools/svg/paint_corpus.py --set v7`.** 86 cases, each at scale 1 and
  at real-zoom scale 2 (172 captures). Sets v1 to v6 are unchanged, and
  case ids are now prefixed only where `id="` starts an attribute (the
  old replacement also matched `marker-mid="`); the v1 to v6 case lists
  are byte-identical before and after.
- **Fixtures.** `tests/fidelity/fixtures/svg-paint-v7/` holds one capture
  per OS. The Linux and Windows sheets are byte-identical.
- **`crates/div-svg/tests/paint_corpus.rs`.**
  - It gains `paint_v7_matches_chromium_147`.
  - Its harness resolves marker references (with self-reference cut off),
    paints each marker's content inheriting from the marker, and resolves
    `context-fill` and `context-stroke` to the shape's paints.
  - A shape's bounds include its markers', and opacity no longer folds
    into a shape whose markers paint.
- **`tests/support`.** The marker properties cascade, the `marker`
  shorthand expands in `style`, and `stroke-width` resolves as a length
  (percentages against the normalized diagonal).

## Verification

Command: `cargo test --locked --no-fail-fast --package div-svg`, on main
cba2c60 (with #101 and #102). Windows (MSVC) and Linux (WSL,
Ubuntu, CfT oracle, `CARGO_INCREMENTAL=0`, `-j 4`) give the same results
except where noted:

- v1: 147 of 147 exact;
- v2: 115 of 115 exact;
- v3: 106 of 108 exact;
- v4: 203 of 206 exact on Windows and 204 on Linux;
- v5: 116 of 116 exact;
- v6: 120 of 122 exact;
- v7: 167 of 172 exact; the five others are the named gap below;
- the 39 unit tests pass, including 4 new ones (`marker`);
- the geometry corpus (76/125) and the text corpus (positions and
  pixels) are unchanged.

`cargo fmt --all --check` and
`python tools/release/generate_v02_contract.py --check` pass.

## What the corpus taught

These are measurements against Chromium 147.0.7727.50.

- **Which shapes and vertices.** Markers apply to `path`, `line`,
  `polyline` and `polygon` only. There is a vertex at every move and at
  the end of every segment, the closing one included. A lone move takes
  only `marker-end`.
- **Directions.** The first version followed SVG 2 and missed eleven
  `dir/` and `pos/` cases. Three rules fixed them:
  - a zero-length segment points along the x axis;
  - a move has the direction from the current point (the origin, for the
    first) to its point, and a vertex at a move points along the segment
    leaving it, or along the move when none does;
  - a turn is taken in [-180°, 180°), so a full reversal bisects to -90°.
  The `dir/` cases put an arrow at every vertex for this; the first draft
  used dots for `marker-mid`, which hid all of it.
- **The transform is concatenated in single precision.** Composed in
  double precision and set once, 6 more captures differ than when the
  marker transform and then the `viewBox` transform are concatenated onto
  the canvas, as Chromium's canvas does.
- **The viewport clip is not anti-aliased.** With anti-aliasing, 92 of
  the 172 captures differ.
- **Properties.** Content inherits from the marker, not the shape;
  `context-fill` and `context-stroke` take the shape's paints; the
  `marker` attribute does nothing (the shorthand is a property only); a
  marker's `transform` is ignored; `display: none` on the marker does not
  stop it; a hidden shape draws no markers; `refX` keywords are not
  supported, and percentages resolve against the shape's viewport.
- **Opacity.** A shape's markers fade with it as one layer, bounded by
  the shape's and the markers' rects. The single-draw fold no longer
  applies; folded, the markers stayed opaque (413 pixels at 1x).

## Remaining gap

At scale 2, `dir/polyline`, `prop/inherited`, `pos/arc-degenerate`,
`pos/cubic-degenerate` and `pos/quad` differ at 3 to 6 edge pixels of an
arrowhead, by up to 8 levels, the same on both OSes. The difference
follows the slot's place on the sheet: `pos/polyline` draws the same end
arrow exactly in another column, and `pos/line` is exact while the two
degenerate paths, with the same geometry, differ identically. Offsetting
every angle by a few millionths of a degree flips these pixels, and no
single offset fixes all five. Four alternatives each matched fewer cases:
double-precision composition, rounding in layer space before the tile
translation, single-precision angles, and the reference point moved into
the content transform. So the cause is in the single-precision rounding
of the marker transform or of its device translation, and the exact
pipeline is unmeasured. Owner: SVG lane.

## Sources

Nothing was fetched. `core/paint/svg_shape_painter.cc` (BSD, already in
`out\chromium-ref\147.0.7727.50`, header checked earlier) was read for
`PaintShape`, `PaintMarkers` and `PaintMarker`. No LGPL marker file was
opened.
