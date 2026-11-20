# SVG lane tools (RFC 0009)

Tools for the inline SVG work in RFC 0009
(`docs/alpha/rfcs/0009-inline-svg.md`).

| Tool | Purpose |
| --- | --- |
| `licence_audit.py` | Classifies Chromium source files by licence header only (BSD, LGPL, GPL, other). Prints the class and the first copyright line; never prints or stores code |
| `geometry_corpus.py` | Black-box geometry oracle: queries pinned Chromium through web APIs only, for the parts RFC 0009 writes clean-room |
| `text_corpus.py` | Black-box text oracle for `div_svg::text`: the SVG text DOM APIs in pinned Chromium (`capture`), and screenshots of the same cases plus paint cases (`pixels`), at scale 1 and at real-zoom scale 2 |
| `paint_corpus.py` | Pixel oracle for `div_svg::paint`: screenshots of pinned Chromium painting each case in its own slot, at scale 1 and at real-zoom scale 2, in seven sets (`--set v1` to `--set v7`) |

## Geometry corpus

`geometry_corpus.py list` prints the 125 cases. `capture` loads them in
one page in headless Chromium over CDP (the shared client in
`tools/accountability/run_all_pixel_comparisons.py`). Each case's outer
`<svg>` sits in its own absolutely positioned 256-px slot, so no result
depends on text metrics or on another case. The output is
`div-svg-geometry-corpus-v1`. For each probed element it records:

- `getTotalLength()`, and `getPointAtLength()` at 33 evenly spaced
  lengths;
- `getBBox()`;
- `getCTM()` and `getScreenCTM()`;
- `isPointInFill()` and `isPointInStroke()` on a 17×17 grid over the box
  grown by the stroke width;
- the outer `<svg>`'s `getBoundingClientRect()`.

Exceptions are recorded as data.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `path/` | 31 | every command, absolute and relative; implicit repeats; smooth commands with and without a previous control point; number syntax (no separators, exponents, leading dots); error handling (bad command, truncated, no initial move, trailing comma, empty); zero-length subpaths |
| `arc/` | 24 | the four flag pairs; seven x-axis rotations including -60 and 400; radii too small, zero or negative; equal endpoints; packed flags; near-full, half and quarter turns; tiny spans; relative chains; pie and donut shapes as Recharts draws them |
| `rect/`, `circle/`, `ellipse/`, `line/`, `poly*/` | 24 | `rx`/`ry` rules (one given, clamped, negative, `auto`), zero and negative sizes, percentages and absolute units, odd `points` counts, separators, a self-intersecting star |
| `stroke/` | 14 | caps, joins (including `arcs` and `miter-clip`), miter limits, dashes and `evenodd`, through `isPointInStroke` and `isPointInFill` |
| `viewbox/`, `transform/` | 32 | all 19 `preserveAspectRatio` values, an offset `viewBox`, zero and negative sizes, commas, a missing size, a nested `svg`, and `transform` lists including invalid ones |

**Evidence** is one capture per OS in `tests/fidelity/fixtures/svg-geometry-v1/`:

- `chromium-linux.json`, from Linux CfT 147.0.7727.50 in WSL (Ubuntu
  26.04, glibc 2.43), the census oracle;
- `chromium-windows.json`, from the read-only Windows CfT 147.0.7727.50
  (`out\chrome-win` in the main checkout).

`crates/div-svg/tests/geometry_corpus.rs` checks each OS against its own
capture (through `include_str!`, so the captures are tracked fixtures),
and asserts the capture's platform. The two captures are identical
apart from their timestamp and platform.

To recapture:

```sh
# Linux, in WSL
python3 tools/svg/geometry_corpus.py capture \
  --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome \
  --output tests/fidelity/fixtures/svg-geometry-v1/chromium-linux.json
# Windows
python tools/svg/geometry_corpus.py capture \
  --chrome <main checkout>/out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe \
  --output tests/fidelity/fixtures/svg-geometry-v1/chromium-windows.json
```

### What `div-svg` matches

`cargo test --locked --package div-svg` compares every value exactly. On
Windows (MSVC) and on Linux (WSL), 76 of 125 cases match in every field. In the other 49, 80
fields differ, and each is listed with its cause in
`tests/fidelity/fixtures/svg-geometry-v1/known-gaps.txt`. The other fields
of those cases are still checked exactly. Every bounding box and every
fill and stroke hit grid matches in all 125 cases.

The corpus settled these choices, all as black-box observations of
Chromium:

- Arcs in the drawn path are Skia's SVG arcs (conics). Every arc
  bounding box and hit grid then matches, rotated arcs included.
  `div-svg` ports Skia's SVG `arcTo` (BSD) in `src/arc.rs` rather than
  calling it. Skia's version uses the C library's single-precision `sin`,
  `cos`, `tan` and `atan2`, which differ between C libraries: on the CI
  runner's glibc, `arc/rotation--60`'s box came out a few ulps from
  Chromium's. The port keeps Skia's float operations and their order, but
  computes each trig call in double precision and rounds it to single.
  That is the correctly rounded result on every OS, which is what
  Chromium returns in both captures.
- Numbers in attributes are accumulated in single precision, digit by
  digit, not correctly rounded: `1.9e2` reads as 190.00001525878906.
- An `rx`, `ry` or other length attribute that is present but does not
  parse (`rx="auto"`) is used as 0. An absent `rx` is `auto` and takes
  `ry`. This is presentation-attribute parsing, so it belongs to the
  engine's attribute mapping (integration request E2).
- Rects and ellipses of zero size, and negative sizes clamped to zero,
  still have a path, so their length and box are not empty. A path
  whose contours all have zero length reports its start point.

The gaps:

