# Grid auto-placement starts from the first cell, 10 November 2026

Auto-placed grid items now start from the grid's first row and column, as
in Chromium, once the items with a definite position are placed. Before
this change, a definite item moved the auto-placement cursor past itself,
so the next auto-placed item could skip a whole row. The components track
found this in the shadcn card header. The action, at
`grid-column: 2; grid-row: 1 / span 2`, pushed the title and description
down one row, and the header was 52px tall instead of Chromium's 44px.
The components track measured it again on Div `main` at `154d0a72`
(`card-action`, light and dark, `legacy-800x600@1`): every box after the
header was 8px lower than Chromium's.

## Finding

`place_items` in `crates/div-layout/src/grid.rs` places items in the order
of the steps of the CSS Grid placement algorithm. Fully definite items go
first, then items locked to a track of the auto-flow axis, then the rest.
In sparse packing it moved one shared cursor to the end of every item it
placed, whatever the step. Two related differences were in the same code:

- A row auto-flow item with a definite column searched down from the
  cursor's row, even when its column was behind the cursor.
- Items locked to a row each searched from the first column, not from the
  end of the previous item locked to that row.

Chromium 147 follows the specification in
`GridPlacement::RunAutoPlacementAlgorithm`
(`third_party/blink/renderer/core/layout/grid/grid_placement.cc`):

1. `PlaceNonAutoGridItems` places the fully definite items.
2. `PlaceGridItemsLockedToMajorAxis` places items locked to a track of the
   auto-flow axis. With sparse packing, each such track keeps its own
   cursor (`minor_cursors`), at the end of the last item placed in it.
3. Step 4 creates a new `AutoPlacementCursor` at the first row and column
   and places the rest in order. For an item with a definite position in
   the other axis, `AutoPlacementCursor::MoveToMinorLine` moves the cursor
   to the next track of the auto-flow axis when that position is behind
   the cursor. Each placed item leaves the cursor at its own end.

These sources were read at 147.0.7727.24, the local source checkout, which
is on the same branch as the pinned 147.0.7727.50.

The pinned Chromium was then measured.
`tools/qualification/capture_grid_placement_oracle.py` lays out four grids
with fixed-size items in the pinned Windows Chrome-for-Testing
147.0.7727.50. It records each item's box relative to its grid in
[the oracle](../../tests/fidelity/fixtures/grid-placement-cursor-v1/chromium-windows.json):

| Case | Chromium | Div before |
| --- | --- | --- |
| Card header: title, description, action at `grid-column: 2; grid-row: 1 / span 2`, `gap: 8px` | title y 0, description y 24 | title y 8, description y 32 |
| Row flow, three 50px columns: `a` at `grid-column: 2`, then `b` at `grid-column: 1` | `b` at x 0, y 20 | `b` at x 0, y 0 |
| Row flow: `x` at row 1 column 2, `l1` at row 1 spanning 2, `l2` at row 1 | `l2` at x 200 | `l2` at x 0 |
| Column flow, two 20px rows: `d` at row 2 column 1, then two auto items | auto items at x 0 and x 50, both y 0 | x 50 y 0 and x 50 y 20 |

## Change

| Change | Owner | Source |
| --- | --- | --- |
| Items placed in steps 1 and 2 no longer move the step 4 cursor. | `div-layout` | `crates/div-layout/src/grid.rs` |
| In sparse packing, an item locked to a track of the auto-flow axis starts after the last item locked to that track. | `div-layout` | same |
| A step 4 item with a definite position behind the cursor starts on the next track of the auto-flow axis. | `div-layout` | same |
| The four oracle cases are tests that read the oracle file. `div-layout` gains `serde_json` as a dev-dependency to read it. | `div-layout` tests | `crates/div-layout/tests/grid_auto_placement_cursor_tests.rs` |
| The components renderer checks the `card-action` header at 1x, light and dark: 44px tall at y 41, title at y 41, description at y 65, Chromium's rectangles as the components track measured them. | components renderer tests | `tests/fidelity/components/render/src/main.rs` |

Dense packing is unchanged: it already restarted from the first cell for
every item.

## Verification

Measured on this branch in Div, rebased onto `main` at `154d0a72`. The
Windows rows use the CI Skia flags.

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `python tools/qualification/capture_grid_placement_oracle.py --chrome E:\open-ui\out\chrome-win\win-147.0.7727.50\chrome-win64\chrome.exe` | 4 cases, table above |
| The four cases against the oracle | `cargo test -p div-layout --test grid_auto_placement_cursor_tests` (Windows) | 4 passed. With `main`'s `grid.rs` all 4 fail, with the "Div before" boxes above |
| The shadcn card header | `cargo test -p div-components-render the_card_header` (Windows) | passed. With `main`'s `grid.rs` it fails: the header is 52px tall |
| Touched crates | `cargo test -p div-layout -p div-engine -p div-components-render --no-fail-fast` (Windows) | 6,475 passed, 0 failed, 8 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify` | all pass. `generate_v02_contract.py` regenerated `docs/v02/generated/dependency-policy.json`, whose lockfile hash changes with the new dev-dependency |
| Expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, under `census_lock.py` | not yet run (below) |
| CSS Grid coverage (WSL) | coverage matrix, under `census_lock.py` | not yet run (below) |

The census and CSS Grid coverage have not run on this branch yet. On 16
November the census lock is held for the fidelity lane's real-zoom oracle
switch (RFC 0010), which recaptures the Chromium frames at 1.25x, 1.5x and
2x; 1x is unchanged. Both run under the lock once it frees: at 1x against
`main`, and at the other profiles against the new real-zoom baseline once it
exists. This PR does not merge before then.

### Measured in Open UI before the move to Div

The change was first made and measured in Open UI (`zhuowcui/open-ui`), as
`c8a65d72` on `carry/grid-auto-placement-cursor`. Those measurements used
Open UI's crates and its earlier baselines. They are history, not
measurements of this branch:

- The paired expanded census, at all four profiles, had no change: 22,517
  of 23,728 exact before and after.
- CSS Grid coverage gained 28 exact comparisons and lost none. Seven tests
  gained in all four profiles: `grid-percent-cols-filled-shrinkwrap-001`
  and its reference, the reference of
  `grid-percent-rows-filled-shrinkwrap-001`, and the references of
  `subgrid/grid-gap-004`, `grid-gap-larger-001`, `grid-gap-normal-001` and
  `grid-gap-smaller-001`. The other 68 changed images all had fewer
  mismatched pixels.
- The components track built its card fixtures with this change and two
  other layout changes, since merged to Div. Every components layout then
  matched Chromium within 1/128 px.

## Not done here

- The auto-placement scan still wraps at the explicit grid's track count.
  Blink wraps at the implicit grid's, which also counts tracks created by
  definite items beyond the explicit grid (`minor_max_end_line_`).
- Blink's `MoveToNextMajorLine` can jump to the end of an overlapping
  item; Div steps one track at a time. Whether the two can ever choose
  different positions was not checked.
- The components track's reference evidence,
  `tests/fidelity/components/evidence/reference-v1.json`, records the
  `card-action` pages as different. It is the components track's
  measurement, to update when it next measures.
