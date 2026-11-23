# T8: exact HarfBuzz units (RFC 0013), 19 November 2026

`div-text` now shapes with HarfBuzz directly, as Blink's `HarfBuzzShaper`
does, instead of through Skia's SkShaper. Each glyph keeps HarfBuzz's
advance in 16.16 fixed point, and widths are integer sums converted to
`f32` once. This is RFC 0013, approved on 18 November 2026.

All 160 cases of the long-text advance oracle
(`tests/fidelity/fixtures/advance-truncation-v1/`) now match pinned
Windows Chrome 147 exactly; T3 left 101 of them listed. The oracle's gap
ledger is empty.

## What Blink and Skia do

Read from Chromium 147.0.7727.50 (BSD), listed in
`out/chromium-ref/SOURCES-engine-t3-2026-11-18.txt` and
`SOURCES-engine-t8-2026-11-19.txt` (`shape_result_spacing.cc` and `.h`,
fetched for this change), and from Div's own Skia (BSD) and HarfBuzz
12.2.0 (Old MIT), rust-skia `a31b86b`.

- **Blink** (`shaping/harfbuzz_face.cc`, `skia/skia_text_metrics.cc`,
  `shaping/shape_result.cc`). A sub-font of a per-typeface OpenType font
  has Skia's advance and extents funcs, which truncate to HarfBuzz units
  (`SkiaScalarToHarfBuzzPosition`). The scale is
  `ClampTo<int>(size * 65536)`, and `ptem` is the specified size. Each
  glyph keeps `x_advance` as a `TextRunLayoutUnit` (16.16 in `int32`), and
  a run sums them in an `InlineLayoutUnit` (`int64`), converting once.
  Letter and word spacing are converted once by `TextRunLayoutUnit(float)`,
  a truncation, and added to the glyph advances in units
  (`ShapeResultSpacing::SetSpacing`, `HarfBuzzRunGlyphData::AddAdvance`).
- **SkShaper** (`modules/skshaper/src/SkShaper_harfbuzz.cpp`,
  `ShapeThenWrap`). The same HarfBuzz, but its funcs round advances and
  extents, it rounds the scale and sets no `ptem`, and it hands Div float
  glyph positions. Past 256 px their spacing is coarser than 1/65536 px,
  and Div then summed the derived advances in `f32`.

## Change