- Length along `path` data (73 fields, `getTotalLength` and
  `getPointAtLength` on `path/` and `arc/` cases). Chromium measures
  `path` elements by traversing their data, not the drawn path: arcs as
  cubics of at most 90 degrees and curves measured almost exactly. Shapes
  measure the drawn path in Chromium too, and match exactly. This matters
  for dashes along paths, `pathLength` and text on a path, not for
  painting strokes and fills.
- The viewBox translation under `slice` (6 fields): one double ulp,
  below what a float matrix can carry.
- The size of an outer `svg` with no width or height (1 field): layout's
  answer (E3), an input to `div-svg`.

## Paint corpus

`div_svg::paint` ports Blink's SVG painters at 147.0.7727.50, which are
BSD-licensed:

- `core/paint/svg_shape_painter`, `svg_object_painter`,
  `svg_model_object_painter`, `svg_root_painter` and
  `svg_container_painter`;
- `platform/geometry/stroke_data`.

The sources are read-only in `out\chromium-ref` (`SOURCES-svg-2026-11-17.txt`
there). It paints a `div-svg` shape with resolved fill, stroke, opacities,
fill rule and stroke geometry onto any Skia canvas. Nothing in it flushes,
reads back or assumes the canvas is the final surface.

`paint_corpus.py list` prints the cases. `capture` writes each sheet as one
page of absolutely positioned 112-px slots. It captures the sheet through
`render_chrome` in `tools/accountability/run_all_pixel_comparisons.py`:
nothing is emulated at scale 1, and scale 2 runs at real zoom
(`--force-device-scale-factor`, RFC 0010).

| Sheet | Cases | What it pins down |
| --- | --- | --- |
| 1x `fill/` | 21 | rects (integer, half and quarter pixel), rounded rects, circles (small too), an ellipse, polygons under both fill rules, curves, arcs, subpaths, colours, `fill-opacity`, overlap, the initial black fill |
| 1x `stroke/`, `both/` | 25 | widths 0.1 to 3 and 0, the initial width, circles, ellipses and rounded rects, every cap and join, miter limits, a zero-length round cap, `stroke-opacity`, fill plus stroke |
| 1x `transform/` | 8 | translate by half a pixel, rotate, scale (non-uniform too), skew, a matrix, nested groups, inherited paint |
| 1x `viewbox/`, `nested/`, `offset/` | 13 | `viewBox` at scales 2, 1.25 and 0.75, an offset `viewBox`, `meet`, `slice`, `none` and `xMinYMax`, the root clip, nested `svg` viewports (clipped, at half pixels), roots at fractional page offsets |
| 1x and 2x `lucide/` | 40 + 40 | the 20 vendored Lucide icons at 16 and 24 px, `#09090b` (`currentColor`) on white |

Those are set `v1`. Set `v2` (`--set v2`, fixtures in
`tests/fidelity/fixtures/svg-paint-v2/`) covers stage 2, with 85 cases at
scale 1 and 30 of them again at scale 2:

| Group | Cases | What it pins down |
| --- | --- | --- |
| `dash/` | 41 | odd, single, comma-separated and fractional arrays; offsets (negative, past the pattern, percentage); negative and all-zero arrays; zero-length dashes with each cap; percentages and units; dashes on every shape, arcs, subpaths and a closed path; `pathLength` on a line, circle, rect and curve, with an offset, zero and negative; dashes under transforms and a `viewBox`; a hairline; `stroke-opacity` |
| `order/` | 8 | `paint-order` `normal`, `stroke`, `stroke fill`, `fill stroke`, `markers`, `markers stroke`, an invalid value, and inheritance from a group |
| `crisp/`, `rendering/` | 14 | `crispEdges` fills and strokes at integer, half, quarter and other fractional positions, a diagonal, a curve, a rotation, inheritance, a dashed grid; `optimizeSpeed`, `geometricPrecision` and `auto` |
| `nonscaling/` | 13 | `vector-effect: non-scaling-stroke` under uniform, non-uniform, skewed and rotated transforms, a `viewBox` (`none` too), a nested `svg`, with dashes, with a fill, on a line and a curve, on a group (no effect) and at a fractional page offset |
| `chart/` | 9 | hand-written in the shape of Recharts' output, with no text: a `3 3` dashed grid, a monotone line, dots, rounded bars with `fill-opacity`, an area, a draw-in dash state, a dashed reference line, pie sectors and a radial bar using `pathLength` |

Set `v3` (`--set v3`, fixtures in `tests/fidelity/fixtures/svg-paint-v3/`)
covers `clipPath` (V3a), with 54 cases, each at scale 1 and at scale 2. Most
clip the same content, a filled square crossed by a thick red stroke, so
fills and strokes both show the clip edge. `paint_corpus.py` prefixes each
case's ids (`k<index>-`), because ids are page-wide and every case on a
sheet would otherwise resolve to the first case's `clipPath`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `clip/` | 10 | one child: rect at fractional positions, rounded rect, circle, ellipse, curve, star polygon, a line (no area), a zero-size rect, a stroke and `fill="none"` (both ignored) |
| `rule/` | 3 | `clip-rule="evenodd"` on the child and inherited from the `clipPath`; `fill-rule` ignored |
| `xf/` | 5 | `transform` on the child, on the `clipPath`, on both, on the clipped group, and on a clipped shape |
| `units/` | 6 | `clipPathUnits="objectBoundingBox"`: a rect and a circle on a group, with a `transform`, with a stroked target, on a shape, on a group of two children |
| `union/` | 5 | two or three children, disjoint and overlapping, with mixed `clip-rule`s, transformed |
| `nest/` | 5 | `clip-path` on the `clipPath`, on one child, on nested groups, a chain of three, and on a two-child `clipPath` |
| `edge/` | 13 | `display: none` and `visibility: hidden` children, an empty `clipPath`, a missing reference, a reference to a non-`clipPath`, a self reference, a two-element cycle, `clip-path` in `style`, a fractional page offset, `crispEdges` on the child and on the target, `opacity` with a clip, hairline children |
| `child/` | 2 | a `text` child (a named gap, T1) and a `use` child |
| `chart/` | 5 | Recharts' plot-area clip on a line, an area, bars, dots, and a draw-in clip |

