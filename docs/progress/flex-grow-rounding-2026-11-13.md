# Flexible lengths are distributed as Blink distributes them, 13 November 2026

Flex items with fractional or uneven `flex-grow` factors now get the same
sizes as in Chromium 147. Before, they could land one LayoutUnit (1/64 px)
away, and so could every box placed after them. sample-interactions found
this (its finding F21). With `flex: 69.697 1 0px` and `flex: 30.303 1 0px`
over 396px, Chromium gives 276px and 120px; Div gave 276.015625px and
119.984375px. The same error explained all 99 differing boxes out of 380 in
the sample's react-resizable-panels run.

## Finding

`LineFlexer` in `crates/div-layout/src/flex/line_flexer.rs` gave each
unfrozen item a fraction of the line's total flex factor. It then took
truncated differences of cumulative shares (`LayoutUnit::from_f64`, which
truncates). The cumulative fraction is a float sum, so it can end just
below 1. Then the line's total also came out short: grow 10, 70 and 20 over
792px gave 79.1875, 554.40625 and 158.390625, 1/64 px less than 792.

Chromium 147 does it differently. This source was read at 147.0.7727.24,
the local source checkout, which is on the same branch as the pinned
147.0.7727.50.

- **Fractions.** `LineFlexer::FreezeItems`
  (`third_party/blink/renderer/core/layout/flex/line_flexer.cc`) gives each
  item a fraction against the running sum of the factors so far, in item
  order: `flex_factor / total_flex_factor_`. When shrinking, factors are
  weighted by the base size against the running weighted sum.
- **Distribution.** `ResolveFlexibleLengths` walks the items in reverse.
  - Each takes `LayoutUnit::FromDoubleRound(free_space_ * fraction)` and
    subtracts it from the free space, so each takes its share of what the
    later items left.
  - The first item, whose fraction is exactly 1, takes the rest.
  - For the example that is 158.40625 for the last item, 554.390625 for the
    middle one and 79.203125 for the first, Chromium's values, summing to
    792.
- **Reset.** `FreezeItems` also resets each unfrozen item to its
  hypothetical size, and the early exits keep that size.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `freeze_items` computes each fraction against the running sum (weighted by base size when shrinking) and resets unfrozen items to their hypothetical size; `resolve_flexible_lengths` walks the items in reverse, rounding each share with `from_f64_round`, and the first item takes the rest. The early exits no longer overwrite unfrozen items with their base size | `div-layout` | `crates/div-layout/src/flex/line_flexer.rs` |
| `LayoutUnit::from_f64_round`, Blink's `FromDoubleRound`: round half away from zero, then a saturating cast | `div-geometry` | `crates/div-geometry/src/layout_unit.rs` |

The early-exit change cannot alter a first pass. In grow mode the initial
free space is positive. In shrink mode it is zero only when every unfrozen
item's base size equals its hypothetical size. It follows Blink for later
passes.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Blink's arithmetic | `cargo test --locked -p div-layout --lib line_flexer` (Windows) | 14 passed. New: 69.697/30.303 over 396px gives exactly 276 and 120; grow 10/70/20 over 792px gives 79.203125, 554.390625 and 158.40625; a shrink case worked from Blink's formula; `from_f64_round` rounding, NaN and saturation |
| Layout and geometry crates | `cargo test --locked -p div-layout -p div-geometry --no-fail-fast` (Windows) | 6,068 passed, 0 failed, 5 ignored |
| Paint, engine and `divui` | `cargo test --locked -p div-paint -p div-engine -p divui --no-fail-fast` (Windows) | 2,073 passed, 0 failed, 12 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify` | pass |
| sample-interactions probe `layout.flex-grow-fractions` | `cargo test --test probe_layout fractional_flex_grow -- --nocapture` in the sample, with its Div pin changed locally to each commit | Div `main` `5a315f42`: 3 of 24 boxes equal Chrome for Testing 147.0.7727.50, 6 of 6 cases differ by 1/64 px. This change (`e6203f64`): 24 of 24 equal, no case differs |
| Expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, head `e6203f64`, against the run of `5b7b8e47` (build-with-div/div#13, whose engine code is `main`'s) | 22,520 of 23,728 exact, up from 22,519; Chromium identical in all comparisons; one gain, no loss ([delta](evidence/flex-grow-rounding-div-census-v1.json)) |
| CSS Grid coverage (WSL) | coverage matrix, same head and base | 2,227 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/flex-grow-rounding-div-coverage-v1.json)) |

The census gain is `wpt/css_flexbox/table-item-flex-percentage-width` at
`desktop-1920x1080@1.5`. It went from different, with 240 pixels
mismatched, to exact.

The evidence files are in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data);
`python3 tools/fidelity_data.py fetch --profile all` restores them here.

The census measured `e6203f64` on `main` at `5a315f42`. The branch was then
rebased on `ac4f40d8` (build-with-div/div#34). That change touches only CI
and `tools/fidelity_data.py`, so the measured layout code is the head's.

The sample probe's own assertion still expects the old failures, so it now
fails by design. The sample updates it when it moves its Div pin.

## Not done here

- The react-resizable-panels run (99 differing boxes) was not rerun here.
  The sample reruns it after its pin moves.