- **`shaping/harfbuzz.rs` (new).** The HarfBuzz C API (stable functions
  only), resolved against the HarfBuzz Skia links on both OSes; no new
  dependency or build step.
  - Font: a face over the typeface's tables with Skia's units per em, an
    OpenType parent font at the typeface's variation position (cached per
    typeface, the 100 most recently used, as Skia's cache), and a sub-font
    per call with Blink's truncating advance and extents funcs, Blink's
    scale and `ptem`. Glyph lookup stays Skia's (`SkFont::unicharToGlyph`,
    variation selectors ignored), as before.
  - Shaping: the same runs SkShaper made (the intersection of Skia's ICU
    bidi and HarfBuzz-ICU script runs, Skia's `RunIteratorQueue`), the same
    buffer (UTF-8 byte clusters with the whole text as context,
    `MONOTONE_CHARACTERS`, direction by bidi level, script, language or
    `und`), the same global features in Div's order, and runs in visual
    order by UAX #9 rule L2. Unlike SkShaper, a run with no glyphs does not
    drop the runs after it, as in Blink.
- **`ShapeResultRun::advance_units`.** Each advance in 1/65536 px; the
  `f32` `advances` are filled from them, so callers that read them do not
  change. `ShapeResult::width` is the `i64` sum of every glyph's units
  converted once (Blink's `ComputePositionData`), and the characters'
  `x_position`s come from the same running sum. Letter spacing, word
  spacing, `text-autospace`, `text-spacing-trim` and the two justification
  entry points add in units, converted by truncation. Runs built by hand
  (tests) take their units from `ShapeResultRun::units_from_px`.
- **T3's per-glyph correction is removed**: the funcs truncate.
- `TextShaper::shape` and the fallback path now share one shaping function.
- **Canvas text** (`div-canvas/src/text.rs`), which merged while this PR
  was in review, now reads the exact units: it copies `advance_units`
  with each run and takes ink origins from them, within a run and for
  each later run (`run->width_`, the run's units converted once). A
  space at the start of canvas text gets no word spacing, through the
  new `TextShaper::shape_at_text_start` (`ShapeResultSpacing::ComputeSpacing`).
  Canvas used to add the spacing and subtract it, which loses the
  space's advance once the add saturates at 32768 px.

## Verification

The census and the Windows suites ran at `dccc14e`, on `main` `090406b`.
This PR was then rebased onto `main` `3b6ec37`, which added canvas text
(#118), canvas readback (#123), `div-wpt-script` work (#119, #124) and
site changes (#121), and gained the canvas commits above. Its code is
final at `ea1ede4`.

| Check | Command | Commit | Result |
| --- | --- | --- | --- |
| Advance oracle, Windows | `cargo test -p div-engine --test advance_truncation` | `dccc14e` | 160 of 160 exact (T3: 59). Ledger emptied |
| Windows suites: `div-style`, `div-text`, `div-layout`, `div-paint`, `div-svg`, `div-engine`, `divui`, `div-wpt-script` | `cargo test --locked --no-fail-fast -j 4` under the shared lock | `dccc14e` | 9,517 passed, 0 failed, 18 ignored |
| Generators and ledgers (Windows) | `check_platform_scoped_tests.py`, the renderer contract, v0.2, property and FFI generators and the SP13-R, SP17, SP18 and SP20 closures with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify` | `dccc14e` | All pass, no drift |
| Linux suites, CI's package list and features, including `div-canvas`'s Chromium traces (WSL, CI's Skia flags) | as CI | `ea1ede4` | 9,781 passed, 0 failed, 17 ignored |
| SVG text corpus, Linux | `cargo test -p div-svg --test text_corpus` | `ea1ede4` | Position and pixel tests pass; 264 of 290 captures exact, as on `main` |
| Expanded census, all four profiles, against `main` `090406b` | `div-census.sh` (1x run) and `div-census-zoom.sh` (real-zoom oracle above scale 1) under the shared lock | `dccc14e` | 16,297 of 23,728 exact (1x run) and 10,404 of 17,796 (real zoom), before and after. 0 gains, 0 losses, 0 errors; 0 Div images changed |
| CSS Grid coverage | The same runs | `dccc14e` | 1,473 of 4,204 exact (1x run) and 894 of 3,153 (real zoom), before and after. 0 statuses or images changed |

The census was not run again at `ea1ede4`, because nothing the census
binaries run differs from `dccc14e`. Outside `div-canvas`, the only crate
that changes is `div-text`: one `pub` on
`FontFaceSourceInfo::is_web_font`, one comment, and
`TextShaper::shape_at_text_start`. `TextShaper::shape` passes
`word_spacing_at_start = true`, which leaves the condition in
`apply_spacing` as it was for page text. The new entry point's only
caller is `div-canvas/src/context/text_methods.rs`, and the ink-bounds
change is private to `div-canvas`. No crate depends on `div-canvas` or
`div-wpt-script` (`cargo metadata --no-deps`), and `Cargo.lock` only adds
`div-geometry`, which has no features, to `div-canvas`. Canvas text with
T8 was not run locally on Windows, because that build needs the shared
lock; CI's Windows job runs it.

The evidence is div-fidelity-data `4628f28e`
(`docs/progress/evidence/t8-harfbuzz-units-div-census-v1.json`, and
`-census-zoom-v1.json`, `-coverage-v1.json` and `-coverage-zoom-v1.json`
beside it); `fidelity-data.lock` pins it.

## Named gaps and follow-ups

Owned by the engine (`div-text`), not part of RFC 0013's three changes,
and each left as SkShaper had it so this change moves only the units:

- **Non-subpixel position rounding.** Blink rounds every HarfBuzz position
  to whole pixels when the shaping font does not position subpixels
  (`RoundHarfBuzzBufferPositions`, `harfbuzz_shaper.cc`); SkShaper did not.
  Unmeasured.
- **Unsafe-to-break flags.** RFC 0013 proposes reading
  `HB_GLYPH_FLAG_UNSAFE_TO_BREAK`; `shaper.rs` keeps its script heuristic,
  because the flags change where the line breaker reshapes, not widths.
- **Spacing distribution.** Blink adds spacing once per cluster to its
  last glyph; Div spreads it over a ligature's glyphs by the characters
  each covers. The arithmetic is now Blink's; the distribution is not.
- **Cluster level** (`MONOTONE_GRAPHEMES` in Blink), **glyph lookup**
  through HarfBuzz's cmap with variation selectors, **vertical funcs**
  (`HB_DIRECTION_TTB`), and **8-bit text forced to Latin** script.
- `ShapeResult::sub_range` takes its width from `x_position` differences,
  not from the sub-range's units (Blink's `SubRange`).
- **Width of a multi-run result.** `ShapeResult::width` sums all runs'
  units and converts once. Blink adds each run's float width
  (`shape_result.cc`, `InsertRun` while shaping and
  `ApplySpacingOrExpansion` afterward), and clamps a negative run to zero
  while shaping. In a throwaway sweep of 70 multi-run canvas words on
  Linux, 14 widths still differ from Chromium; `main` had 34. The fix
  needs Blink's run order for right-to-left and fallback text and its
  own census pair, so it is a follow-up PR owned by the engine track.
- **No-break spaces get no word spacing** in `apply_spacing`; Blink's
  `ComputeSpacing` adds it, also at index 0. Measured in canvas text on
  Linux, on `main` and with this change alike. Page text is unmeasured.