Set `v4` (`--set v4`, fixtures in `tests/fidelity/fixtures/svg-paint-v4/`)
covers paint servers (V3b), with 103 cases, each at scale 1 and at scale 2.
Ids are prefixed as in `v3`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `lin/` | 10 | `linearGradient` horizontal, vertical and diagonal; fractions and percentages in `objectBoundingBox`; `userSpaceOnUse` lengths, percentages and percentages of a `viewBox`; a degenerate vector, padded and repeating |
| `stop/` | 12 | three stops, percentage offsets, unordered and out-of-range offsets, inner stops only, a hard edge, `stop-opacity` (and half), stop properties in `style`, `currentColor`, one stop and none |
| `spread/` | 4 | `pad`, `reflect` and `repeat`, and `repeat` along a diagonal |
| `xf/` | 4 | `gradientTransform` rotations (about the centre too), in user space, and with a transformed shape |
| `paint/` | 9 | a server as `stroke` (both units), with `fill-opacity` (an odd value too), on a circle, a path and a group, on fill and stroke, under `crispEdges` |
| `href/` | 7 | inherited stops and attributes, a chain, own stops winning, `xlink:href`, a cycle, a radial inheriting from a linear |
| `ref/` | 7 | a missing server (with and without a fallback), a reference to a non-server, an empty bounding box (with a fallback, and in user space), a stop-less gradient with a fallback |
| `rad/` | 18 | `radialGradient` in both units, wide boxes, focal points (on the edge and outside), `fr` (with a focus, larger than `r`), `r="0"`, `reflect`, `repeat`, transforms, strokes, circles, inherited focus, opacity |
| `pat/` | 28 | `pattern` tiles in both units (offsets, fractions), `objectBoundingBox` content, `viewBox` (`slice`, `meet`), `patternTransform`, overflow (fractional too), strokes, `fill-opacity`, circles, transformed shapes, stroked and gradient content, nested patterns, `href` (own content too), zero width (with a fallback), no content, a self reference, inherited style, a checkerboard |
| `chart/` | 4 | Recharts-shaped area and bars with gradient fills, a pie sector with a radial gradient, hatched bars |

Set `v5` (`--set v5`, fixtures in `tests/fidelity/fixtures/svg-paint-v5/`)
covers `mask` (V3c), with 58 cases, each at scale 1 and at scale 2. Most
mask the same content as `v3`. Ids are prefixed as in `v3`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `lum/` | 14 | luminance coverage: white, grey, black, red, green and blue, mixed colours, `fill-opacity` and `opacity` on the mask's child, a circle, a star, a stroke, overlapping children, a cut-out |
| `type/` | 4 | `mask-type="alpha"` (half-transparent too, and in `style`), explicit `luminance` |
| `region/` | 9 | the initial region against a wide stroke, `objectBoundingBox` fractions and percentages, `userSpaceOnUse` lengths at fractional positions, percentages and the initial values, zero and negative sizes, a rotated target |
| `units/` | 4 | `maskContentUnits="objectBoundingBox"` on a group, with a circle, with a child `transform`; both units in user space |
| `grad/` | 5 | a linear fade (horizontal, vertical), a radial spot, an alpha mask from `stop-opacity`, a striped pattern |
| `target/` | 5 | a mask on a rect, a circle, a stroked rect, a transformed group, a root at a fractional page offset |
| `nest/` | 5 | a mask on the mask's child, a `clip-path` on the mask's child, `clip-path` and `mask` on one element, `opacity` with a mask, nested masked groups |
| `edge/` | 10 | a missing reference, a reference to a non-`mask`, an empty mask, hidden and `display: none` children, a self reference, `mask` in `style`, `crispEdges` and hairline children, a line target (an empty bounding box) in both units |
| `chart/`, `icon/` | 2 | an area faded by a gradient mask, an avatar with a badge cut-out |

Set `v6` (`--set v6`, fixtures in `tests/fidelity/fixtures/svg-paint-v6/`)
covers `use` and `symbol` (V3d, first part), with 61 cases, each at scale 1
and at scale 2. Ids are prefixed as in `v3`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `use/` | 16 | a shape inheriting `fill` from the `use`; `x` and `y` (fractional, percentages) with and without a `transform`; the target's own `fill` winning; an inherited dashed stroke; `xlink:href`; two instances; a group; a nested `use`; `currentColor`; group `opacity`; a target after the `use` and one rendered in place; `fill` in `style`; a target `transform` |
| `effects/` | 6 | `clip-path` in both units and `mask` on a translated `use`, `visibility` on the `use` and overridden by the target, a `display: none` target |
| `symbol/` | 17 | a sized icon, no size (100%), percentage and fractional sizes, no `viewBox` (clipped to the viewport), `overflow="visible"`, the viewport clip, `meet`, `slice`, `none`, a `viewBox` origin, the symbol's own `width` and `height`, an empty `viewBox`, a `symbol` outside a `use`, a transformed instance, a scaled stroke, `clip-path` on the instance |
| `svg/` | 2 | a referenced `svg` with its own size and position, and with the `use`'s size replacing it |
| `clip/` | 10 | `use` in a `clipPath`: plain, with `x` and `y`, with transforms on the `use` and the target, `clip-rule` on the `use` and on the target, with a shape beside it, in `objectBoundingBox` units, of a `g` and of a `symbol`, under `crispEdges` |
| `edge/` | 8 | a missing reference, no `href`, a reference to an ancestor, a two-group cycle, a cycle inside an instance, a reference to a gradient, `href` beside `xlink:href`, `width` and `height` on a shape's `use` |
| `icon/`, `chart/` | 2 | an icon sprite at three sizes and colours, Recharts' legend icon |

