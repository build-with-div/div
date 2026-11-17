# SVG lane tools (RFC 0009)

Tools for the inline SVG work in RFC 0009
(`docs/alpha/rfcs/0009-inline-svg.md`).

| Tool | Purpose |
| --- | --- |
| `licence_audit.py` | Classifies Chromium source files by licence header only (BSD, LGPL, GPL, other). Prints the class and the first copyright line; never prints or stores code |
| `geometry_corpus.py` | Black-box geometry oracle: queries pinned Chromium through web APIs only, for the parts RFC 0009 writes clean-room |
| `text_corpus.py` | Black-box text-position oracle for `div_svg::text`: the SVG text DOM APIs in pinned Chromium, at scale 1 and at real-zoom scale 2 |
| `paint_corpus.py` | Pixel oracle for `div_svg::paint`: screenshots of pinned Chromium painting each case in its own slot, at scale 1 and at real-zoom scale 2, in two sets (`--set v1`, `--set v2`) |

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

**Evidence** is one capture per OS and set in
`tests/fidelity/fixtures/svg-paint-v1/` and `svg-paint-v2/`.
Each capture has a JSON file of cases and slots, plus one PNG per scale:

- `chromium-linux*`, from Linux CfT 147.0.7727.50 in WSL;
- `chromium-windows*`, from Windows CfT 147.0.7727.50.

The Linux and Windows sheets are pixel-identical.

`crates/div-svg/tests/paint_corpus.rs` paints every case through
`div_svg::paint` and compares each slot exactly. It rasterizes as Div's
renderer rasterizes a page: 256-px `raster_n32_premul` tiles, stepping by
254, after cc's tiling as `div-paint`'s `render.rs` models it. Each case
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
# Linux, in WSL (and again with --set v2 into svg-paint-v2/)
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
- Markers (a later clean-room PR), paint servers, clipping, text,
  element and group `opacity` (applied by `div-paint` at the call site),
  and scales other than 1 and 2.

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
exactly, as f64. On Linux, 129 of 131 cases match at both scales. On
Windows, 123 match at scale 1 and 126 at scale 2.

Every remaining difference is one of two precision behaviours in
`div-text`, which the engine lane owns:

- **T3.** Chromium truncates glyph advances to 1/65536 px; `div-text`
  rounds them. This shows only on Windows.
- **T4.** Chromium shapes at the font size truncated to 1/100 px;
  `div-text` uses the exact size.

Both were confirmed by experiment. With them applied, every number
matches on both OSes. They are listed in `known-gaps.txt` with an `<os>`
column, and `div-svg` does not work around them. Set
`DIV_SVG_TEXT_DEBUG=<case prefix>` to print a case's numbers.

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
