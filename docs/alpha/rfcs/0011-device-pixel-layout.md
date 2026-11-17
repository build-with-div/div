# RFC 0011: Layout in device pixels at scales other than 1

Status: proposed, 16 November 2026. Owner: the engine track. Consumers:
the framework track (every geometry value `divui` reads from the engine),
the components track, the canvas and SVG lanes, the fidelity lane and the
samples.

Context: RFC 0010 moved every oracle at a scale other than 1 to real zoom
(`--force-device-scale-factor`), on the user's decision of 16 November
2026 ("Switch to real zoom"). This RFC is the engine work that closes the
difference that switch exposed. Nothing is implemented before it is
approved.

## Problem

At a device scale other than 1, Chromium implements the scale as zoom
(zoom for DSF). The scale multiplies every computed length, so layout and
paint work in device pixels. It then divides geometry by the zoom where
script reads it. Div lays out in CSS px with `LayoutUnit` at 1/64 CSS px.
It snaps paint in CSS px and replays the recorded paint under a scale
transform. A few device-aware exceptions exist, such as fragmentainer clips
(`RasterSnapping`) and native scrollbar frames.

The two models give the same frames under CDP emulation. They differ under
real zoom.

**The census at real zoom.** The first real-zoom census is in
`archive/docs/progress/evidence/real-zoom-baseline-2026-11-16/` in
build-with-div/div-fidelity-data at
[c82502d](https://github.com/build-with-div/div-fidelity-data/tree/c82502def3b921d2d46a6b2b4b2ca60ebaa6f843/archive/docs/progress/evidence/real-zoom-baseline-2026-11-16).
Div's frames are byte-identical between the two runs, so every change is
the oracle.

| Profile | Exact, emulated oracle | Exact, real zoom |
| --- | --- | --- |
| 1x | 5,888 | 5,888 |
| 2x | 5,767 | 5,153 |
| 1.25x | 5,344 | 2,183 |
| 1.5x | 5,540 | 3,063 |
| All of 23,728 | 22,539 | 16,287 |

CSS Grid coverage fell from 2,227 to 1,459 exact.

**What the new differences are.** Of the census rows that were exact under
emulation, 6,277 differ at real zoom. A classifier read each row's Div and
Chromium frames and sorted every differing pixel two ways:
- **Geometry or edge.** A pixel is geometry when its 3×3 neighbourhood is
  one colour in both frames. Otherwise it is an edge pixel.
- **Which engine blends.** At each edge pixel, the classifier checks which
  frame paints an endpoint colour and which paints an anti-aliased one.

The script and per-row results are listed under Migration, step 1.

| Profile | Edge only | Some geometry | Div blends where Chromium is crisp |
| --- | --- | --- | --- |
| 1.25x | 2,938 | 233 | 2,765 |
| 1.5x | 2,165 | 322 | 1,642 |
| 2x | 529 | 90 | 21 |
| Census total | 5,632 (90%) | 645 (10%) | 4,428 |
| CSS Grid coverage (771 rows) | 654 (85%) | 117 (15%) | 532 |

- **Edge-only rows.** These differ only within about one device pixel of an
  edge. Device-pixel snapping would give this, and so would a layout or
  glyph change smaller than a device pixel.
- **Rows with geometry pixels.** An area moved or changed size by more than
  a device pixel. Snapping cannot do that; lengths resolved in device
  pixels can. So **paint snapping explains at most 90% of the rows (5,632),
  and at least 10% (645) need layout or text in device pixels.**

The samples show all three mechanisms:

1. **Snapping position.** At 1.25x, a box edge falls at a fractional
   device position. Chromium paints it crisp on a device pixel; Div blends
   it. Example: `css/CSS2/floats/crashtests/float-rewind-parallel-flow-2-crash`.
2. **Snapping granularity.** At 2x both engines are crisp, but edges are
   one device pixel (0.5 CSS px) apart. Div snaps to CSS pixels, which at
   2x is every other device pixel. Example: `floats-wrap-bfc-outside-001`.
3. **Lengths in device pixels.** In the same crash test at 2x, a border
   line is 3 device px in Chromium and 6 in Div. The border is `medium`
   under two nested `zoom: 0.1`, so 0.03 CSS px. Chromium raises it to one
   device pixel. Div raises it to one CSS pixel, two device pixels.

**Other measurements agree.**
- **ResizeObserver.** The framework track's root fixture
  (`tests/fidelity/fixtures/resize-observer-root-v1/chromium-windows.json`,
  #57) records Chromium at 1.25x:
  - a 1px border is 0.8 CSS px, so the border box is 31.59375 tall, not 32;
  - the scrollbar is 15.2 CSS px (19 device px), so the scroll case is
    184.796875 wide, not 185.
- **Web fonts.** In the real-zoom recapture of the web-font fixtures
  (build-with-div/div#62), 11 of 14 non-exact rows have identical glyph
  rasters, and the baseline is one device pixel off. Blink rounds
  baselines in device pixels.

**Why the CSS-px model cannot get there.** Div's `LayoutUnit` is 1/64 CSS
px. At 1.25x, one device pixel is 0.8 CSS px, or 51.2 units, so a
device-pixel length is not representable. Blink's lengths are whole
1/64-device-pixel units. The rounding Chromium applies (borders, scrollbar
thickness, snapped sizes, baselines) happens in a space Div does not lay
out in. Patching each case with a device-scale branch would add code Blink
does not have, and would still round in the wrong unit.

Blink's rules, read in BSD-licensed sources at 147 (headers checked
first):
- `ConvertBorderWidth` (`core/css/resolver/style_builder_converter.cc`):
  a width in (0, 1) becomes 1; anything larger is floored. Both happen
  after zoom, so the result is whole device pixels.
- `ConvertOutlineOffset`: the same rule.
- `SnapSizeToPixel` (`platform/geometry/layout_unit.h`) and
  `ToPixelSnappedRect` (`core/layout/geometry/physical_rect.h`): these
  snap in whatever space the `LayoutUnit`s are in. Under zoom for DSF,
  that is device pixels.

## Proposal

**Adopt Blink's model: the device scale is a zoom.** Layout and paint run
in device pixels, and the engine's public geometry stays in CSS px.

- **Style.**
  - Implement CSS `zoom`: the effective zoom is inherited and multiplied,
    and computed absolute lengths are multiplied by it.
  - Each property follows Blink's rounding: border widths and outline
    offsets in whole pixels at least 1, and font size zoomed.
  - The document's root effective zoom is the device scale. Div has no
    `zoom` today: `css/css-viewport/zoom` and tests such as the crash test
    above use it.
- **Layout** is unchanged code. It runs on zoomed styles, so its
  `LayoutUnit`s are 1/64 device px. Native scrollbar thickness comes from
  the scrollbar metrics at the scale: 19 device px at 1.25x.
- **Paint.**
  - The existing Blink-shaped snapping (`SnapSizeToPixel`,
    `pixel_snapped_rect` and the border contour snapping) now snaps device
    pixels.
  - Recorded paint replays at scale 1.
  - The device-scale special cases (`RasterSnapping`,
    `snapped_native_scrollbar_frame`, `native_following_inline_origin`)
    are removed where the general path covers them. Each removal is
    checked against the census.
- **Text.** Fonts are shaped at the zoomed size, and line boxes and
  baselines are rounded in device pixels. This should close the #62 rows
  and the Inter 400 line-box item. That item stays clean-room: Blink's
  `font_description` sources are not read.
- **API boundary.** Everything the engine exposes in CSS px is divided by
  the effective zoom: client, offset and scroll geometry, bounding and
  client rects, ResizeObserver boxes, hit-test and event coordinates, and
  `divui` layout reads. Inputs (pointer positions, scroll offsets and
  deltas) are multiplied by it, as in Blink. `devicePixelRatio` and
  ResizeObserver's `device-pixel-content-box` keep their device values.
  The framework track sees no API change: values it reads at scale 1 are
  unchanged, and values at other scales become Chromium's.
- **Scale 1.** The root zoom is 1, so no length, rounding or frame changes.
  The 1x census must stay byte-identical, except for tests that use the
  CSS `zoom` property in stage 1.

**Considered and not chosen: snapping paint in device pixels while layout
stays in CSS px.**
- Paint would scale each position to device space, then snap. That is
  cheaper, and it could reach at most the 5,632 edge-only census rows;
  4,428 is a closer estimate, the rows where Div blends and Chromium is
  crisp.
- It cannot reach the 645 rows that need device-pixel lengths.
- Its code would become dead once layout moves to device pixels.
- It is the fallback only if stage 2 below proves too large for alpha.1.

## Stages and their expected rows

| Stage | Change | Census rows it targets (of the 6,277) | Measured by |
| --- | --- | --- | --- |
| 1 | CSS `zoom` in style, per-property rounding after zoom. No device scale change | None at 1.25x, 1.5x or 2x. At 1x, census and coverage rows that use `zoom` | the 1x census, `css/css-viewport/zoom` tests |
| 2 | Device scale as root zoom: layout and paint in device px, replay at scale 1, the API boundary | The 5,632 edge-only rows (snapping comes with the change) and the part of the 645 that are lengths: borders, outline offsets, scrollbars, `LayoutUnit` rounding | the full census and coverage at the real-zoom oracle, against the c82502d baseline |
| 3 | Text in device px: shaping at the zoomed size, rounded line metrics | What stage 2 leaves among the text rows, #62's 11 baseline rows, the Inter 400 line-box item | the census, the #62 web-font fixtures |

The split of the 645 rows between stages 2 and 3 is not measured yet.
Stage 2's census measures it.

## Affected tracks

- **Framework.** No API change. Its fixtures at scales other than 1 move
  from Div's current values to Chromium's. The ResizeObserver
  `device-scale-edges` gap pinned in #57 is expected to close in stage 2,
  and that test is updated there.
- **Components.**
  - No API change.
  - Its real-zoom reference evidence is re-measured after stages 2 and 3.
- **Canvas and SVG lanes.** Paint replays at scale 1 inside a
  zoomed layout:
  - a canvas bitmap is drawn into a device-pixel box;
  - SVG gets a zoomed root transform.
  Each lane's oracles at scales other than 1 are rerun after stage 2.
- **Fidelity lane.** None: the oracle is already real zoom. Census
  receipts follow RFC 0010.
- **Samples.** None beyond re-measuring.

## Migration

1. **This RFC, docs only.** The classifier script and per-row results go
   into the data repository with this RFC. The parked 205-row
   layout-at-DSF experiment (emulated oracle) stays closed; this RFC
   replaces it.
2. **Stage 1**, one engine PR: CSS `zoom`, with its WPT tests, the 1x
   census and coverage. D4 is measured against the real-zoom baseline.
3. **Stage 2**, one engine PR, or two if the API boundary splits cleanly
   from layout:
   - the full census and coverage at all four profiles, under the census
     lock;
   - every row lost at any scale is reported and fixed before merge (D4);
   - the 1x frames are byte-identical apart from stage 1's rows.
4. **Stage 3**, one engine PR for text.

Every comparison stays exact; no tolerance is added. `main` stays green
because each stage is a complete model change measured by the full census.
No stage needs a feature flag or a second code path.