Set `v7` (`--set v7`, fixtures in `tests/fidelity/fixtures/svg-paint-v7/`)
covers markers (V3d, second part), with 86 cases, each at scale 1 and at
scale 2. Most cases share three markers: an arrow whose tip is its
reference point, a dot, and a notched flag that shows a mirror or a
quarter turn. Ids are prefixed as in `v3`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `pos/` | 26 | start, mid and end markers on `line`, `polyline`, `polygon` and paths: relative commands, subpaths, closed subpaths (with an explicit closing line, a line after the close, two closed subpaths), cubics (with coincident and fully degenerate control points), quadratics, smooth curves, arcs (large, chained, zero radius), zero-length segments, a lone move, a reversal, sharp turns and a path error |
| `dir/` | 14 | an arrow at every vertex, so mid directions show: a polyline, subpaths, a polygon, mixed curves, reversals, zero-length segments (diagonal, mid-path, closing, curves), moves (two, a pair, a trailing one, a lone one) and a move after a close |
| `orient/` | 4 | a fixed angle, angles in `rad` and `turn`, `auto-start-reverse`, an invalid value |
| `units/` | 4 | `strokeWidth` at two widths, `userSpaceOnUse`, a shape without a stroke, a percentage stroke width |
| `size/` | 8 | the initial size, `viewBox` with `meet`, `slice` and `none`, a `viewBox` origin, zero and negative sizes, an empty `viewBox` |
| `ref/` | 3 | `refX` and `refY` in the `viewBox`, the SVG 2 keywords, percentages |
| `clip/` | 4 | the viewport clip of a rotated marker, `overflow` `visible` and `auto`, a `meet` viewport wider than the `viewBox` |
| `prop/` | 20 | inheritance from the marker, the `marker` shorthand in `style` and as an attribute, inherited marker properties, other basic shapes, missing and non-marker references, `context-fill` and `context-stroke`, group `opacity` on the shape and in the marker, `paint-order`, a transformed shape, `transform` on the marker, `display: none` on the marker, `visibility`, `clip-path` on the shape and in the marker, `use` in the marker, markers in a `use` instance, a marker that marks itself |
| `chart/`, `diagram/` | 3 | axis arrowheads, data dots on a series, a diagram's edges with arrowheads and circles |

**Evidence** is one capture per OS and set in
`tests/fidelity/fixtures/svg-paint-v1/` to `svg-paint-v7/`.
Each capture has a JSON file of cases and slots, plus one PNG per scale:

- `chromium-linux*`, from Linux CfT 147.0.7727.50 in WSL;
- `chromium-windows*`, from Windows CfT 147.0.7727.50.

The Linux and Windows sheets are pixel-identical, except for the text
in `v3`'s `child/text`.

`crates/div-svg/tests/paint_corpus.rs` paints every case through
`div_svg::paint` and compares each slot exactly. It rasterizes as Div's
renderer rasterizes a page: 256-px N32 premultiplied tiles tagged sRGB
(`create_raster_surface` in `div-paint`), stepping by 254, after cc's tiling as `div-paint`'s `render.rs` models it. Each case
is painted in the device space of each tile that shows it, on a full
256-px tile surface.

