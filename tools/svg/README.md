# SVG lane tools (RFC 0009)

Tools for the inline SVG work in RFC 0009
(`docs/alpha/rfcs/0009-inline-svg.md`).

| Tool | Purpose |
| --- | --- |
| `licence_audit.py` | Classifies Chromium source files by licence header only (BSD, LGPL, GPL, other). Prints the class and the first copyright line; never prints or stores code |
| `geometry_corpus.py` | Black-box geometry oracle: queries pinned Chromium through web APIs only, for the parts RFC 0009 writes clean-room |

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
