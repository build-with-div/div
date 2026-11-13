# Nested column flex containers lay each item out once per pass, 13 November 2026

Nested `display: flex; flex-direction: column` containers no longer double
the layout work at each level. The first Div sample app,
`build-with-div/sample-git`, found this (gap 26): its commit list sits
inside six nested column containers, and laying out 100 rows took 7.2 s in
a release build. It now takes 0.13 s, with the same geometry.

## Finding

Nothing in `div-layout` kept a layout or intrinsic-size result, and a
column flex container asks for each item twice:

- `resolve_content_based_size` lays the item out for its content-based flex
  basis, and the final pass lays it out again at its used size
  (`crates/div-layout/src/flex/algorithm.rs`).
- `compute_flex_intrinsic_sizes` reads each item's intrinsic block sizes
  through its contribution and again for its raw main sizes
  (`crates/div-layout/src/intrinsic_sizing.rs`).

Each nested container repeats both for everything below it, so the work
doubled at every level: six nested columns laid their innermost items out
32 times.

Chromium 147 keeps layout results on the box. `LayoutBox` holds
`CachedLayoutResult` and `SetCachedLayoutResult`, and intrinsic inline sizes
in `CachedIntrinsicLogicalWidths`
(`third_party/blink/renderer/core/layout/layout_box.h`). It measures block
sizes by laying the box out, so a second request with the same constraint
space returns the cached result. An item whose constraints are unchanged is
laid out once. This source was read at 147.0.7727.24, the local source
checkout, which is on the same branch as the pinned 147.0.7727.50.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| A per-pass memo holds flex item fragments by box and constraint space, and intrinsic block sizes by box and used border, while the outermost flex container on the thread is laid out. | `div-layout` | `crates/div-layout/src/pass_cache.rs` |
| Flex item layout and intrinsic block sizing go through the memo. | `div-layout` | `crates/div-layout/src/flex/algorithm.rs`, `crates/div-layout/src/intrinsic_sizing.rs` |
| The column measurement that treats the item's main size as `auto` (`resolve_content_based_size` with `replaces_main_size`) lays the item out in the document itself when that size is already `auto`, so it shares the memo. | `div-layout` | `crates/div-layout/src/flex/algorithm.rs` |

The document is borrowed immutably while the memo lives, and both
computations are pure functions of the document, the box and the keyed
inputs, so a hit returns exactly what recomputing would. The memo is
narrower than Chromium's cache:

- It caches only the plain document. Style-overriding contexts (the flex
  measurement context and the fieldset content context) bypass it.
- Constraint spaces with shared mutable state (line-clamp and first-line
  contexts), float exclusions or fragmentation bypass it.
- The key destructures `ConstraintSpace`, so a new field fails to compile
  until it is keyed or excluded.
- It is dropped when the outermost flex container finishes. Nothing is kept
  between layouts, so no invalidation is needed.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Work grows linearly with nesting | `cargo test --locked -p div-layout pass_cache` (Windows) | 1 passed. The test counts item layouts and intrinsic block sizings for 1 to 8 nested columns around 4 rows: `[19, 32, 37, 42, 47, 52, 57, 62]`, 5 more per level. With the memo disabled it fails: `[35, 91, 223, 527, 1215, 2751, 6143, 13567]` |
| Layout, paint and engine crates | `cargo test --locked -p div-layout -p div-paint -p div-engine --no-fail-fast` (Windows) | 7,732 passed, 0 failed, 9 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify` | pass. The author-style inventory lists the new test's style fields |
| Expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, head `18fffb9c`, against the run of `5b7b8e47` (build-with-div/div#13, whose engine code is `main`'s at `7e0a2674`) | 22,519 of 23,728 exact before and after; Chromium identical in all comparisons; 0 Div images changed ([delta](evidence/flex-pass-cache-div-census-v1.json)) |
| CSS Grid coverage (WSL) | coverage matrix, head `18fffb9c`, against the run of `5b7b8e47` | 2,227 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/flex-pass-cache-div-coverage-v1.json)) |

The evidence files are in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data);
`python3 tools/fidelity_data.py fetch --profile all` restores them here.
The branch's second commit changes only a comment. The branch was later
rebased on `5a315f42` (build-with-div/div#26), which changes only
`tools/migrate`, and then on `ac4f40d8` (build-with-div/div#34), which
changes only CI and `tools/fidelity_data.py`, so the measured engine code
is the head's.

A local release probe on the native API (100 one-line rows in a 785px
column; not committed) timed one layout at each nesting depth. On `main`
with build-with-div/div#13, depth 6 took 7.2 s, about 2.5 times depth 5.
With this change depths 0 to 8 take 0.04 to 0.19 s. Rows are 785×18 and
the list 1,800px tall at every depth, before and after. These are timings
on one shared Windows host and are not a published performance baseline.

## Not done here

- The memo lives for one flex pass only. Chromium also keeps results across
  frames; Div relays out the whole tree on each change, which the long-list
  profiling (A5) measures next.
- Only flex item fragments are memoized; block, grid and table children are
  laid out as before. No measured case repeats their work at each nesting
  level.
