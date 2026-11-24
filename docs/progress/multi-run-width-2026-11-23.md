# Multi-run text width (23 November 2026)

Engine track. Owner: `div-text` (`crates/div-text/src/shaping/shape_result.rs`,
`shaper.rs`).

## Change

Blink keeps each shaped run's width as a float and adds those floats. Div
summed every glyph's 16.16 units as an `i64` and converted once, so a result
of several runs past 256 px could differ from Chromium by a rounding step.

Div now converts each run's units to a float once and adds the floats:

- At shaping, in the order Blink inserts runs: by segment, then font pass,
  then position in the pass (`width_of_inserted_runs`,
  `width_after_fallback`). Source: `ShapeResult::InsertRun`,
  `third_party/blink/renderer/platform/fonts/shaping/shape_result.cc:1540`
  and `:1573`.
- After spacing, in visual order (`width_from_runs`). Source:
  `ShapeResult::ApplySpacingOrExpansion`, `shape_result.cc:960-1013`. Div's
  fallback splice stores right-to-left runs in logical order, so they are
  walked from the last character.

The original `width_from_runs` comment is kept verbatim, with new lines after
it saying this change closes that gap.

## What it clears

All 14 widths of the T8 sweep, measured at `2cee39d` on `main` `4e69bdf`
(Linux):

- T8 regressions: #19, #45, #50, #57.
- Wrong before and after T8: #36, #38, #49, #51, #60, #62, #65, #66, #68, #69.

t8-extra #4, #6, #7 and #8 fail identically on `main`, T8 and this fix. They
are not this change.

## Oracle

`tests/fidelity/fixtures/multi-run-width-v1`, captured from Chromium
147.0.7727.50 on Linux and Windows by
`tools/canvas/generate_multi_run_width_trace.py`: 728 widths per platform
covering left-to-right, right-to-left, fallback and spacing. The replay is
`cargo test -p div-canvas --test multi_run_width`.

- Linux, at `2cee39d`: 100 of the 616 non-Arabic widths differ without the
  fix, 0 with it.
- Windows, at `691b956` with `main`'s Skia flags: all 616 non-Arabic widths
  match.

## Named gaps (not closed here)

Each is pinned by the replay, so a change in its count fails. Both are
recorded in [the engine track file](tracks/engine.md#open-text-gaps-found-by-the-multi-run-width-oracle-22-november-2026).

| Gap | Owner | Linux | Windows |
| --- | --- | --- | --- |
| Letter spacing on cursive scripts | `div-text`, spacing PR | 112 of 112 Arabic widths differ | 112 of 112 Arabic widths differ |
| Right-to-left fallback paint order | `div-text`, RTL fallback PR | Pixels of 19,200: `alternating` 3,514, `box-letter` 1,443, `letter-box` 1,443, `letter-box-letter` 717 | `alternating` 3,740, `box-letter` 1,512, `letter-box` 1,512, `letter-box-letter` 743 |

The right-to-left cause (`splice_fallback_runs` storing runs in logical
order) is likely and has not been proven.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Linux suites (`div-text`, `div-canvas`, `div-paint`, `div-engine` and the rest of the census slot's set) and format of changed files | `691b956` on `main` `b7e4f6a` | 9,366 passed, 0 failed, 12 ignored; format clean |
| Windows `cargo test --locked -p div-canvas --test multi_run_width` | `691b956` | 2 passed |

Windows used MSVC 14.52.37025; CI's `windows-msvc` job pins 14.44, which
this machine no longer has. Skia was built with the `windows-msvc` job's
`SKIA_GN_ARGS` from `main`'s `ci.yml`, after #112. The build's `args.gn`
holds:

```text
extra_cflags += [ "-DSK_SUPPORT_LEGACY_CONIC_CHOP", "-DSK_ENABLE_LEGACY_SHADERCONTEXT", "-USK_ENABLE_AVX512_OPTS", "-DSK_USE_FREETYPE_EMBOLDEN", "-DSK_GAMMA_EXPONENT=1.2", "-DSK_GAMMA_CONTRAST=0.2", "-USK_GAMMA_APPLY_TO_A8", "-fno-strict-aliasing", "-msse3", "/clang:-ffp-contract=off" ]
```

`obj\hsw.ninja` holds `-ffp-contract=off`. The Windows pins were first
measured at `97cb9ae` with the flags before #112. They hold unchanged with
the new flags, so no number moved.

## Census (D4)

Measured at `main` `b7e4f6a` against `691b956` (pushed as
`engine/multi-run-width`), in one census lock slot, after the Linux suites
and format passed:

| Profile set | Comparisons | Exact before | Exact after | Div images changed |
| --- | --- | --- | --- | --- |
| Census, 1x | 23,728 | 16,297 | 16,297 | 0 |
| Census, real zoom | 17,796 | 10,404 | 10,404 | 0 |
| CSS Grid coverage, 1x | 4,204 | 1,473 | 1,473 | 0 |
| CSS Grid coverage, real zoom | 3,153 | 894 | 894 | 0 |

Every Chromium image is identical. Each stage finished on its first attempt,
and its logs hold no `/mnt/e` memory error. Receipts: data repository
`b43a0631`, `docs/progress/evidence/multi-run-width-div-*-v1.json`.

**What the pair does not cover.** It was measured before #126 and #143 and not
rerun after the rebase. The code patch is unchanged by the rebase:

```text
git diff <base> <head> | git patch-id --stable
b7e4f6a..691b956  43aeddd91c6073cab13ae91b4c8d9412b3015168
c2ad60a..e889151  43aeddd91c6073cab13ae91b4c8d9412b3015168
```

This does not establish renderer or release qualification.
