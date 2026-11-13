# A column flex item's automatic minimum is measured at its cross size, 10 November 2026

A column flex item with `min-height: auto` no longer grows taller than its
content when the content would wrap at a narrower width. Its content size
suggestion (the automatic minimum's content part) is now measured at the
item's used cross size, as Chromium measures it, not at its min-content
width. The components track found this: a text item in a column container
was clamped to one line per word.

## Finding

For a block-axis main size, `resolve_main_axis_min_max` in
`crates/div-layout/src/flex/algorithm.rs` took the content size suggestion from
`compute_intrinsic_block_sizes_with_layout`. For an item with no definite
inline constraint, that is the block size of a layout at the min-content
inline size. Text that fits on one line at the stretched width then
measures one line per word: for "XX XX XX" at a 20px font and line height
in a 300px wide column container, the minimum was 60px, three lines, and
the item could not be smaller.

Chromium 147 takes the content size suggestion for a block-axis main size
from `BlockSizeFunc(SizeType::kContent)`, inside `FlexLayoutAlgorithm`'s
content suggestion lambda (`third_party/blink/renderer/core/layout/flex/flex_layout_algorithm.cc`).
`BlockSizeFunc` lays the item out in `child_space`, the flex basis space,
whose inline size is the stretched or fit-content cross size, and returns
`layout_result->IntrinsicBlockSize()`: the content's block size, which does
not depend on the item's own `height`. For the example that is one line,
20px. This source was read at 147.0.7727.24, the local source checkout, which
is on the same branch as the pinned 147.0.7727.50.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| In a column container, an item whose inline flow is horizontal, left to right and unconstrained (no definite inline size and no semantic break) takes its content size suggestion from `resolve_content_based_size`: a layout at the used cross size with the item's main size treated as `auto`. | `div-layout` | `crates/div-layout/src/flex/algorithm.rs` |
| Row containers with orthogonal items, and items with a definite inline constraint, a semantic break, a right-to-left direction or an aspect ratio, keep the previous measurement. | `div-layout` | same |
| `resolve_content_based_size` loses its unused constraint space parameter. | `div-layout` | same |

Treating the main size as `auto` matches `IntrinsicBlockSize()`, which
ignores the item's own `height`. The existing test
`min_auto_column_flex_min_height` (`crates/div-layout/tests/flex_fixes_validation.rs`)
covers this: an empty item with `height: 150px` in a 100px column container
must shrink to fit, because its content suggestion is 0. An earlier draft of
this change measured at the item's height, made the suggestion 150px, and
failed that test.

Items with an aspect ratio keep the previous measurement because
`resolve_content_based_size` returns the size the ratio gives when the
cross size is definite, not the content's block size. The first census of
this change, at `f5118c44`, found this: WPT
`css/css-sizing/aspect-ratio/flex-aspect-ratio-050.html` and `-052.html`
went from exact to different at all four profiles. Each is a column item
with `aspect-ratio: 2/1`, 100px wide, holding 100px of content in a 0px
tall container. Its automatic minimum must be the 100px content height, and
the new path gave the 50px from the ratio.

## Verification

The change was made and measured in Open UI (`zhuowcui/open-ui`) before the
move to Div. Its last Open UI head, `69f544e3` on `main` at `29104c15`, was
never pushed there. The commands below use Open UI's crate names
(`openui-*`, `openui`); the Div rows at the end are from this branch in Div.
The evidence files are in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data);
`python3 tools/fidelity_data.py fetch --profile all` restores them here.

