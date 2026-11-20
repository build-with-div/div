# T3: glyph advances truncated to HarfBuzz units, 18 November 2026

Div now hands each glyph the advance Blink gives HarfBuzz: the Skia width
truncated to 1/65536 px, where SkShaper rounds it. The SVG lane found the
difference (T3 in its requests to the engine lane): 7 Windows cases of the
SVG text corpus differed by it alone. All 7 are now exact on Windows, and
every number of the corpus matches on both OSes.

A new oracle of long texts measures the rest of the way Blink keeps
advances. It finds a larger, older difference, named T8: Blink sums
advances in fixed point, and Div sums SkShaper's float positions. T8 is
proposed as RFC 0013 and is not part of this change.

## What Blink and Skia do

Read from Chromium 147.0.7727.50 (BSD), listed in
`out/chromium-ref/SOURCES-engine-t3-2026-11-18.txt`, and from Div's own
Skia (BSD), rust-skia `a31b86b`.

- **Blink** (`platform/fonts/skia/skia_text_metrics.cc`). HarfBuzz's
  advance funcs read `SkFont::getWidths`, round to whole pixels for a font
  without subpixel positioning, and convert with
  `SkiaScalarToHarfBuzzPosition`: `ClampTo<int>(value * 65536)`, a
  truncation. Off Apple, Blink always uses these Skia advances
  (`shaping/harfbuzz_face.cc`).
- **SkShaper** (`modules/skshaper/src/SkShaper_harfbuzz.cpp`). Its funcs
  read the same widths and convert with `skhb_position`:
  `SkScalarRoundToInt(value * 65536)`, half up.
- **The difference.** Scaling by 65536 is exact in f32, so a glyph's
  advance differs by exactly one unit (1/65536 px) when the scaled width
  has a fraction of one half or more, and is the same otherwise. Linux's
  FreeType widths sit on 1/64 px and never differ; Windows' DirectWrite
  widths do.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `advance_truncates_below_skshaper`: per glyph of a run, whether Blink's advance is one unit below SkShaper's, from the run font's widths | `div-text` | `crates/div-text/src/shaping/shaper.rs` |
| The shape collector takes that unit off each such glyph's advance. A zero advance (a mark HarfBuzz zeroed after reading the funcs) is left alone | `div-text` | `shaper.rs` |
| The 7 T3 lines leave `known-gaps.txt`, which now lists none. Their pixel lines join the Windows text raster block, where they differ only in tone (`list/dx-em` from 350 pixels, 56 per channel, to 341 and 16) | `div-svg` corpus | `tests/fidelity/fixtures/svg-text-v1/`, `tools/svg/README.md` |
| An oracle of 160 long texts and its test, with T8's gaps listed | engine qualification | `tools/qualification/capture_advance_truncation_oracle.py`, `tests/fidelity/fixtures/advance-truncation-v1/`, `crates/div-engine/tests/advance_truncation.rs` |

## The long-text oracle

`capture_advance_truncation_oracle.py` on Windows with pinned Chrome
147.0.7727.50 lays out one inline-block span (`white-space: pre`) per case
and records its width. Glyph units add up over a long text:

- **Texts:** `n` 1,000 times (no kerning), `AV` and `To` 500 times each
  (kerned), and a 38-character sentence 25 times;
- **Fonts:** DejaVu Sans and Geist, as web fonts from this repository;
- **Sizes:** 20, from 10 to 32 px.

`advance_truncation.rs` lays the same spans out in Div. 59 of 160 widths
match Chromium. The other 101 are listed in `known-gaps.txt` with Div's
width, and the test fails if any of them changes.

- **T8, 100 cases.** They differ on `main` too, with the same widths.
  - **Blink:** keeps each advance as `TextRunLayoutUnit::FromFixed<16>`
    and sums a run in int64 (`InlineLayoutUnit`), converting to float once
    (`shaping/shape_result.cc`, `geometry/layout_unit.h`).
  - **Div:** derives each advance from SkShaper's float glyph positions,
    whose spacing past 256 px is coarser than one unit, and sums them in
    f32. A run of 1,000 glyphs drifts by up to 0.23 px.
