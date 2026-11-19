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
| RFC 0008 Canvas 2D | RFC merged (build-with-div/div#43). Step 2, core state and paths in `div-canvas`: 54 of 54 trace canvases and 108 of 108 observations exact against Chromium; [record](../canvas-core-2026-11-15.md) |

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
- 18 November 2026: a state or attribute change restyles only the
  elements whose matching it can change, found from where adopted
  selectors read each class, id, attribute and state, and marks only the
  invalidation classes of the longhands that changed (0.1 PR 4). So a
  hover color change restyles the hover path and repaints, without
  layout. A gate test compares each step with a full re-cascade. The
  census and CSS Grid coverage are unchanged at all four profiles.
  [Record](../state-invalidation-pr4-2026-11-18.md).
