# Engine track progress (Workstream A)

Owner: engine track lead. Plan: `docs/alpha/plan.md`, Workstream A. Each
entry names the PR, what changed, the verification command and its result.
Results here are from this branch's tests unless marked otherwise.

## Status

| Unit | State |
| --- | --- |
| RFC 0001 typed style rules | Merged, PR #89 (`b8dcfaf8`) |
| A1 typed style rules | In progress. Step 1, `view!` emits typed constructors: [record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/typed-style-constructors-2026-11-09.md). Step 2, `style!`: [record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/style-macro-2026-11-10.md). Step 3, selectors and Blink specificity: [record](../selector-specificity-2026-11-10.md). Step 4, matching on the element tree: [record](../selector-matching-2026-11-10.md). Step 5, interaction pseudo-classes: [record](../state-pseudo-classes-2026-11-10.md). Step 6, form pseudo-classes: [record](../form-state-pseudo-classes-2026-11-10.md). Next: `:invalid`, `:open` and `:modal`, then `stylesheet!` and `css!` |
| A2 colors (`oklch()`, `color-mix()`) | Merged, PR #95 (`ee46e350`); 54/54 Chromium swatches exact; [record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/color-functions-2026-11-09.md) |
| A2 tokens and media | Not started; needs RFC 0002 |
| A3 top layer and dialog | Not started |
| A5 performance baseline | First internal baseline taken 9 November on one shared Windows host, outside the repository; not publishable. Finding: each dashboard relayout costs about 205 ms, all in layout; not yet profiled |
| A6 real-font baseline | Not measured; fonts are the components track's C4 files (PR #94). WOFF2 decoding on every platform merged, PR #97 (`c4b2e42e`; [record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/woff2-decode-2026-11-09.md)) |
| RFC 0008 Canvas 2D | RFC merged (build-with-div/div#43). Step 2, core state and paths in `div-canvas`: 54 of 54 trace canvases and 108 of 108 observations exact against Chromium; [record](../canvas-core-2026-11-15.md). Step 3, text: 68 of 68 canvases and 201 of 201 observations exact on Windows, 67 of 68 and 201 of 201 on Linux (one named gap); [record](../canvas-text-2026-11-21.md). Readback: 74 of 75 canvases and 258 of 258 observations exact on Linux (the same gap), Windows checked by CI; [record](../canvas-readback-2026-11-22.md). Gradients: 176 of 177 canvases and 381 of 381 observations exact on Linux (the same gap), Windows checked by CI; [record](../canvas-gradients-2026-11-22.md) |

## Findings while writing RFC 0001

- `pointerenter` and `pointerleave` already exist: `Document::dispatch_pointer_event`
  (`crates/divui/src/document.rs`) dispatches them target-only from the
  engine's hover-path diff. The plan's "no pointer enter or leave events" is
  stale; the remaining work is restyle on hover change, events on window exit
  and re-hover after scroll or layout.
- Window exit loses events: `PointerPhase::Leave` maps to
  `PointerEventKind::Cancel` (`crates/divui/src/app.rs`), and
  `Engine::pointer_event` still hit-tests the last position for `Cancel`, so the
  hovered path loses hover without receiving `pointerleave`. Owned by A1;
  fixed on 10 November ([record](../pointer-window-leave-2026-11-10.md)).
- `Engine::is_active` matches only the pressed element; Chromium's `:active`
  also covers its ancestors. Owned by A1.

## Reported by the components track (9 November 2026)

Not yet reproduced by the engine track; from `docs/progress/tracks/components.md`
on branch `components/c4-assets` and the shared findings note.

- WOFF2 faces fail to register on Linux with `MalformedFont`:
  `platform_font_data` in `crates/div-text/src/font/collection.rs`
  passes WOFF2 bytes to FreeType undecoded under `#[cfg(not(windows))]`; the
  census FreeType has no Brotli. Windows decodes with `wuff` first, and
  Chromium always decodes. Owner: `div-text`. Fixed by PR #97.
- In a column flexbox, a text item with an installed real font lays out one
  word per line (7 words: 140px tall instead of 20px). Owner: layout or text
  intrinsic sizing. Unmeasured here.
- Real-font glyphs sit at a different height from Chromium's, with the same
  horizontal extents. Owner: `div-text` metrics. Unmeasured here.

## Open text gaps found by the multi-run width oracle (22 November 2026)

Both are on main at `4e69bdf`, with the same results before and after the
multi-run width fix. Each has its own PR to come, and each is replayed and
pinned by `cargo test -p div-canvas --test multi_run_width`, so a change in
either fails. Fixture: `tests/fidelity/fixtures/multi-run-width-v1`, captured
from Chromium 147.0.7727.50. Counts below are Linux. The Windows captures are
in the fixture, but Div has not been compared with them yet.
Compared on 24 November at `691b956` with `main`'s Windows Skia flags: on
Windows, too, 112 of 112 Arabic widths differ, and the right-to-left
canvases differ by 3,740, 1,512, 1,512 and 743 pixels.

**Letter spacing on cursive scripts.** Owner: `div-text`.

- Source: Blink adds no letter spacing to a run in a cursive script:
  `IsCursiveScript` (`third_party/blink/renderer/platform/fonts/shaping/shape_result.cc:944`)
  is passed to `ShapeResultSpacing::ComputeSpacing`
  (`shape_result_spacing.cc:129`). Div's `apply_spacing`
  (`crates/div-text/src/shaping/shaper.rs`) adds it to every character, and
  its runs carry no script.
- Repro: `font = "33.3px R, S"` (DejaVu Serif, DejaVu Sans),
  `direction = "rtl"`, `letterSpacing = "1.7px"`,
  `measureText("\u05D0\u0628\u05D0").width`: Div 80.96095275878906, Chromium
  79.26095581054688, one spacing for the one Arabic letter.
- Measured: 112 of 112 widths of Arabic words with letter spacing differ (four
  spacings, two sizes, fourteen words).

**Right-to-left text with font fallback paints differently.** Owner:
`div-text`.

- Source: Blink stores a right-to-left result's runs in visual order
  (`ShapeResult::InsertRun`). Div's `splice_fallback_runs`
  (`crates/div-text/src/shaping/shaper.rs`) sorts them by ascending
  `start_index`. This is the likely cause and has not been proven.
- Repro: a 320 by 60 canvas, `font = "33.3px A, S"` (Ahem, DejaVu Sans),
  `direction = "rtl"`, `fillText("\u05D0.", 310, 42)`.
- Measured, pixels of 19,200 that differ (maximum channel delta 255):
  `alternating` 3,514, `box-letter` 1,443, `letter-box` 1,443,
  `letter-box-letter` 717. `hebrew-only` (no fallback) and `box-letters-box`
  (symmetric) match.

## Text gaps left by the T8 sweep (24 November 2026)

Owner: `div-text` (engine track), assigned by the orchestrator on 24 November.
No work on them is planned yet. Source: a `measureText` canvas trace with
`font = "33.3px P, Q"` (P is `Ahem.ttf`, Q is `DejaVuSans.ttf`) and
`wordSpacing = "7.3px"`, Chromium 147.0.7727.50 on Linux. The values are the
same on `main` `2f8af21`, T8 `3a58` and the multi-run width fix `2cee39d`.
Only the fields that differ are listed.

| Case | Text | Field | Div | Chromium |
| --- | --- | --- | --- | --- |
| #4 | two spaces | `actualBoundingBoxLeft` | 0 | -33.29682922363281 |
| #4 | two spaces | `actualBoundingBoxRight` | 0 | 33.29682922363281 |
| #6 | U+00A0 `a` | `width` | 66.59365844726562 | 73.89364624023438 |
| #6 | U+00A0 `a` | `actualBoundingBoxLeft` | -33.29682922363281 | -40.59681701660156 |
| #6 | U+00A0 `a` | `actualBoundingBoxRight` | 67.29682922363281 | 74.59681701660156 |
| #7 | `a` U+00A0 `b` | `width` | 99.89048767089844 | 107.19047546386719 |
| #7 | `a` U+00A0 `b` | `actualBoundingBoxRight` | 100.59365844726562 | 107.89364624023438 |
| #8 | U+00A0 | `width` | 33.29682922363281 | 40.59681701660156 |

Likely causes, not proven:
- #6, #7 and #8: each Chromium value is exactly 7.3px (the `wordSpacing`)
  larger, so Chromium applies word spacing to U+00A0 and Div does not.
- #4: for two spaces, Chromium reports a zero-width ink box at x = 33.297, the
  end of the first space's glyph advance, while Div reports one at 0. The
  width and the single-space case match. The cause has not been identified.

## Log

- 9 November 2026, PR #89: RFC 0001 typed style rules merged. Docs only;
  CI green.
- 9 November 2026: `oklch()`, `oklab()` and `color-mix()` parse and
  convert as Chromium 147 does, and paint no longer clamps color channels
  before Skia. Oracle: 54 swatches from pinned Chromium on Linux and
  Windows, identical; `cargo test -p divui --test native_color_functions`
  passes 54 of 54 on Windows. Unsupported forms are rejected; remaining
  limits are in the record.
- 10 November 2026, PR #95 merged (`ee46e350`).
- 10 November 2026, PR #97 merged (`c4b2e42e`): WOFF and WOFF2 are
  decoded to sfnt before the scaler on every platform.
- 10 November 2026: `view!` style attributes emit typed constructors
  instead of run-time `parse_literal` calls, the first A1 step. Every
  property's initial value and 155 corpus literals are checked against
  `parse_literal`; `cargo test -p divui --test typed_style_constructors`
  passes 3 of 3 on Windows. Census and CSS Grid coverage are unchanged.
  [Record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/typed-style-constructors-2026-11-09.md).
- 10 November 2026, PR #109 merged (`acd090c8`).
- 10 November 2026: `style!` compiles a CSS declaration block to typed
  styles at compile time, the second A1 step. [Record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/style-macro-2026-11-10.md).
- 11 November 2026, PR #116 merged (`e80a0bc2`): `rustix` 1.1.6 fixes
  RUSTSEC-2026-0486, which failed `dependency-security` on every PR.
- 11 November 2026, PR #111 merged (`c48c2e38`).
- 11 November 2026: a pointer leaving the window dispatches `pointerleave`
  to its whole hover path and clears `:hover`, as Chromium does, instead of
  a `pointercancel`. Census and CSS Grid coverage are unchanged.
  [Record](../pointer-window-leave-2026-11-10.md).
- 10 November 2026: grid columns with intrinsic maximums grow toward their
  growth limits (Blink's `MaximizeTracks`) when the container's inline size
  is definite, so an `auto` column reaches its max-content size. Stretched
  `auto` tracks keep the remainder of the free space, as in Chromium. The
  census is unchanged; CSS Grid coverage gains 9 exact references and loses
  none.
  [Record](../grid-auto-track-max-2026-11-10.md).
- 10 November 2026: a column flex item's automatic minimum height is
  measured at its used cross size, as Chromium measures it, not at its
  min-content width, where wrapped text made the item taller than its
  content. Census and CSS Grid coverage are unchanged.
  [Record](../column-flex-min-size-2026-11-10.md).
- 13 November 2026: flexible lengths are distributed as Blink distributes
  them: fractions against the running sum of the flex factors, items
  walked in reverse, each share rounded with `FromDoubleRound`. Fractional
  `flex-grow` no longer leaves items 1/64 px off Chromium
  (sample-interactions F21: its probe went from 3 to 24 of 24 boxes). The
  census gains one exact reference and loses none; CSS Grid coverage is
  unchanged.
  [Record](../flex-grow-rounding-2026-11-13.md).
- 13 November 2026, build-with-div/div#13 merged (`7e0a2674`).
- 13 November 2026: nested column flex containers lay each item out once
  per pass. A per-pass memo of flex item layouts and intrinsic block sizes
  stands in for Chromium's layout cache, so the work no longer doubles at
  each nesting level: 100 rows in six nested columns went from 7.2 s to
  0.13 s in release, with the same geometry. Census and CSS Grid coverage
  are unchanged.
  [Record](../flex-pass-cache-2026-11-13.md).
- 14 November 2026: Skia is initialized where Div creates its Skia users
  (font collections and so every document, shapers, paint recording,
  compositors and engines), not lazily on the first paint, so no thread
  can draw while `SkGraphics::Init` swaps Skia's function tables. The
  `div-compositor` lib tests aborted with heap corruption in 18 of 900
  parallel runs on `main` and in none with this change. Census and CSS
  Grid coverage are unchanged.
  [Record](../skia-init-order-2026-11-14.md).
- 10 November 2026: a typed selector model and a macro-time selector
  parser, with specificity checked against 93 selectors measured in pinned
  Chromium (and 18 it drops), the third A1 step.
  [Record](../selector-specificity-2026-11-10.md).
- 10 November 2026: the engine matches selectors on the element tree,
  checked against the elements pinned Chromium selects for 64 selectors on
  one fixture tree, the fourth A1 step.
  [Record](../selector-matching-2026-11-10.md).
- 15 November 2026: the new `div-canvas` crate implements the core of
  `CanvasRenderingContext2D`, ported from Blink's canvas2d (RFC 0008 step
  2). It covers state, transforms, compositing, colour and line styles,
  paths, fill, stroke, clip, rectangles and hit tests. Replaying 27 call
  traces against Chromium 147.0.7727.50, all 54 canvases and all 108
  observations are exact on Windows. Closed arcs follow cc's `DrawArcOp`
  playback. The Linux and Windows Chromium captures are identical. No existing crate's output
  changes.
  [Record](../canvas-core-2026-11-15.md).
- 21 November 2026: `div-canvas` gains text (RFC 0008 step 3): the text
  attributes, `fillText`, `strokeText` and `measureText`, shaped and
  rasterized by div-text, with Blink's canvas shape caches. Replaying 34
  traces against Chromium 147.0.7727.50, all 68 canvases and 201
  observations are exact on Windows; on Linux all observations and 67
  canvases are, with transformed web-font text a named gap. div-text's
  `is_web_font` becomes public; no existing crate's output changes.
  [Record](../canvas-text-2026-11-21.md).
- 22 November 2026: `div-canvas` gains gradients (RFC 0008 §7 step 4):
  `CanvasGradient` with linear, radial and conic constructors, as fill and
  stroke styles. Replaying 47 traces against Chromium 147.0.7727.50 on Linux,
  all 381 observations and 176 of 177 canvases are exact, the one left being
  the text gap above. Windows is checked by the `windows-msvc` CI job. No
  other crate changes. Three `gradient-composite` canvases differ by CPU
  vendor in Chromium itself, so they have one oracle per class, chosen by
  RCPPS fingerprint; Windows Intel comes from a local capture.
  [Record](../canvas-gradients-2026-11-22.md).
- 22 November 2026: `div-canvas` gains readback (RFC 0008 §5): `ImageData`,
  `createImageData`, `getImageData` and `putImageData`. Replaying 38 traces
  against Chromium 147.0.7727.50 on Linux, all 258 observations and 74 of 75
  canvases are exact, the one left being the text gap above. Windows is
  checked by the `windows-msvc` CI job. No other crate changes.
  [Record](../canvas-readback-2026-11-22.md).
- 10 November 2026: the selector matcher matches `:hover`, `:active`,
  `:focus`, `:focus-visible` and `:focus-within` from the engine's
  interaction state, with Blink's `:focus-visible` heuristic, checked
  against pinned Chromium; the fifth A1 step.
  [Record](../state-pseudo-classes-2026-11-10.md).
- 10 November 2026: the selector matcher matches `:disabled`, `:enabled`,
  `:checked`, `:indeterminate`, `:placeholder-shown`, `:read-only`,
  `:read-write` and `:required` from the engine's form control state,
  checked against pinned Chromium; the sixth A1 step.
  [Record](../form-state-pseudo-classes-2026-11-10.md).
- 16 November 2026: grid auto-placement starts from the first cell, as
  in Chromium: items with a definite position no longer move the
  auto-placement cursor, so the shadcn card header (`card-action`) is 44px
  tall, not 52px. Checked against four grids measured in pinned Chromium and
  the components track's card-action rectangles at 1x. The census is
  unchanged at all four profiles, and CSS Grid coverage gains 7 exact
  comparisons at 1x and 7 at 2x (real-zoom oracle) and loses none.
  [Record](../grid-auto-placement-cursor-2026-11-10.md).
- 16 November 2026: RFC 0011 proposes layout and paint in device pixels at
  scales other than 1, with the device scale applied as a zoom, as Chromium
  does. Of the 6,277 census rows that the real-zoom oracle (RFC 0010) made
  different, 5,632 differ only at edges and 645 need lengths or text in
  device pixels. Approved by the user the same day, sequenced as filler
  after Tier 0: stage 1 (CSS `zoom`) is an engine filler, and stage 2
  waits for its measured size.
  [RFC](../../alpha/rfcs/0011-device-pixel-layout.md).
- 16 November 2026, evidence history: the local helper that adds evidence
  files to div-fidelity-data converted CRLF to LF in every file, binaries
  included. It was caught on RFC 0011's three sample PNGs before their
  data-repo commit (`ae10ca8c`), which holds them byte for byte. Those
  three PNGs are the only binary files among the data repository's 54
  recorded additions (`SOURCE.json`); the other 51 are JSON, Python,
  Markdown, CSV and text, where the conversion is intended, so no
  published file changed. The helper now converts line endings in text
  files only.
- 15 November 2026: the engine applies style rules in CSS Cascade 5 order
  (R0a, the first step of Tier 0 item 0.1). An application adopts a
  `StyleSheet` of rules with normal and `!important` declarations and
  optional layers; matching rules cascade with the user-agent and author
  origins, presentational hints, the element's own declarations as the
  `style` attribute, and animations. Conservative invalidation keeps every
  incremental restyle equal to a full re-cascade. The census and CSS Grid
  coverage are unchanged at all four profiles.
  [Record](../rule-cascade-r0a-2026-11-15.md).
- 17 November 2026: a `style` attribute styles its element, read as
  Chromium 147 reads one (R0b, the second step of Tier 0 item 0.1).
  Declarations Div cannot use are dropped and reported, and the rest
  apply. The normal declarations rank just below the element's own
  declarations, and the `!important` ones above author `!important` rules.
  54 attributes measured in pinned Chromium give the same declarations,
  apart from 2 named gaps (`var()` and `inherit`, item 0.5). The census
  and CSS Grid coverage are unchanged at all four profiles.
  [Record](../style-attribute-r0b-2026-11-17.md).
- 17 November 2026: `div-text` builds a text blob whose glyphs sit at
  caller-given positions (T1, an engine filler for SVG text). A run
  rasterizes the same way whichever builder positions its glyphs, and
  placements equal to the advances give the advance blob. The census and
  CSS Grid coverage are unchanged at all four profiles.
  [Record](../positioned-text-blob-t1-2026-11-17.md).
- 17 November 2026: `stylesheet!` compiles CSS style rules, from a file
  or inline, to a typed `StyleSheet` and typed class names (0.1 PR 3a),
  and a document adopts it. Nesting and named layers are supported;
  `@media` and other unsupported constructs are compile errors. A
  cascade oracle of 8 cases in pinned Chromium gives the same computed
  values for all 344 probes. The census and CSS Grid coverage are
  unchanged at all four profiles.
  [Record](../stylesheet-pr3a-2026-11-17.md).
- 17 November 2026: Div makes each font at its device size truncated to
  1/100 px in f32, and its font cache shares a font between sizes with
  one key, as pinned Chromium 147 does (T4, an engine filler for SVG text).
  A Windows oracle of 4,514 cases agrees on every pair of sizes apart from
  25 named `font-size-adjust` pairs. The census changed 5 Div images at
  1.25x, all closer to Chromium, with exact counts unchanged; CSS Grid
  coverage is unchanged at all four profiles.
  [Record](../font-size-precision-t4-2026-11-17.md).
- 18 November 2026: `RasterConfiguration::chromium_windows_lcd()`, the
  text raster Windows Chromium 147 uses with ClearType on, RGB order and no
  ClearType tuning, and an oracle pin in `windows_chrome.py` that refuses
  to capture under any other Windows text settings (RFC 0012, engine
  part). No default changes, so the census and CSS Grid coverage cannot
  change.
  [Record](../windows-lcd-preset-rfc0012-2026-11-18.md).
- 18 November 2026: Div's web-font glyphs take their mask and hinting
  from the size and matrix they are drawn under, as Skia's Fontations
  scaler does: grayscale above 48 device px (T5), and unhinted unless
  the matrix is axis-aligned (T6). On Linux, 264 of 290 SVG text
  captures are exact (was 244); the 22 rotated and skewed captures left
  are named as the T6 residue, and T7 (non-uniform scale) is named. The
  census and CSS Grid coverage are unchanged at all four profiles.
  [Record](../web-font-raster-t5-t6-2026-11-18.md).
- 18 November 2026: Div hands each glyph the advance Blink gives
  HarfBuzz, its Skia width truncated to 1/65536 px, where SkShaper rounds
  it (T3, an engine filler for SVG text). The 7 Windows SVG text cases it
  held are exact. A new Windows oracle of 160 long texts names T8, Blink's
  fixed-point advance sums, in 100 cases that differ on `main` too, plus 1
  case T3 loses on the same drift; RFC 0013 proposes the fix.
  The census and CSS Grid coverage are unchanged at all four profiles.
  [Record](../advance-truncation-t3-2026-11-18.md).
- 18 November 2026: a state or attribute change restyles only the
  elements whose matching it can change, found from where adopted
  selectors read each class, id, attribute and state, and marks only the
  invalidation classes of the longhands that changed (0.1 PR 4). So a
  hover color change restyles the hover path and repaints, without
  layout. A gate test compares each step with a full re-cascade. The
  census and CSS Grid coverage are unchanged at all four profiles.
  [Record](../state-invalidation-pr4-2026-11-18.md).
- 18 November 2026: `line-height` accepts `calc()` over numbers,
  lengths and percentages, computed as Chromium 147 does, through a
  shared math-function value ported from Blink (0.2 PR 1). The line box
  now holds the used line height in layout units. A new Chromium oracle
  of 328 cases matches except 11 named gaps. The census and CSS Grid
  coverage are unchanged at all four profiles.
  [Record](../calc-line-height-2026-11-18.md).
- 22 November 2026: calc() (0.2 PR 1) rebased onto `main` `1f31bc59`
  and measured again: Windows 9,481 and Linux 9,485 tests pass, and the
  census and CSS Grid coverage are unchanged at all four profiles
  against that `main` (evidence `2d7287de`).
  [Record](../calc-line-height-2026-11-18.md).
- 22 November 2026: T8 (RFC 0013). `div-text` shapes with HarfBuzz
  directly and keeps each advance in exact 16.16 units, so all 160 cases
  of the long-text advance oracle match Chromium and its gap ledger is
  empty. Canvas text reads the same units. Windows 9,517 tests pass at
  `dccc14e`, and Linux 9,781 at `ea1ede4` on `main` `3b6ec37`. The census
  and CSS Grid coverage are unchanged at all four profiles against `main`
  `090406b` (evidence `4628f28e`).
  [Record](../t8-harfbuzz-units-2026-11-19.md).
- 22 November 2026: Skia is compiled with `-USK_ENABLE_AVX512_OPTS`, so
  Div has the same CPU raster paths as Chromium. Before this, Div on an
  AVX-512 CPU could render differently from Chromium. The Linux census
  pair changes 0 of 23,728 images, but its machine has no AVX-512, so it
  cannot show the effect there. Windows on Intel was not compared.
  [Record](../skia-no-avx512-2026-11-22.md).
- 23 November 2026: the eight corner radius axis names are no longer CSS
  names; their typed constants and setters stay. The site reference drops
  from 258 to 250 properties. The census and CSS Grid coverage are unchanged
  at all four profiles, measured at `4e69bdf` against `5bbcfff`, before #132
  and #120 (evidence `5d5ef0fb`). The rebased code patch has the same
  patch ID. Linux 1,154 tests pass at `d62d081` on `main` `8fbf18e`.
  [Record](../internal-css-names-2026-11-23.md).
- 23 November 2026: the last font client now releases the shaper's parent
  fonts before it purges Skia's strike cache, which repairs `main`'s weekly
  address and leak sanitizer jobs without a suppression. The census and CSS
  Grid coverage are unchanged at all four profiles, measured at `8fbf18e`
  against `0a11dbe`, before #134 and #135 (evidence `63f6e1ae`). The
  rebased code patch has the same patch ID. Linux 3,526 tests pass at
  `e6a2c1f` on `main` `9219d0a`. The web-font outline cache has the same
  retention and gets its own PR.
  [Record](../font-cache-parent-release-2026-11-23.md).
- 23 November 2026: twelve names pinned Chromium rejects by default are no
  longer CSS names. Their typed setters, FFI and animation stay. The site
  reference drops from 250 to 238 properties. The census and CSS Grid coverage
  are unchanged at all four profiles, measured at `5bbcfff` against
  `d3f4c31` (evidence `9c42d2a2`). Linux 1,160 tests pass
  at `8a29c22` on `main` `db3e244`.
  [Record](../css-dropped-names-2026-11-23.md). Open for the
  `-webkit-line-clamp` PR: `block-ellipsis-024` matches at 3 of 4 profiles by
  accident, because its fixture sets a clamp Chromium rejected and
  `algorithm.rs:6224` treats the empty string as no clamp; `block-ellipsis-023`
  and `-024` differ at 1.5x, cause not established.
- 23 November 2026: the last font client now empties the web-font outline
  cache too, which kept up to 32 entries with their own font bytes after the
  last engine (a probe kept 24.3 MB for one 760 KB font). The font lifetime
  counts are no longer public: both tests run as `div-text` unit tests in a
  child process of their own. The census and CSS Grid coverage are unchanged at
  all four profiles, measured at `db3e244` against `2d435c0`
  (evidence `4ce88e21`); the rebased code patch has the same patch ID.
  Linux 3,532 tests pass at `26fb891` on `main` `b7e4f6a`.
  [Record](../web-font-outline-release-2026-11-23.md).
- Pre-existing failure, owner Fontconfig initialisation in `div-text`: under
  `RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p div-text --lib lifetime_tests`, the parent-font lifetime test's child reports 320 bytes in 3
  allocations through `libexpat` and exits 23 (Fontconfig 2.17.1 locally). The
  same command with `--test shaping_font_lifetime` on `main` `d53a2d9` gives
  the same 320 bytes. CI's sanitizer job runs only `div-ffi`, so it does not run
  these tests. No suppression; the fix is a separate later item.
- 24 November 2026: `div-text` adds shaped run widths as floats, run by
  run, in the order Blink inserts runs and, after spacing, in visual order,
  instead of converting one summed 16.16 total. All 14 widths of the T8
  sweep now match, including its four regressions (#19, #45, #50, #57).
  The new `multi-run-width-v1` oracle's 616 non-Arabic widths match on Linux
  and Windows. The two open text gaps above stay pinned. The census and CSS
  Grid coverage are unchanged at all four profiles, measured at `b7e4f6a`
  against `691b956` (evidence `b43a0631`); the rebased code patch has the
  same patch ID. Linux 9,366 tests pass at `691b956`.
  [Record](../multi-run-width-2026-11-23.md).
- 24 November 2026: compile-time `calc()` in `style:`, `style!` and
  `stylesheet!` compiles again. The `ConstructorTokens` derive gains a
  variant option, `module = "name"`, and `StyleValue::Math` uses
  `module = "math"`, so the emitted constructors name the `calc()` tree at
  `typed_style::math`. No public path is added. The test is the components
  track's repro. A masked binary comparison showed the code compiled into
  pixel-compare changes, so a full pair ran. The census and CSS Grid
  coverage are unchanged at all four profiles, measured at `b7e4f6a` against
  `b618850` before #142's `-O2` (evidence `b6c41deb`); the code patch has
  the same patch ID at every base. The four T8-sweep text gaps above are
  recorded as engine-owned. [Record](../calc-macro-paths-2026-11-24.md).
- 24 November 2026: S3/S4 PR 1 (RFC 0014). `stylesheet!` sheets hold
  `@media` rules at the top level, in a layer, nested in `@media` and nested
  in a style rule. Queries parse and evaluate as Chromium 147 does, and they
  follow the viewport, the device scale factor, the new
  `Document::set_color_scheme` and the reduced-motion preference. The new
  `media-queries-v1` oracle (84 queries in 7 environments) matches in all
  588 values. The census and CSS Grid coverage are unchanged at all four
  profiles, measured at `b7e4f6a` against `8106a6d` before #142's `-O2`
  (evidence `179feb54`); the rebased code patch has the same patch ID.
  [Record](../media-style-sheets-2026-11-24.md).