- **A loss caused by T3, 1 case.** `DejaVu Sans | sentence | 25.29` was
  exact on `main` and is 1/64 px narrow with T3.
  - **Method:** a temporary probe shaped the text word by word, so every
    position is below 256 px, to get each glyph's exact units. Replaying
    them through SkShaper's f32 accumulation reproduces `main`'s 950 glyph
    positions bit for bit.
  - **Units, 1/65536 px each:**

    | Sum | Units |
    | --- | ---: |
    | Blink, exact | 803,311,225 |
    | SkShaper's rounded advances, exact | 803,311,500 |
    | `main`, f32 | 803,310,656 |
    | T3, f32 | 803,310,592 |

  - **Why `main` was exact:** its rounding added 275 units and its f32
    drift took away 844. The two partly cancelled, leaving `main` inside
    Blink's layout unit by 64 units.
  - **Why T3 is not:** T3 removes the rounding. In f32 that moves the sum
    one step (64 units) lower, onto the layout-unit boundary, so the width
    rounds down a layout unit.
  - **Owner:** T3's per-glyph units are Blink's. The drift is T8's, and
    T8's exact sum fixes this case.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| SVG text corpus, Windows | `cargo test --locked -p div-svg --test text_corpus` | All 24,056 numbers exact; the 7 T3 cases were listed |
| Advance unit test | `glyph_advances_are_skia_widths_truncated_to_harfbuzz_units` | Each lone glyph's advance is its Skia width truncated to 1/65536 px; on Windows some widths truncate |
| Long-text oracle | `cargo test --locked -p div-engine --test advance_truncation` (Windows) | 59 of 160 exact; the 101 listed gaps unchanged |
| Windows suites | `cargo test --locked -j 4 -p div-text -p div-paint -p div-svg -p div-engine -p divui` at `c330927a` | 3,331 passed, 0 failed |
| Linux suites (WSL, CI's Skia flags) | `cargo test --locked --no-fail-fast -j 4 -p divui -p div-engine -p div-svg -p div-text -p div-paint` at `c330927a` | 3,333 passed, 0 failed |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles, real-zoom oracle, against `main` `c65b1245` | `div-census.sh` and `div-census-zoom.sh` under the shared lock, at `c330927a` | 16,296 of 23,728 exact before and after. 0 gains, 0 losses, 0 errors; 0 Div images changed |
| CSS Grid coverage | The same runs | 1,473 of 4,204 exact (1x run) and 894 of 3,153 (real zoom), before and after. 0 changed |

The census and coverage run on Linux, where FreeType advances are whole
units and T3 changes nothing, so no image moves. The Windows effect is
measured by the SVG text corpus and the long-text oracle above. The
evidence is div-fidelity-data `c4b87acd`
(`docs/progress/evidence/advance-truncation-t3-div-census-v1.json`, and
`-div-coverage-v1.json` and `-div-coverage-zoom-v1.json` beside it);
`fidelity-data.lock` pins it.

### After rebasing onto `main` `3bed9562`

`main` gained #103 (T5 and T6), whose evidence is div-fidelity-data
`31eeae71`, the parent of this change's `c4b87acd`. The head `4a2e16e9`
was measured again:

| Check | Result |
| --- | --- |
| Windows suites: `div-style`, `div-text`, `div-layout`, `div-paint`, `div-svg`, `div-engine`, `divui` | 9,456 passed, 0 failed, including the SVG text corpus and the long-text oracle (59 of 160 exact, the 101 listed gaps unchanged) |
| Linux suites, the same seven crates (WSL, CI's Skia flags) | 9,461 passed, 0 failed |
| SVG text corpus, Linux | 24,056 position numbers, none differ; 264 of 290 captures exact, as on `main` |
| Generators, `cargo fmt --check`, `fidelity_data.py verify --profile all` | All pass. SP20 could not be checked locally (this machine's WPT checkout is not the pinned tree); this change touches none of its inputs |

The census was not rerun: T3 changes no Linux advance (FreeType advances
are whole units), and #103 changed no census image either.

## Named gaps

- **T8, exact HarfBuzz units.**
  - **Owner:** engine (`div-text`).
  - **Cause:** Blink's fixed-point advances and run sums, against Div's
    float positions and f32 sums (above).
  - **Cases:** 100 of the long-text oracle, plus the loss below.
  - **Also unmeasured under T8:** Blink truncates the HarfBuzz scale,
    `size × 65536` (`HarfBuzzFace::GetScaledFont`), and sets `ptem`;
    SkShaper rounds the scale and sets no `ptem`. The scale enters GPOS
    kerning and mark offsets.
  - **Next step:** RFC 0013, a port of Blink's HarfBuzzShaper, for the
    user's approval.
- **Loss caused by T3:** `DejaVu Sans | sentence | 25.29` (above), owned
  with T8.

## Not done here

- **Linux.** FreeType advances are already whole units, so T3 does not
  change Linux shaping. The long-text oracle is Windows only.
