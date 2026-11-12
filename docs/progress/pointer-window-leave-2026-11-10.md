# A pointer leaving the window dispatches `pointerleave`, 10 November 2026

When the pointer leaves the window, every node on its hover path now
receives `pointerleave` and loses `:hover`. Before this change, no node
received `pointerleave`, and the hovered element received a
`pointercancel`. This is the window-exit fix in
[RFC 0001](../alpha/rfcs/0001-typed-style-rules.md) (state sources; plan
unit A1, Workstream A). Hover-driven style rules need it: without it, a
`:hover` rule would end without the matching boundary events.

## Finding

The desktop adapter sends `PointerPhase::Leave` on winit's `CursorLeft`
with the last cursor position, which is inside the window
(`crates/div-platform/src/desktop.rs`). `App` mapped `Leave` to
`PointerEventKind::Cancel` (`crates/divui/src/app.rs`). `Engine::pointer_event`
then hit-tested that position, found the same path, and so reported no
nodes left. It still removed the hover path. `Document::dispatch_pointer_event`
then sent `pointercancel` to the hit element, and the cancel bubbled to its
ancestors.

Chromium 147 does not hit-test a mouse leave. `EventHandler::HandleMouseLeaveEvent`
(`third_party/blink/renderer/core/input/event_handler.cc`) calls
`HandleMouseMoveOrLeaveEvent`, which for `kMouseLeave` skips the hit test,
because another frame may be above this one at that point. It calls
`Document::UpdateHoverActiveState(request.Active(), false, nullptr)`, which
clears hover. When the mouse is pressed, `request.Active()` is true, and
`Document::UpdateActiveState` (`core/dom/document.cc`) keeps the frozen
`:active` chain until the mouse is released. It then dispatches the pointer
move with a null target, so only boundary events fire on the old path; there
is no `pointercancel`. These sources were read at 147.0.7727.24, the local
source checkout, which is on the same branch as the pinned 147.0.7727.50.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `Engine::pointer_leave(pointer_id)` reports the pointer's whole hover path as left, from the former target toward the root, and removes it without a hit test. The pressed target and pointer capture are kept, so a later pointer-up still clicks. | `div-engine` | `crates/div-engine/src/interaction.rs` |
| `Document::dispatch_pointer_leave` dispatches `pointerleave` (`mouseleave` for mouse names) to each node, target-only, as the existing boundary events are. Nothing else is dispatched. | `divui` | `crates/divui/src/document.rs` |
| `App` routes `PointerPhase::Leave` to it, not to the cancel path. `PointerPhase::Cancel` is unchanged. | `divui` | `crates/divui/src/app.rs` |

The public `PointerEventKind` enum is unchanged, so code that matches on it
still compiles; the leave is a separate method.

## Verification

The change was made and measured in Open UI (`zhuowcui/open-ui`, branch
`engine/a1-pointer-window-leave`, head `94b46632`) before the move to Div;
the commands below use its crate names (`openui-engine`, `openui`). The Div
rows at the end are from this branch in Div. The evidence files are in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data);
`python3 tools/fidelity_data.py fetch --profile all` restores them here.

| Check | Command | Result |
| --- | --- | --- |
| Engine: the whole path is left, the press is kept, re-entry and click still work | `cargo test -p openui-engine --lib leaving_the_window` (Windows) | 1 passed |
| Document: leave order innermost first, a second leave dispatches nothing, a press survives leaving and coming back | `cargo test -p openui --test pointer_window_leave` (Windows) | 2 passed |
| Platform event path: `PointerPhase::Leave` through `App` | `cargo test -p openui --features windows --lib leaving_the_window` | 1 passed. With the new `App` arm disabled it fails: the log is `["pointercancel inner", "pointercancel outer"]`, expected `["pointerleave inner", "pointerleave outer"]` |
| Touched crates | `cargo test -p openui-engine -p openui --features openui/windows` (Windows) | 771 passed, 0 failed, 11 ignored. The engine test above was then rewritten to destructure its result, and passes again. After rebasing onto `main` at `c4b2e42e`: 772 passed, 0 failed, 11 ignored. After rebasing onto the `style!` macro (PR #111), whose `openui` tests are added: 781 passed, 0 failed, 11 ignored, and the same on `main` at `c48c2e38` (PRs #111 and #116 merged) |
| Generators | `generate_properties.py --check`; `generate_v02_contract.py --check`; `generate_renderer_contract.py --check` | no drift |
| `cargo fmt --all --check` | in `bindings/rust` | pass |
| Paired expanded census (WSL) | `wsl-branch-census-a8.sh`, base `9d7bccf6` | 22,517 of 23,728 exact before and after; 0 changed |
| CSS Grid coverage (WSL) | coverage matrix, base `04d1eeab` | 2,218 of 4,204 exact before and after; 0 changed |
| Div: touched crates | `cargo test --locked -p div-engine -p divui --features divui/windows` (Windows) | 781 passed, 0 failed, 11 ignored (as in Open UI) |
| Div: evidence pin | `python tools/fidelity_data.py fetch --profile all`, then `verify --profile all` | 30,474 files match data commit `41f5d6a6` |
| Div: generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check` | pass |

The paired expanded census in WSL has no change. The base is `9d7bccf6`,
the recorded census base; the head is `146160a5`, this change. Both are
built with the CI `SKIA_GN_ARGS` and share one Chromium cache. Chromium
and Open UI are each identical in all 23,728 comparisons: 22,517 exact
before and after, with nothing gained or lost. The census renders static
documents and sends no pointer events, so this checks that rendering is
unchanged. The delta is in the
[census evidence](evidence/pointer-window-leave-census-v1.json).

The CSS Grid coverage suite has no change either
([coverage evidence](evidence/pointer-window-leave-coverage-v1.json)). The
base is `04d1eeab`, the recorded coverage baseline; the head is `146160a5`.
In all 4,204 measured comparisons the status and the Open UI image are
unchanged: 2,218 exact before and after. The two references that the
baseline lists as unmeasured, whose renderer memory is unbounded, are left
out of both runs.

## Not done here

- Open UI has no `pointerout` / `mouseout` events, which Chromium also
  dispatches on leave; this is unchanged for every boundary.
- The C ABI input path (`div-ffi`) has no leave code, so a C host can
  report only a cancel, as before.
- Re-hit-testing the last pointer position after a scroll or layout change,
  and `:active` on the pressed target's ancestors, are separate RFC 0001
  fixes.
