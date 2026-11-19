# RFC 0013: Exact HarfBuzz units (a port of Blink's HarfBuzzShaper)

Status: draft for the user's approval, 18 November 2026. Owner: the
engine track (`div-text`). Consumers: every track that lays out or paints
text (framework, components, the SVG and canvas lanes, the fidelity lane
and the samples). Gap name: T8.

## Problem

`div-text` shapes through Skia's SkShaper, which wraps HarfBuzz. Blink
calls HarfBuzz itself (`platform/fonts/shaping/harfbuzz_shaper.cc`,
`harfbuzz_face.cc`, `shape_result.cc`). Three differences follow. The
first is fixed per glyph by T3; the other two are not reachable through
SkShaper:

1. **Advance conversion (T3).** Blink hands HarfBuzz each Skia advance
   truncated to 1/65536 px (`SkiaScalarToHarfBuzzPosition`,
   `ClampTo<int>(value * 65536)`); SkShaper rounds it. T3 takes the
   difference back out per glyph, which made the SVG text corpus exact.
2. **Exact sums.** Blink stores each advance as
   `TextRunLayoutUnit::FromFixed<16>(x_advance)` and sums a run in int64
   (`InlineLayoutUnit`), converting to float once. SkShaper hands Div float
   glyph positions; past 256 px their spacing is coarser than 1/65536 px,
   and Div then sums the derived advances in f32. A long run drifts.
3. **HarfBuzz scale.** Blink sets the scale to `ClampTo<int>(size * 65536)`
   (`HarfBuzzFace::GetScaledFont`) and sets `ptem`; SkShaper rounds the
   scale and sets no `ptem`. The scale enters GPOS kerning and mark
   offsets. Unmeasured.

**Evidence.** `tests/fidelity/fixtures/advance-truncation-v1/` (T3's PR):
160 long single-font spans on pinned Windows Chrome 147, DejaVu Sans and
Geist at 20 sizes. Div differs in 101, by up to 0.23 px; 100 differ on
`main` too, and one is a loss T3 caused on the same drift (its gap entry
gives the measured units). Short runs are unaffected: below 256 px Skia's
positions are exact, which is why the SVG corpus and most census rows
already match.

## Proposal

Shape each item with HarfBuzz directly, as Blink does, and keep glyph
advances and run widths in fixed point.

- **Itemization stays.** Skia's run iterators through `skia-safe` give
  the script (`new_hb_icu_script_run_iterator`), bidi level
  (`new_icu_bidi_run_iterator`) and language runs, as today. `div-text`
  walks them itself (`consume`, `end_of_current_run`) instead of handing
  them to `SkShaper::shape`. Font fallback stays `div-text`'s own.
- **The HarfBuzz face and font, as `harfbuzz_face.cc` makes them.** A face
  from the typeface's tables (`hb_face_create_for_tables` with
  `SkTypeface::getTableData`, or a blob of its data), an OT parent font
  (`hb_font_create`, `hb_ot_font_set_funcs`, `hb_font_set_variations` from
  the typeface's design position) and a sub-font
  (`hb_font_create_sub_font`) with Blink's Skia funcs: nominal and
  variation glyphs, horizontal advances (`SkFont::getWidths`, rounded to
  whole pixels without subpixel positioning, then truncated), extents,
  and vertical origin and advance. Scale `ClampTo<int>(size * 65536)` on
  both axes, and `hb_font_set_ptem` with the specified size. Non-Apple
  Blink always uses the Skia advances; so does Div.
- **Shaping.** One buffer per item (`hb_buffer_create`, `hb_buffer_add_utf8`
  with the item's context, `hb_buffer_set_direction`, `_set_script`,
  `_set_language`), the existing OpenType features, `hb_shape`, then
  `hb_buffer_get_glyph_infos` and `_positions`. Glyph flags
  (`hb_glyph_info_get_glyph_flags`, `HB_GLYPH_FLAG_UNSAFE_TO_BREAK`) give
  the safe-to-break points that `shaper.rs` now approximates.
- **Fixed point.** `ShapeResultRun` gains `advance_units: Vec<i32>`,
  each advance in 1/65536 px as Blink's `TextRunLayoutUnit` keeps it.
  `ShapeResult::width` becomes the `i64` sum of every run's units
  (Blink's `InlineLayoutUnit`) converted to `f32` once, as `ToFloat`
  does; the per-character `x_position`s come from the same running sum.
  The public `f32` fields stay, filled from the units, so callers that
  read them do not change. Offsets stay `f32` from `x_offset / 65536`, as
  `HarfBuzzPositionToFloat` does. T3's per-glyph correction is removed;
  the funcs truncate.
- **FFI.** The `hb_*` functions are declared in `div-text` (about 30, all
  in HarfBuzz's stable C API) and resolve against the HarfBuzz that Skia
  already links: `libskia.a` on Linux and `skia.lib` on Windows each
  export the full API (543 and 548 `hb_*` symbols in the current builds).
  No new dependency or build step. A build check fails if a declared
  symbol is missing.

A short example of the run a caller sees, unchanged in shape:

```rust
// HarfBuzz's advances, kept exactly; the width is their sum, converted once.
let run = &shape_result.runs[0];
let units: i32 = run.advance_units[0]; // 1/65536 px
let first: f32 = run.advances[0]; // units as f32 / 65536, as before
let width: f32 = shape_result.width(); // (i64 sum of all units) / 65536
```

## Licences

- Blink's shaping sources: BSD (Chromium). Read at 147.0.7727.50 and
  listed in the engine lane's `SOURCES-engine-t3-2026-11-18.txt`.
- HarfBuzz: "Old MIT" (`third_party/externals/harfbuzz/COPYING` in Div's
  Skia).
- Skia and rust-skia: BSD and MIT.

No copyleft source is read or linked.

## Census

Widths move only where Div and Blink now differ:

- runs with a truncated advance (Windows web fonts and system fonts with
  fractional DirectWrite advances; Linux FreeType advances sit on 1/64 px
  and do not change);
- runs longer than 256 px (the drift);
- kerned text at a size whose scale truncates (the unmeasured third).

Each moves by a few 1/65536 px before the layout unit's ceiling, so a
changed row moves by 1/64 px at a line or box edge. The implementation PR
runs the full census at all four profiles and the CSS Grid coverage, and
lists every changed image. D4 holds: no losses. A row that changes and is
not closer to Chromium is a named gap with its cause, as in T3's loss.

## Affected tracks

- **Engine (`div-text`, `div-layout`, `div-svg` callers):** the port. The
  `ShapeResult` and `ShapeResultRun` `f32` fields stay, so callers that
  read them do not change. Code that builds a `ShapeResultRun` by hand
  (`div-text`'s tests and one `div-engine` test) adds the units.
- **Framework, components, samples:** no API change; text widths may move
  by 1/64 px where the census shows it.
- **SVG and canvas lanes:** none; their corpora gate the result.
- **Fidelity lane:** the advance oracle becomes exact, and T3's gap
  ledger empties.

## Migration

1. This RFC merges after the user's approval.
2. One PR in `div-text`: the HarfBuzz face and font, the fixed-point run,
   the shaper switch, and removal of T3's correction. The advance oracle's
   ledger must empty, or each remaining line gets a named cause. The SVG
   text corpus, census and coverage run as for any engine PR.
3. `main` stays green throughout: the switch is one PR, with no flag and no
   parallel shaper.