**Production raster tiling.** This is not a test-only model. `div-paint`
already rasterizes a page this way (`crates/div-paint/src/render.rs`):
`RASTER_TILE_SIZE_PX` = 256, `RASTER_TILE_STEP_PX` = 254, each tile drawn
on a full tile surface translated by `-tile_x, -tile_y`, and every tile
but the first cropping its leading row and column. The test repeats the
same rule standalone because `div_svg::paint` is not yet called from a
page. Layers no larger than 512 px whose content fits
(`should_replay_untiled`, after cc's `max_untiled_layer_size`) replay
untiled in both Chromium and Div; the corpus sheets are wider than that,
so it measures the tiled path only. Painted instead at the origin of a
surface the size of the case, 37 of the 147 captures differed.

**Why tiles matter.** Both details are needed for exact pixels.
- The tile translation matters because Skia strokes and maps points in
  single precision in device space. A case painted at the origin instead
  of at its place in the tile differs by a few levels along stroke edges.
- The tile size matters because analytic anti-aliasing takes a different
  route for a path that leaves the device.

To recapture:

```sh
# Linux, in WSL (and again with --set v2 to --set v7 into svg-paint-v2/ to svg-paint-v7/)
python3 tools/svg/paint_corpus.py capture   --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome   --output-prefix tests/fidelity/fixtures/svg-paint-v1/chromium-linux
# Windows
python tools/svg/paint_corpus.py capture   --chrome <main checkout>/out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe   --output-prefix tests/fidelity/fixtures/svg-paint-v1/chromium-windows
```

`DIV_SVG_PAINT_DUMP=<dir>` makes the test write Div's pixels for each
differing case.

### What `div_svg::paint` matches

All 147 `v1` captures (107 at scale 1, 40 at scale 2) and all 115 `v2`
captures (85 at scale 1, 30 at scale 2) are exact on Windows (MSVC) and on
Linux (WSL). Both `known-gaps.txt` files are empty.

Of the 108 `v3` captures, 106 are exact on both OSes. The other two are
the `text` child, a named gap in `svg-paint-v3/known-gaps.txt` (T1, and on
Windows the LCD text edging owned by the fidelity lane). The `use` child
is exact since V3d. The `clipPath` corpus settled these points; the clip itself is
in `src/clip.rs`, and the test's harness resolves references and paints
groups:

- A `clipPath` whose content is one path clips the canvas,
  anti-aliased. Several children are united in one Skia path-ops build
  (`OpBuilder`), each with its own `clip-rule`; uniting them one at a time
  differs in `union/mixed-rules`, and appending them into one path differs
  for every fill rule. One child's path is used as it is: simplifying it
  differs for the star polygons.
- With no nested `clip-path`, the content transform (the `clipPath`'s
  `transform` and the `objectBoundingBox` map) is applied by the canvas,
  not to the path: transforming the path first differs by two pixels in
  `units/obb-circle` at scale 2.
- A nested reference that resolves to nothing, or closes a cycle (a self
  reference, or `c` to `d` to `c`, broken at the reference back to `c`),
  rules out the path-based clip, and the mask chain stops there.
- The mask draws each child as the shape painter does, a rectangle or an
  oval as such, opaque. The content is drawn into a layer clipped to the
  `clipPath` content's bounds; the mask is composited onto it with `DstIn`
  and is itself unclipped. Skia's scan conversion depends on the clip a
  path crosses, so the bounds clip changes the stroke's edge pixels in
  `edge/self`, and clipping the mask too changes the circle's edge in
  `edge/cycle`. With this structure both are exact.
- `opacity` on a clipped group wraps the clip: the clipped content is
  drawn into the opacity layer.

Of the 206 `v4` captures, 203 are exact on Windows and 204 on Linux. The
others differ by one channel level in one or two pixels of a radial
gradient with a focal point or a repeat, at scale 2; they are listed in
`svg-paint-v4/known-gaps.txt` as rounding in Skia's raster pipeline, owned
by the fidelity lane with the setup track. `div_svg::paint_server` hands
Skia what cc does for each case, so the remaining difference sits below
Div's code. The test binaries initialize Skia at load
(`div_text::initialize_skia_at_load!`, as Chromium calls
`SkGraphics::Init`), so the raster pipeline runs the same CPU stages as
Chromium's; without it, 15 more captures differ on Windows and 16 on
Linux, among them the seam of `spread/repeat-diagonal`. The paint-server corpus
settled these points; the servers are in `src/paint_server.rs`, and the
test's harness resolves references and records pattern content:

- `PreparePaint` (BSD) paints a server black with its shader, the
  opacity quantized to 8 bits as the alpha; a server that does not apply
  falls back to the fallback colour, or to nothing. Gradients are always
  dithered (`Gradient::ApplyToFlags`, BSD `gradient.cc`).
- Gradients follow `gradient.cc` (BSD): the first stop is copied to 0 and
  the last to 1, `pad`, `reflect` and `repeat` are clamp, mirror and
  repeat, colours interpolate in sRGB, unpremultiplied, and a radial
  gradient is a two-point conical one from the focus (with `fr`) to the
  centre. A gradient with no stops paints transparent, not its fallback.
- Patterns follow `paint_record_pattern.cc` and `paint_shader.cc` (BSD):
  the tile is a recorded picture, repeated both ways, rasterized at
  scale with linear filtering. Nearest filtering differs in every scaled
  tile.
- Which attributes and children an `href` inherits, how
  `objectBoundingBox` units and an empty box resolve, and the pattern
  tile, content transform and local matrix are clean-room from SVG 2
  (Blink's resource code is LGPL): common attributes are always
  inherited, geometry only from the same kind of element, stops and
  pattern content only when the element has none.
- Channel values are converted as Blink converts them, `r / 255` in
  single precision. Multiplying by `1 / 255` changes the dithered pixels.

All 116 `v5` captures are exact on both OSes, and
`svg-paint-v5/known-gaps.txt` is empty. The mask painter is in
`src/mask.rs`; the test's harness resolves `mask` references and records
the mask's content. The mask corpus settled these points:

- The mask layer follows `PaintSVGMaskLayer` (BSD
  `svg_mask_painter.cc`): it is clipped to the mask region without
  anti-aliasing (with anti-aliasing every `region/` case differs, by up to
  162 levels), the content is drawn under the content transform, and a
  luminance mask goes through Skia's luma colour filter. It is composited
  onto the masked content with `DstIn`.
- The region and both units are clean-room from SVG 2 and CSS Masking
  (Blink's masker is LGPL): `x`, `y`, `width` and `height` in
  `maskUnits`, initially `objectBoundingBox` -10%, -10%, 120% and 120%; a
  region of zero or negative size, or an `objectBoundingBox` region on an
  empty bounding box, masks the element out.
- A reference that resolves to nothing, to an element that is not a
  `mask`, or back to a mask being resolved leaves the element unmasked
  (`HasSingleInvalidSVGMaskLayer`, BSD `css_mask_painter.cc`, and
  `edge/self`).
- The mask clip, the mask region united with the element's visual rect
  (`ResourceBoundsForSVGChild`), is ported but changes no capture in this
  corpus.

Of the 122 `v6` captures, 120 are exact on both OSes. The other two are
`chart/legend`, a named gap in `svg-paint-v6/known-gaps.txt`: the arcs of
Recharts' legend icon, whose radius is within a float ulp of half the
chord, differ at the stroke's inner edge. The same path drawn directly,
without `use`, differs the same way. The cause is unmeasured; a comparison
of Skia's `arcTo` and stroker is pending.
`src/use_element.rs` places an instance and gives a `symbol` or `svg` its
viewport (`src/viewport.rs`); the test's harness builds the instance tree.
`use` and `symbol` are clean-room (Blink's `use` element and its layout
container are LGPL). The corpus settled these points:

- An instance is a copy of the target that inherits from the `use`. Its
  own properties, `transform`, `display` and `visibility` come with it
  (`effects/visible-target`, `effects/display-none-target`).
- `translate(x, y)` follows the `use`'s `transform` and is part of the
  `use`'s own transform: a `clip-path` or `mask` on the `use` is in the
  translated space (`effects/clip-user`, `effects/mask`).
- A `symbol` or `svg` instance is a nested viewport. The `use`'s `width`
  and `height` replace the target's where set, and both are initially
  100%; the symbol's own size applies otherwise (`symbol/own-size`). It
  clips to the viewport, not to the `viewBox` (`symbol/meet`), unless
  `overflow` is visible. A zero-size `viewBox` disables it
  (`symbol/viewbox-zero`), as Blink's BSD container painter states.
- A `use` whose target is the `use` itself or one of its ancestors (a copy
  counting as its original) renders nothing. Only that `use` is dropped:
  the rest of the outer instance renders (`edge/mutual`,
  `edge/cycle-deep`).
- In a `clipPath`, a `use` of a shape contributes the shape under the
  `use`'s placement, with `clip-rule` inherited from the `use`. A `use`
  of a `g` or a `symbol` clips everything out (`clip/use-group`,
  `clip/use-symbol`).
- Group `opacity` is painted as cc plays it back
  (`PaintChunksToCcLayer`, `PlaybackFoldingIterator`, both BSD): the layer
  is bounded by its content's integer drawable bounds (unbounded, one
  edge pixel of `use/opacity` differs by 4 levels), and a layer holding a
  single drawing operation is folded into it (as a layer, every pixel of
  `use/twice`'s faded rect is one level off). `div-paint` applies opacity
  at the call site, and its `paint_record.rs` does the same folding.

Of the 172 `v7` captures, 167 are exact on both OSes. The other five are
a named gap in `svg-paint-v7/known-gaps.txt`: at scale 2, a few edge
pixels of an arrowhead differ by up to 8 levels, depending on where the
slot sits on the sheet (the same arrow is exact in another column). It is
in the single-precision rounding of the marker transform; the exact
pipeline is unmeasured. `src/marker.rs` places the markers; the test's
harness resolves the references and paints the content. Blink's marker
painting (`SVGShapePainter::PaintMarker`) is BSD and is followed; its
marker resource, marker element and marker position code are LGPL and
were not opened, so positions, directions and the marker transform are
clean-room. The corpus settled these points:

- Markers apply to `path`, `line`, `polyline` and `polygon` only
  (`prop/other-shapes`). There is a vertex at every move and at the end
  of every segment, a closing segment included. The first vertex takes
  `marker-start`, the last `marker-end`, and the others `marker-mid`; a
  lone move takes only `marker-end` (`pos/moveto-only`).
- Directions differ from SVG 2 in three ways. A zero-length segment
  points along the x axis instead of borrowing its neighbour's direction
  (`dir/zero-diagonal`, `dir/zero-close`). A move has a direction, from
  the current point (the origin, for the first) to its point: a vertex at
  a move points along the segment leaving it, or along the move when none
  does (`dir/moves`, `dir/move-pair`, `dir/lone-move`). A full reversal
  turns by -90°, the turn being taken in [-180°, 180°) (`dir/reversal`).
  Curve directions skip coincident control points, and arcs use the
  tangent of the centre parameterization.
- The marker transform is translate to the vertex, rotate, scale by the
  stroke width (for `strokeWidth` units, even without a stroke), then
  translate by minus the reference point mapped through the `viewBox`
  transform. It is concatenated onto the canvas in single precision, as
  Chromium's canvas does: composed in double precision instead, 6 more
  captures differ.
- The viewport clip is not anti-aliased (anti-aliased, 92 of the 172
  captures differ) and applies unless `overflow` is `visible` or `auto`.
- The content inherits from the marker and its ancestors, not from the
  shape (`prop/inherit`). `context-fill` and `context-stroke` take the
  shape's paints. The `marker` shorthand is a property only: as an
  attribute it does nothing (`prop/attr-shorthand`). A marker's
  `transform` is ignored, `display: none` on the marker does not stop it,
  and a hidden shape draws no markers (`prop/visibility`).
- `refX` and `refY` are lengths: the SVG 2 keywords are not supported and
  leave 0 (`ref/keywords`), and a percentage resolves against the shape's
  viewport, not the `viewBox` (`ref/percent`).
- A marker that refers to itself, directly or through its content, is
  not painted again (`prop/nested`).
- A shape's markers are inside its effects: `opacity` fades them as one
  layer with the shape (it no longer folds into a single draw), and the
  layer's bounds include the markers' viewports (`prop/opacity`).

The corpus settled these points:

- A square-cornered `rect` is drawn with `drawRect`, and a `circle` or
  `ellipse` with `drawOval`, on the object bounding box
  (`SVGShapePainter`). Everything else, rounded rects included, is drawn
  as its path.
- The colour's alpha is multiplied by the opacity in single precision
  (`PreparePaint`), and anti-aliasing is on.
- An inline root is translated to its pixel-snapped border box. The
  snapping scale adjustment (`SvgInlineRootPixelSnappingScaleAdjustment`)
  is a test-only feature in 147, so it does not apply.
- A nested `svg` clips to its viewport with an anti-aliased clip, which
  `nested/svg-half` (viewport edges on half pixels) checks.
- `paint-order` and `shape-rendering` follow `SVGShapePainter` (BSD):
  the layers are painted in the listed order, and `crispEdges` and
  `optimizeSpeed` turn anti-aliasing off for fill and stroke alike.
- Dashes go through `StrokeData::SetLineDash` (BSD). Which arrays draw is
  clean-room, because Blink's resolution is in LGPL `svg_layout_support`:
  an array with a negative entry or summing to zero draws solid,
  percentages resolve against the normalized diagonal, and dashes start
  where the shape's Skia path starts (a `rect` at its top-left corner, an
  ellipse at its rightmost point), clockwise.
- `pathLength` scales the dash array and offset by Skia's contour length
  of the shape over `pathLength`. It is not Chromium's `getTotalLength`
  traversal for `<path>` (90.766 for `dash/pathlength-path`'s curve): with
  that length the curve differs by 136 pixels, and with Skia's (90.656)
  it is exact. A negative `pathLength` is ignored and zero strokes solid.
- `stroke-dasharray` and `stroke-dashoffset`, even as presentation
  attributes, are read as CSS reads numbers: in double precision, stored
  as floats. With `div-svg`'s single-precision scanner,
  `dash/pathlength-path`'s `0.05 0.03` differs by three pixels. The test
  reads them that way (`css_length` in `tests/support/mod.rs`); the engine
  owns the parse (E2).
- A non-scaling stroke is drawn as `SVGShapePainter` draws it (BSD): the
  path mapped into the host space, on a canvas carrying the inverse. The
  host space is clean-room, because layout computes it in LGPL
  `layout_svg_shape`: it is the outer `svg`'s own coordinates before its
  paint offset and zoom. With the full device transform instead, the
  scale-2 cases differ by up to 938 pixels, because the stroke loses its
  zoom, and two scale-1 cases by a few levels.

**Not claimed.**
- Anything about the page path. The engine does not call the painter
  yet; that needs E1 to E3 and F1, then the `div-paint` call sites.
- The untiled path (layers of 512 px or less) for SVG content: the
  corpus pages are tiled.
- Text, element and group `opacity` (applied by `div-paint` at the call site),
  and scales other than 1 and 2.
- For clipping: `text` children of a `clipPath`, `clip-path:
  url()` on HTML elements (engine item E4), basic shapes and geometry
  boxes in `clip-path`, and the reference resolution and group painting
  that the test harness models until the engine calls the painter.
- For paint servers: servers on text with `objectBoundingBox` units,
  which need the text's bounding box with its glyph ink (V2b, `getBBox`);
  they fall back for now. `userSpaceOnUse` servers on text use the text
  painter's shader transform. Also `context-fill` and `context-stroke`,
  `meshgradient` and `hatch`, and colour interpolation other than sRGB.
- For masks: `text` and `use` children of a `mask`, `mask` on HTML
  elements and CSS `mask-image` with images or several layers (engine
  work after E1), `mask-mode`, `foreignObject` zoom, and
  `color-interpolation` on the mask.
- For `use` and `symbol`: `context-fill` and `context-stroke`, references into other documents, `use` of `text`,
  CSS selectors matching inside an instance, `symbol`'s own `x` and `y`,
  and the opacity fold for a paint server's shader (the harness folds
  colour paints only).
- For markers: the scale-2 rounding gap above, `context-fill` and
  `context-stroke` that resolve to a paint server, marker scaling under
  `vector-effect: non-scaling-stroke`, markers on shapes inside a
  `clipPath`, and markers referenced across documents.

## Text corpus (V2a positions)

`text_corpus.py list` prints the 131 cases. `capture` loads them in
headless Chromium over CDP, once at device scale 1 and once at real-zoom
scale 2 (`--force-device-scale-factor`), with the vendored Geist and Inter
variable fonts (OFL) loaded as data URLs. Each case is one `<svg>` in its
own slot. The output is `div-svg-text-corpus-v1`. For every `<text>` it
records:

- `getNumberOfChars()` and `getComputedTextLength()`;
- for each addressable character, `getStartPositionOfChar`,
  `getEndPositionOfChar`, `getExtentOfChar` and `getRotationOfChar`;
- `getCTM()`;
- `getBBox()`, which is recorded but not compared. It needs ink bounds,
  which are V2b.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `run/` | 31 | Geist and Inter at many sizes and weights, digits, punctuation, kerning, a ligature, italic, fractional positions |
| `list/` | 17 | `x`, `y`, `dx`, `dy` and `rotate` lists: short, long, em and percent lengths, CSS number syntax |
| `tspan/` | 18 | nested spans, list inheritance, kerning across span boundaries, font changes inside a run |
| `baseline/` | 18 | every `dominant-baseline` and `alignment-baseline` value, `baseline-shift` (`sub`, `super`, lengths, percentages, nesting) |
| `anchor/` | 10 | `text-anchor` per chunk, chunks started by `x` and `y` |
| `xf/` | 13 | transforms (scale, non-uniform scale, skew, rotation) and `viewBox` scaling, which change the font's scaled size |
| `spacing/` | 7 | `letter-spacing` and `word-spacing` in px and em |
| `ws/` | 7 | white-space collapsing within and across elements, newlines, tabs, no-break spaces, `white-space: pre` |
| `chart/` | 10 | axis ticks, rotated ticks, labels, legends and tooltips as Recharts draws them |

**Evidence** is one capture per OS in `tests/fidelity/fixtures/svg-text-v1/`
(`positions-windows.json`, `positions-linux.json`), from pinned CfT
147.0.7727.50. The two differ, because Windows and Linux rasterize and
measure glyphs differently, so each OS is checked against its own capture.

To recapture:

```sh
# Linux, in WSL
python3 tools/svg/text_corpus.py capture \
  --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome \
  --output tests/fidelity/fixtures/svg-text-v1/positions-linux.json
# Windows
python tools/svg/text_corpus.py capture \
  --chrome <main checkout>/out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe \
  --output tests/fidelity/fixtures/svg-text-v1/positions-windows.json
```

### What `div_svg::text` matches

`crates/div-svg/tests/text_corpus.rs` compares 24,056 numbers per OS
exactly, as f64. All 131 cases match at both scales on both OSes.

Two precision behaviours in `div-text` closed the last gaps: T4, the font
size truncated to 1/100 device px, and T3, glyph advances truncated to
1/65536 px as Blink hands them to HarfBuzz (SkShaper rounds them). A
future gap is listed in `known-gaps.txt` with an `<os>` column, and
`div-svg` does not work around it. Set
`DIV_SVG_TEXT_DEBUG=<case prefix>` to print a case's numbers.

## Text pixels (glyph painting)

`text_corpus.py pixels` renders the 131 position cases and 14 paint cases
(`paint/`) as pixels, 145 in all. Each case gets its own slot on one sheet
per device scale (1, and 2 at real zoom), and the sheet is rendered through
`render_chrome` once the web fonts have loaded. `list --pixels` prints the
cases. The output is `div-svg-text-pixels-v1`:
`pixels-<os>.json` plus `pixels-<os>-<scale>x.png` in
`tests/fidelity/fixtures/svg-text-v1/`.

| Group | Cases | What it pins down |
| --- | --- | --- |
| `paint/` | 14 | `stroke` with fill, alone, wide and round-joined, `paint-order="stroke"`, `fill-opacity` and `stroke-opacity`, a dashed stroke, a stroke under `scale(1.5)` and its `non-scaling-stroke` counterpart, a stroked `tspan`, `currentColor`, `shape-rendering="crispEdges"` (ignored for text), 96 px glyphs, stroked `rotate` glyphs |

To recapture:

```sh
# Linux, in WSL
python3 tools/svg/text_corpus.py pixels \
  --chrome ~/openui-oracle/chrome/linux-147.0.7727.50/chrome-linux64/chrome \
  --output-prefix tests/fidelity/fixtures/svg-text-v1/pixels-linux
# Windows
python tools/svg/text_corpus.py pixels \
  --chrome <main checkout>/out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe \
  --output-prefix tests/fidelity/fixtures/svg-text-v1/pixels-windows
```

### What `div_svg::text::paint` matches

`crates/div-svg/tests/text_corpus.rs` paints every case with
`div_svg::text::paint`, rasterizing as Div's renderer does (white 256-px
tiles stepping by 254, an RGB-stripe LCD surface with gamma 1.2 and
contrast 0.2, the `chromium_linux_lcd` raster configuration), and compares
each slot byte for byte.

On Linux, 264 of 290 captures are exact, including every 1x stroke,
opacity, dash and `paint-order` case. Since T5 and T6, a web-font run takes
its mask and hinting from the size and the matrix it is drawn under, as
Chromium's Fontations scaler does: grayscale above 48 device px, and
unhinted glyphs unless the matrix is axis-aligned. The other 26 are named
gaps in `pixel-gaps.txt`, all owned by `div-text` in the engine lane:

- **T6 residue, rotated and skewed glyphs (22).** Unhinted, these cases
  dropped to a few edge pixels (`list/rotate` at 1x: 483 to 66), but are
  not exact. The cause is not measured yet.
- **T7, non-uniform scale (4).** `xf/scale-nonuniform` and `xf/viewbox-none`
  have exact positions since T4, but their glyphs still differ. The cause
  is not measured yet; the engine lane measures it after T5 and T6.

On Windows, no capture is exact yet. Both sides paint LCD glyphs, and 282
of the 290 cases differ only in tone, by at most 16 per channel. That is
Windows text rasterization, which the fidelity lane owns. Set
`DIV_SVG_TEXT_DUMP=<dir>` to write Div's pixels for each differing case.

## Lucide capture plan (V1 stage 1 gate)

**Icons.** The components track vendors 20 icons from `lucide-static`
1.70.0 (ISC) in `tests/fidelity/components/assets/icons/lucide/`.
Together they use only `svg`, `path` (26), `circle` (13) and `rect`
(1). Every one has the same root presentation attributes:

- `fill="none"` and `stroke="currentColor"`;
- `stroke-width="2"`;
- `stroke-linecap="round"` and `stroke-linejoin="round"`;
- `viewBox="0 0 24 24"`.

So stage 1 needs presentation attributes and `currentColor` with no
cascade work.

**Matrix.** One fixture page per variant. Each page uses the census
profiles and the components harness's raster configuration
(`chromium-linux-lcd`):

| Axis | Values |
| --- | --- |
| icon | the 20 vendored icons |
| embedding | inline `<svg>`; `<img src=icon.svg>` (static-image path, same painter) |
| size | 16 px (shadcn `size-4`), 24 px (natural) |
| position | integer origin; origin offset by 0.5 px (anti-aliasing across pixel edges) |
| colour | `#09090b` on white; `#fafafa` on `#09090b` |
| device scale | 1, 2 |

That is 20 × 2 × 2 × 2 × 2 × 2 = 640 Chromium captures.

**Capture.** It extends `tests/fidelity/components/harness/reference_harness.py`
rather than adding a new launcher. The fixture HTML is generated from the
vendored files. Chromium screenshots each page through CDP at the
profile's viewport and scale. Div renders the same fixture IDs through
its renderer binary, and the comparison is exact, as in `compare`. The
report gives exact captures out of 640, with every non-exact capture
listed by its differing pixel count and maximum channel difference.

**Extended set (after the gate).** sample-git's icon list, and then
every icon in `lucide-static` 1.70.0 at 24 px and scale 1, 2. That needs
the package installed under `tests/fidelity/components/reference`, which
is a new npm download, so ask the parent first.

**Not claimed until measured:** any exactness for icons outside the
captured matrix, and any `<img>` result before the static-image path
moves onto the new painter.