| Check | Command | Result |
| --- | --- | --- |
| Text item in a 300px column container, `flex-basis: auto` and `0`, with `flex-grow` | `cargo test -p openui-layout --test sp17_auto_height_flex_basis_tests column_auto_minimum` (Windows) | 2 passed. Each expects a 20px item; before the change each was 60px |
| The aspect-ratio WPT cases, `width: auto` and `100%` | `cargo test -p openui-layout --test sp17_auto_height_flex_basis_tests column_aspect_ratio` (Windows) | 1 passed, expecting a 100px item. Without the aspect-ratio exclusion it fails with 50px |
| Layout crate | `cargo test -p openui-layout --no-fail-fast` (Windows) | 5,955 passed, 0 failed, 5 ignored |
| Generators | `generate_properties.py --check`; `generate_v02_contract.py --check`; `generate_renderer_contract.py --check` | no drift |
| `cargo fmt --all --check` | in `bindings/rust` | pass |
| Paired expanded census (WSL) | `wsl-branch-census-a8.sh`, base `9d7bccf6` | 22,517 of 23,728 exact before and after; 0 changed |
| CSS Grid coverage (WSL) | coverage matrix, base `04d1eeab` | 2,218 of 4,204 exact before and after; 0 changed |
| Div: layout crate | `cargo test --locked -p div-layout --no-fail-fast` (Windows), rebased on `main` at `774cf940` | 5,962 passed, 0 failed, 5 ignored: `main`'s 5,959 plus this change's 3 tests |
| Div: generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check` | pass |
| Div: expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, head `5b7b8e47`, against Div `main` at `ee5cc5a8` | 22,519 of 23,728 exact before and after; 0 Div images changed ([delta](evidence/column-flex-min-size-div-census-v1.json)) |
| Div: CSS Grid coverage (WSL) | coverage matrix, head `5b7b8e47`, against Div `main` at `ee5cc5a8` | 2,227 of 4,204 exact before and after; 0 Div images changed ([delta](evidence/column-flex-min-size-div-coverage-v1.json)) |

The paired expanded census in WSL has no change. The base is `9d7bccf6`,
the recorded census base; the head is `0fba067e`, this change. Both are
built with the CI `SKIA_GN_ARGS` and share one Chromium cache. Chromium
and Open UI are each identical in all 23,728 comparisons: 22,517 exact
before and after, with nothing gained or lost. No Open UI image changed,
so the two aspect-ratio tests that the first census lost are exact again.
The delta is in the
[census evidence](evidence/column-flex-min-size-census-v1.json).

The CSS Grid coverage suite has no change either
([coverage evidence](evidence/column-flex-min-size-coverage-v1.json)). The
base is `04d1eeab`, the recorded coverage baseline; the head is `0fba067e`.
In all 4,204 measured comparisons the status and the Open UI image are
unchanged: 2,218 exact before and after. The two references that the
baseline lists as unmeasured, whose renderer memory is unbounded, are left
out of both runs.

The Div census reran both suites on this change in Div. The head is
`5b7b8e47`, this branch rebased on `main` at `774cf940`; the base is the run
of `ee5cc5a8`, the stretch fix of build-with-div/div#12. Chromium is
identical in all 23,728 expanded comparisons and no Div image changed:
22,519 exact before and after. In CSS Grid coverage the status and the Div
image are unchanged in all 4,204 comparisons: 2,227 exact. The branch was
later rebased on `565db46a` (build-with-div/div#29), which changes only
`div-macros`, `divui` and documentation, so the measured engine code is the
same as the head's.

The components track also previewed this change. That preview is not
evidence: it used the system FreeType so that WOFF2 text loads, and its base
was the components branch, not `main`. It applied only
`crates/div-layout/src/flex/algorithm.rs` from `96a9fb47`. Element border boxes
within 1/128 px of Chromium's `getBoundingClientRect` went from 152 to 184
of 192, and `card-basic` is 190px tall as in Chromium (274px before). Forty
images changed, all closer to Chromium, and the other 152 were
byte-identical. The 8 cases left are the grid `auto` column described below.

The first Div sample app, `build-with-div/sample-git`, hit the same bug.
In a `display: flex; flex-direction: column` container each one-line text
row was 72px tall: the text wrapped at its min-content width. Pinned Chrome
147 on Windows lays each row out on one line, 18px tall. With this change
Div's rows are also one line and 18px, at every nesting depth from 1 to 6.
This was checked with a local probe on the native API (100 rows in a 785px
column), which is not a committed test; the `column_auto_minimum` tests
cover the same measurement.

## Not done here

- Right-to-left and vertical items keep the min-content measurement; no
  measured case shows them differing from Chromium yet.
- Row containers with orthogonal (vertical writing mode) items keep the
  previous path.
- The CSS Grid auto track sizing gap the components track also found
  (auto tracks get only their minimum contribution, with no maximize step)
  is a separate fix.
