# Components track progress

Workstream C of the [alpha plan](../../alpha/plan.md): shadcn/ui components
for Div under the working name `div-components`, each measured
against shadcn/ui rendered by pinned Chromium 147. The code is
[`crates/div-components`](../../../crates/div-components/README.md) and the
reference pages, harness and evidence are in `tests/fidelity/components`.
Results are from this track's harness; they are not census results and do
not qualify the renderer.

Entries before 12 November 2026 were recorded in the archived
[zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) repository. Their
commit hashes and PR numbers refer to that repository; there, the track was
a separate `components/` workspace and the public crate was `openui` (now
`divui`).

## Results

| Component | Fixtures | Exact at @1 / @2 / @1.25 / @1.5 | Evidence | Measured at (Div commit) |
| --- | --- | --- | --- | --- |
| badge | 6 variants; light, dark | 5 / 5 / 0 / 0 of 12 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| button | 6 variants, sizes xs, sm and lg, disabled; light, dark | 19 / 0 / 0 / 0 of 20 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| card | basic, with action; light, dark | 0 / 0 / 0 / 0 of 4 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| checkbox | unchecked, checked, disabled, disabled and checked, invalid; light, dark | 6 / 6 / 0 / 0 of 10 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| font | Geist, Geist Mono, Inter; light, dark | 2 / 0 / 0 / 0 of 6 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| input | email input: placeholder, value, disabled, invalid; light, dark | 0 / 0 / 0 / 0 of 8 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| label | basic; light, dark | 2 / 0 / 0 / 0 of 2 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| separator | horizontal, vertical; light, dark | 4 / 4 / 0 / 0 of 4 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| switch | unchecked, checked, disabled, disabled and checked; sm unchecked and checked; light, dark | 1 / 0 / 0 / 0 of 12 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |
| tabs | horizontal, vertical, second tab selected; light, dark | 3 / 0 / 0 / 0 of 6 | [`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json) | `06b9002e`, 17 November 2026 |

The evidence covers all four census profiles (`"complete": true`). Above
1x the Chromium oracle is captured with `--force-device-scale-factor` by
the fidelity lane's shared launcher ([RFC 0010](../../alpha/rfcs/0010-real-zoom-oracle.md));
this baseline starts on 17 November 2026. The four-profile results of
12 November (16 of 216 exact, at `6fc0c1a7`, with CDP's emulated device
scale) are history, in the [12 November entry](#12-november-2026-tabs-and-the-roving-focus-primitive-c2-group-1).

Behavior is compared with Radix running in Chromium by the traces in
[`traces.json`](../../../tests/fidelity/components/traces.json):

| Component | Traces | Same | Evidence | Measured at |
| --- | --- | --- | --- | --- |
| tabs | keyboard, pointer, vertical keyboard, manual activation, leaving with Shift+Tab | 4 of 5 | [`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json) | `06b9002e`, 17 November 2026 |
| switch | keyboard (Space, Enter), pointer, disabled | 3 of 3 | [`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json) | `06b9002e`, 17 November 2026 |
| input | focus by Tab, focus by a click, disabled | 3 of 3 | [`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json) | `06b9002e`, 17 November 2026 |
| checkbox | keyboard (Space, Enter), pointer, disabled | 3 of 3 | [`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json) | `06b9002e`, 17 November 2026 |

## 23 November 2026: the dialog accessibility role (H4, RFC 0015)

A `<dialog>` reported `GenericContainer`. It now reports the existing
`Dialog` role, and an authored role still takes priority. This adds no enum
or FFI value: FFI consumers see the existing value 12 for a dialog.

### Change

| Change | Owner | Source |
| --- | --- | --- |
| `named_role` gives the element named `dialog` the `Dialog` role | engine | `crates/div-engine/src/accessibility.rs`; `dialog-accessibility-v1` |
| `aria-modal` changes only a dialog's reported flag, never inertness: absent or empty is no override, `false` ignores case, any other value is true | engine | same file and oracle |
| A dialog whose role is neither `Dialog` nor `AlertDialog` has no modal flag | engine | same file and oracle |
| The element table reads `named_role`; the dialog page describes the role | website | `site/scripts/gen-reference.mjs`, `site/src/data/elements.json`, `site/src/content/docs/elements/dialog.mdx` |

The oracle is Chromium's computed accessibility tree read through CDP, not
what a screen reader receives from the operating system.
`tools/qualification/capture_dialog_accessibility_oracle.py` recorded 35
cases from pinned Chromium 147.0.7727.50, in two identical captures.

### Remaining failures

Eight of the 35 cases keep a field in the test's `PENDING` ledger. A pending
field that starts to match fails the test, so the ledger can only shrink.

| Gap | Cases | Owner |
| --- | --- | --- |
| `aria-labelledby`, `aria-describedby` and `title` are not read | `show_labelledby`, `modal_title`, `modal_labelledby_inside`, `show_describedby`, `modal_describedby_inside` | engine |
| The `role` attribute is not read | `show_role_alertdialog`, `modal_role_alertdialog`, `modal_role_button` | engine |

### Verification

The Rust rows ran at `396a828` on `main` at `cac87c5`, in WSL with the Skia
flags of `ci.yml` at that commit.

| Check | Command | Result |
| --- | --- | --- |
| Dialog tests in the engine | `cargo test --locked -p div-engine --test dialog_accessibility_oracle --test dialog --test dialog_oracle` | 14 passed |
| Engine accessibility unit tests | `cargo test --locked -p div-engine --lib accessibility` | 8 passed |
| Framework | `cargo test --locked -p divui --test dialog_oracle --test dialog_element --test native_focus --test v02_conformance` | 88 passed |
| FFI accessibility | `cargo test --locked -p div-ffi --lib accessib` | 9 passed |
| Formatting | For each changed Rust file, under a memory cap: `rustfmt --edition 2021 --emit stdout < file.rs \| cmp - file.rs` | both match |
| Generated element table | in `site/`: `node scripts/gen-reference.mjs --check` | current |
| Census and coverage (D4) | not run | Not measured for this change |

## 22 November 2026: the engine top layer without paint (H4, RFC 0015 PR 3a)

The engine learns `<dialog>`'s state, the top layer, inertness and the
focusing steps, and replays the dialog oracle's no-paint scenarios. PR 3 is
split: the top-layer paint pass waits for the SVG lane's A1, which also
changes `painter.rs`, and becomes 3b; the UA element rules and the
`divui` tag move from PR 4 into 3a (RFC 0015's Migration table is
amended). The engine lane reviews this PR, and the framework the `divui`
tag line.

### Change

| Change | Owner | Source |
| --- | --- | --- |
| A dialog is a `Div` element named `dialog` through S6a's local name (agreed with the fidelity lane: no new `ElementTag`). `show()`, `showModal()` and `close(returnValue)` follow Blink 147's `HTMLDialogElement`: the `InvalidStateError` cases are named `EngineError::InvalidState`, `showModal()` (re)adds the dialog at the end of the top layer, `close()` restores focus to the opener when the dialog was modal or held focus, and removal from the document, or moving the dialog or an ancestor (`append_or_move_child`, `insert_before`), ends modality. The legacy focusing steps pick the autofocus element, else the first focusable one, skipping inside nested dialogs, else the dialog itself. A dialog is click and script focusable but sequentially focusable only with `tabindex`. | engine (H4, by components under RFC 0015) | `crates/div-engine/src/dialog.rs`, `focus_navigation.rs` |
| Inertness: the scope is the topmost modal dialog, else the `set_modal_root` root. Inert content takes no focus, no hit and no Tab stop, and leaves the accessibility tree. `set_modal_root` now also makes content outside it inert to pointers. | engine | `dialog.rs`, `interaction.rs`, `lib.rs`, `accessibility.rs` |
| `:open` (open `<details>` and `<dialog>`) and `:modal` match as state pseudo-classes. | engine | `selector_match.rs`, `rule_cascade.rs` |
| An always-on user-agent sheet, adopted with the first dialog, holds the `dialog` element rules adapted from the specification (WHATWG HTML 15.3.3 "Flow content", CC BY 4.0, the copy fetched for RFC 0004, cited in the source; not Blink's `html.css`, and not the specification's rules verbatim): `dialog` (absolute, `left`/`right: 0`, `margin: auto`, fit-content size, a 3px solid `currentcolor` border, `1em` padding, black text on white), `dialog:not([open])` (`display: none`) and `dialog:modal` (fixed, `top`/`bottom: 0`, `overflow: auto`). The sheet fails loudly if it drops a declaration. `::backdrop` waits for PR 4. Three named gaps remain (below). | engine | `dialog.rs` |
| `Element::create("dialog")` creates the engine's dialog (a `Div` named `dialog`) through S6b's element table: `dialog` leaves the rejected names, and the uniform named creation runs the dialog setup. | framework (reviewed), by components under RFC 0015 | `crates/divui/src/element.rs`, `crates/divui/tests/dialog_element.rs` |
| The porter's IR records `show()`/`close()` only on element receivers. A receiver made with `new` (a `MessageChannel`'s port, a `BroadcastChannel`) is rejected as `non-element-receiver` with its constructor, and one the porter cannot follow as `unresolved-dialog-receiver` (the engine review of PR 2). | components (for the engine lane) | `tools/wpt/javascript_mutation_ir.mjs` |
| The engine replays the oracle (below); `divui`'s ledger lists fourteen scenarios as replayed by the engine and the rest with what each waits for. | components | `crates/div-engine/tests/dialog_oracle.rs`, `crates/divui/tests/dialog_oracle.rs` |
| The oracle gains four scenarios from pinned Chromium 147: moving a modal dialog and moving its ancestor, each with `appendChild` and `insertBefore`. | components | `tools/qualification/capture_dialog_oracle.py`, `tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json` |
| The site's element table lists `dialog`, with a reference page that says what works today and what does not. | website, by components (the generated table requires a page for each tag) | `site/src/content/docs/elements/dialog.mdx`, `site/src/components/ElementTable.astro`, `site/astro.config.mjs` |

### Measurement

Replayed exactly against `tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json`:

- **Fourteen scenarios** (`modal_open_close`, `modal_tab`,
  `click_dialog_content`, `script_focus_inert`, `autofocus`,
  `no_focusable`, `non_modal_focus`, `non_modal_focus_outside`,
  `non_modal_click_outside`, `remove_open_modal`, `move_dialog_append`,
  `move_dialog_insert_before`, `move_ancestor_append`,
  `move_ancestor_insert_before`): every input event's
  type and target, the focused element and each dialog's `open`, `:modal`
  and `returnValue` after every step. Their `beforetoggle`, `toggle`,
  `cancel` and `close` events are `divui`'s: a pending field owned by PR 5.
  At the move step of the four move scenarios, `active` is pending on the engine ("clear focus inside a moved subtree"): Chromium clears focus there and Div keeps it, a gap already on `main`. The next step's click compares focus again.
- **All 15 geometry records**, field by field at 480x400, 800x600 and
  375x667: the border box, the client and scroll sizes and the 16
  computed-style properties, with `auto` insets and margins resolved from
  Div's layout. Pending on engine 0.2 PR 2 (`calc()` in `width`,
  `height`, `min-*` and `max-*`, with `em` and `%`): `maxWidth`/`maxHeight` of the nine modal records, and the
  box, sizes and top margin of the four the calc limits (and the left
  margin of the two tall ones), 32 fields in all.

A pending field that matches fails the test, so each comes off the list
when it is fixed.

Remaining, with owners:

Gaps in the UA rules. The oracle's page is horizontal and light, so it
exercises only the last:

| Gap | Effect | Owner |
| --- | --- | --- |
| Logical insets: `left`, `right`, `top` and `bottom` stand for `inset-inline` and `inset-block` | A non-modal dialog in a vertical writing mode gets the wrong axis | Engine, platform-plan item S7 |
| System colors: white and black stand for `Canvas` and `CanvasText` | A dialog under a dark `color-scheme` property keeps its light colors. With `prefers-color-scheme: dark` and no such property, pinned Chromium keeps them too, so `Document::set_color_scheme` alone must not change them | Engine, with item S7's `color-scheme` property, the only thing that closes this gap |
| The modal `max-width` and `max-height`, `calc(100% - 6px - 2em)`, are left out | A modal dialog larger than the viewport is not clamped | Engine 0.2 PR 2: `calc()` in `width`, `height`, `min-*` and `max-*`, with `em` and `%` |

| Scenario | Waits for |
| --- | --- |
| `click_inert` | PR 4: a press outside the modal dialog reaches its `::backdrop`; Div finds no target |
| `nested` | PRs 4 and 5: the backdrop, then Escape |
| `remove_open_attribute` | engine: Chromium clears focus when the focused element stops being rendered (focus fixup); Div keeps it, a general gap reported for its own PR |
| the six `escape_*`, `return_value`, `request_close_prevented`, `invalid_state`, `beforetoggle_prevented` | PR 5: the `divui` API, events and Escape |

### Verification

The D4 pair measured `77d254d`, on `main` at `4e69bdf` (which has S6b,
#122). The branch was then rebased on `main` at `18ba530`. That only brings in
#128 and #130 (WPT-script ledgers and tools, the CI workflow and progress
notes), and outside those paths the tree is the same as `77d254d`. Each row
names the commit it measured. The local Rust rows ran on Windows with the
VS 2022 build tools. That suite was not run again after the move fix: CI is the
test evidence for the later commits.

| Check | Command | Result |
| --- | --- | --- |
| CI at `77d254d` | GitHub Actions | 12 passed, 6 skipped |
| Engine, framework and FFI suites at `862805f`, before the move fix, under the census lock | `cargo test --locked -p div-engine -p divui -p div-ffi -- --test-threads 4` | 128 test binaries: 1,071 passed, 0 failed, 11 ignored |
| The dialog and selector tests inside that `862805f` run | `div-engine`: `dialog_oracle`, `dialog`, `selector_matching`, `form_state_pseudo_classes`; `divui`: `dialog_oracle`, `dialog_element` | 3, 6, 4, 2, 2 and 1 passed |
| Engine without dev features at `862805f` | `cargo check --locked -p div-engine -p divui` | builds (the UA selectors are values, not parsed at run time) |
| Porter IR and contract tests at `862805f` | `python -m unittest tools.qualification.test_renderer_matrix` and `python -m unittest tools/wpt/test_renderer_generality.py`, with the pinned Acorn | 38 and 7 passed |
| Renderer contract at `862805f` | `python tools/qualification/generate_renderer_contract.py --check` with the pinned Acorn and the WPT sources at `09d377d9` | no drift; `javascript=393` |
| Formatting at `77d254d` | For each changed Rust file, under a memory cap: `rustfmt --edition 2021 --emit stdout < file.rs \| cmp - file.rs` | all 14 match |
| Oracle capture, whole page again, at `a4c365f` | `python3 tools/qualification/capture_dialog_oracle.py --chrome <pinned chrome-linux64/chrome>` in WSL (WSLg) | 27 scenarios and 15 geometry records, both runs identical, 125 s. The 23 earlier scenarios and the geometry are unchanged; 4 scenarios and 5 actions are added |
| Site, as CI builds it, at `4f96bd1` | in `site/`: `npm ci`, `npm run check-reference`, `npm run build`, `npm run build:cover`, with Node 22.18 locally (CI pins 24.22.1) | all pass; 45 pages, including `/elements/dialog/` |
| Harness check, on the previous base `662e366` | `click_inert` added to the engine replay temporarily | fails as expected (no backdrop: the press finds no target) |
| Census and CSS Grid coverage (D4), under the census lock | `4e69bdf` against `77d254d`, compared image by image | No image changed in status, mismatched pixels, diff signature or Div or Chromium hash. Census: 23,728 images on both (16,297 exact, 7,431 different, 0 errors). Coverage: 4,204 images on both (1,473 exact, 2,575 different, 156 errors, all `Div render failed`) |

## 22 November 2026: the WPT porter's dialog mutations (H4, RFC 0015 PR 2)

The WPT porter's static script reader learns `HTMLDialogElement`'s
`showModal()`, `show()` and `close(returnValue)`, so ported tests can state
them. The engine lane, which owns the porter, reviews this PR.

### Change

| Change | Owner | Source |
| --- | --- | --- |
| The mutation IR records `showModal()`, `show()` and `close()` as `dialog-show-modal`, `dialog-show` and `dialog-close`, with `close()`'s constant return value (a runtime value is rejected as `runtime-dependent-mutation-input`). Calls on `window`, `document`, `self`, `globalThis`, `top` and `parent`, and bare `close()`, are not dialogs and stay unrecorded, as before. | components (H4, for the engine lane) | `tools/wpt/javascript_mutation_ir.mjs` |
| The porter lowers `show()` to adding `open` and `close()` to removing it, each a no-op in the state it would produce. `showModal()` and either method on another element fail the port with a named reason (`top layer (RFC 0015 PR 3)`, `not a dialog`). | components (H4, for the engine lane) | `tools/wpt/port_wpt.py` |
| The renderer contract rejects `showModal()` as `top-layer-dependency` until the top layer lands (RFC 0015 PR 3), so such a test stays `behavioral-nonvisual`. | components (H4, for the engine lane) | `tools/qualification/generate_renderer_contract.py` |
| Tests: the IR's operations and return values, and that `window.close()`, `document.close()` and `close()` are not recorded; the contract's rejection; the porter's final state and its failures. | components | `tools/qualification/test_renderer_matrix.py`, `tools/wpt/test_renderer_generality.py` |

### Measurement

None of the renderer contract's 393 JavaScript candidates calls
`showModal()`, `show()` or `close()` (searched in the WPT sources at
`09d377d9`), so the pinned audit,
`docs/renderer/generated/javascript-mutation-audit-v4.json`, does not
change: 201 `lowered-exact`, 35 `ast-lowered-pending-exact` and 157
`behavioral-nonvisual`. The dialog tests in
`html/semantics/interactive-elements/the-dialog-element` are outside the
candidate set and CI's sparse WPT checkout; RFC 0015 PRs 3 to 5 port them.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Renderer contract | `python tools/qualification/generate_renderer_contract.py --check` on Windows, with the pinned Acorn 8.14.0 (sha256 `402f6759…`), the WPT sources at `09d377d9`, the fidelity data fetched and `PYTHONUTF8=1` | no drift; `javascript=393` |
| IR and contract tests | `python -m unittest tools.qualification.test_renderer_matrix` | 38 passed |
| Porter tests | `python -m unittest tools/wpt/test_renderer_generality.py tools/wpt/test_native_mutation_codegen.py tools/wpt/test_port_coverage_area.py` | 7 and 21 passed |

## 21 November 2026: the dialog oracle (H4, RFC 0015 PR 1)

The components track builds platform-plan item H4, the top layer and
`<dialog>`, as [RFC 0015](../../alpha/rfcs/0015-top-layer-and-dialog.md)
proposes. This first PR changes no product code. It records Chromium's
behavior before Div implements any of it.

### Change

| Change | Owner | Source |
| --- | --- | --- |
| A capture script built on the keyboard scrolling oracle's browser, input and settling code: real XTEST keys and buttons to pinned Chrome-for-Testing 147.0.7727.50 on X11 in WSL, at scale 1. It records 23 scenarios on its own page (seven dialogs, three outside buttons and a non-focusable box). After each step it records the input, mouse and key events, `beforetoggle`, `cancel` and any exception in order, and `toggle` and `close` apart and sorted, since they race. It also records the focused element and each dialog's `open`, `:modal` and `returnValue`. A geometry pass records each dialog's box, its computed style and its `::backdrop`'s computed style at 480x400, 800x600 and 375x667, without input. The first move after the window appears sometimes misses under WSLg, so calibration is retried. | components (H4, for the engine lane, which owns `tools/qualification`) | `tools/qualification/capture_dialog_oracle.py` |
| The capture: 23 scenarios and 15 geometry records, captured twice and identical (110 s). | components | `tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json` |
| A replay test that checks the capture's provenance and shape. It lists every scenario as pending on the RFC 0015 PR that makes it replayable, and fails if a captured scenario is neither replayed nor pending. Later PRs replay scenarios and take them off the list. | components (H4, for the framework track, which owns `divui`) | `crates/divui/tests/dialog_oracle.rs` |

### Measurement

What Chromium does, from the capture, now part of RFC 0015:

- **Opening.** `showModal()` and `show()` dispatch a cancelable
  `beforetoggle`, then `toggle`. Focus goes to the autofocus element, else
  the first focusable descendant. A dialog with nothing focusable takes
  focus itself. A cancelled `beforetoggle` stops the open.
- **Closing.** `close()` dispatches a non-cancelable `beforetoggle`, then
  `toggle` and `close`, and restores focus to the opener. A non-modal
  dialog does not restore focus once focus has left it.
- **Escape and `requestClose()`.** Both dispatch a cancelable `cancel`
  first. A cancelled `cancel` keeps the dialog open. A second Escape with
  no click in between dispatches a non-cancelable `cancel` and closes the
  dialog. A click inside the dialog makes the next `cancel` cancelable
  again. A cancelled Escape `keydown` dispatches no `cancel`. Escape in a
  text field closes the dialog. A non-modal dialog ignores Escape.
  `requestClose()` can be cancelled every time.
- **Tab.** Tab and Shift+Tab move within a modal dialog. A Tab past its
  last element leaves the document for the browser's UI: the page loses
  focus, so the capture stops there and the scenarios avoid it. RFC 0015
  asks the user what Div does instead.
- **Inertness.**
  - A press on inert content, or on the backdrop, targets the topmost
    modal dialog and focuses it.
  - Script `focus()` on inert content does nothing.
  - With two modal dialogs open, a press on the lower one targets the
    upper one.
  - Removing an open modal dialog from the document ends inertness.
  - Removing only its `open` attribute leaves it `:modal` and the document
    inert, with presses reaching the root.
- **Exceptions.** `show()` on a modal dialog, `showModal()` on an open
  non-modal dialog, and `showModal()` on a disconnected dialog each throw
  `InvalidStateError`.
- **Geometry.** A modal dialog is `position: fixed`, centered, with
  `max-width` and `max-height` of `calc(100% - 30px)` at 12px (6px plus
  2em). A taller or wider dialog is clamped and scrolls. A non-modal
  dialog is `position: absolute` at the top, centered horizontally. The
  `::backdrop` of a modal dialog is fixed with `inset: 0` and
  `rgba(0, 0, 0, 0.1)`.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Capture | `python3 tools/qualification/capture_dialog_oracle.py --chrome <pinned chrome-linux64/chrome>` in WSL (WSLg) | 23 scenarios and 15 geometry records, both runs identical, 110 s |
| Replay test | `cargo test --locked -p divui --test dialog_oracle` on Windows (CI Skia flags, VS 2022 Build Tools) | 2 passed |
| Formatting | `cargo fmt --all --check` | clean |

## 17 November 2026: checkbox (C2 group 2, C3)

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Checkbox primitive, a port of Radix's `react-checkbox` 1.4.1: a `button` with `role="checkbox"`, `aria-checked` (`mixed` when indeterminate) and `data-state`, toggled by a click and by Space through the button's activation; its Enter `keydown` is cancelled, so Enter does not toggle it, as in Radix. An indeterminate checkbox becomes checked when toggled. The indicator (`CheckboxIndicator`, `pointer-events: none`) exists only while the checkbox is checked or indeterminate, as Radix's `Presence` without `forceMount` does; it is built in the document it was declared in, since the effect that adds it reruns from event handlers. | components | `crates/div-components/src/primitives/checkbox.rs` |
| Styled checkbox, shadcn's `checkbox.tsx`: `size-4`, `rounded-[4px]`, the `border-input` border, `shadow-xs`, `dark:bg-input/30`, the checked `border-primary bg-primary text-primary-foreground`, the disabled styles and `aria-invalid`'s destructive border (a prop, `invalid`). Tailwind orders `data-*` variants after `aria-*` ones, so a checked checkbox keeps `border-primary` when invalid. The `data-state` styles are inline, as for the switch (RFC 0003, request 6). | components | `crates/div-components/src/checkbox.rs` |
| Lucide's `CheckIcon` in the indicator: the outer `svg` (24 units square, `size-3.5`, stroked in `currentColor`) is built, but not its `<path>`: Div's DOM has no SVG shapes yet ([RFC 0009](../../alpha/rfcs/0009-inline-svg.md), V1 stage 1). Nothing stands in for the check. | components | `crates/div-components/src/checkbox.rs` |
| Fixtures `checkbox-unchecked`, `-checked`, `-disabled`, `-disabled-checked` and `-invalid`, light and dark, in a flex row with Radix's server-rendered hidden input, as for the switch; trace pages for the unchecked and the disabled checkbox; traces `checkbox-keyboard`, `checkbox-pointer` and `checkbox-disabled`. shadcn's `checkbox.tsx` is vendored unchanged from `shadcn@4.23.0`; `lucide-react` 1.70.0 (ISC), the release of the bundled `lucide-static`, renders its icon in the reference pages. | components | `tests/fidelity/components/reference/`, `traces.json`, `render/src/main.rs` |

### Measurement

All 336 comparisons at `06b9002e`, in the census real-font environment: 57
exact, 279 different, 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json),
`"complete": true`). The 296 earlier rows have the same images (both
hashes) and status as the real-zoom baseline at `84fe5051`.

| Checkbox page | `legacy-800x600@1` | `mobile-375x667@2` | `desktop-1280x720@1.25` | `desktop-1920x1080@1.5` |
| --- | --- | --- | --- | --- |
| unchecked, disabled, invalid (6) | 6 exact | 6 exact | 6 different | 6 different |
| checked, disabled and checked (4) | 4 different | 4 different | 4 different | 4 different |

13 of 14 behavior traces are the same
([`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json)):
the three checkbox traces (Space toggles and Enter does not, with the
indicator added and removed; two clicks and Space; a disabled checkbox
takes no focus and stays unchecked) and the ten that were the same before.

The geometry diagnostic matches no checkbox page, for two reasons and no
other: the hidden form input (13x13 in Chromium, 0x0 in Div, as on the
switch pages) and, on the checked pages, the check icon's missing
`<path>` element. Every other box is the same at 1x.

### Remaining failures

- **The check is not drawn**, on the four checked pages at every profile
  (at 1x, 34 to 56 pixels, inside the icon's 14px box). Owner: the SVG
  lane and the framework track (RFC 0009: SVG elements in the DOM,
  `Element::create_ns`).
- **Rounded corners of the checked box (light)**: outside the icon box the
  light checked pages differ only on the antialiased corners, by 1 in a
  channel; the switch's pill edges are likely the same. Owner: engine
  track, paint (rounded rectangles).
- **Fractional scales**: every checkbox page differs at 1.25 and 1.5, on
  the box's edges, as every other page does (RFC 0011). Owner: engine
  track.
- **The hidden form input** is laid out 0x0 (Chromium: 13x13); it has
  `opacity: 0`, so the pixels are unaffected. Owner: engine track, forms.
- Not built: `focus-visible` (the 3px ring and the `ring` border, or the
  destructive ring when invalid) and `transition-shadow`; the cascade
  (RFC 0003, request 6). Owner: framework track (typed style rules).

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison, all profiles | `python3 tools/qualification/census_lock.py run --wait -- python3 tests/fidelity/components/harness/reference_harness.py compare --renderer <release div-components-render> --repository-commit 06b9002e… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 57 of 336 exact, 279 different, 0 errors; complete |
| Earlier rows unchanged | each row of the new evidence compared with the `84fe5051` evidence by fixture and profile | 296 of 296 the same images and status; 40 new |
| Behavior traces | `reference_harness.py trace --renderer <release div-components-render> --repository-commit 06b9002e… --output tests/fidelity/components/evidence/traces-v1.json` in WSL | 13 of 14 the same; `tabs-tab-out` as before |
| Geometry, all profiles (diagnostic) | `reference_harness.py geometry --renderer <release div-components-render>` in WSL | 190 of 336 pages with every box the same (62, 56, 36, 36: as before, and no checkbox page) |
| Renderer | `sha256sum` of the release `div-components-render` | `f3ee4be9…`, the SHA-256 in both evidence files |
| Reference pages | `node build.mjs --check` in `tests/fidelity/components/reference` | 97 pages and 27 assets up to date |
| Tests | `cargo test --locked --release -p div-components -p div-components-render` in WSL | all pass, among them `space_toggles_the_focused_checkbox_and_enter_does_not` and `a_disabled_checkbox_stays_unchecked` |
| Formatting and lints | `cargo fmt --all --check`; `cargo clippy -p div-components -p div-components-render --all-targets` in WSL | clean; no new warnings in the components crates |

## 17 November 2026: real-zoom baseline

### Change

| Change | Owner | Source |
| --- | --- | --- |
| `reference_harness.py geometry` launches Chromium for each profile with the shared launcher's flags and viewport override (`device_scale_launch`, [RFC 0010](../../alpha/rfcs/0010-real-zoom-oracle.md)): `--force-device-scale-factor` above scale 1, unchanged at scale 1. It used CDP's emulated `Emulation.setDeviceMetricsOverride`. `compare` already renders through the launcher. The comparison evidence and the geometry output record each profile's `device_scale_method` (`none` or `forced`). | components | `tests/fidelity/components/harness/reference_harness.py` |
| All 74 fixtures measured again at the four census profiles, with real zoom above 1x. This is a fresh baseline: the earlier non-1x results used the emulated oracle and are not compared with it. | components | `tests/fidelity/components/evidence/reference-v1.json`, `traces-v1.json` |

### Measurement

All 296 comparisons at `84fe5051` (main `5231c6cf` with the change above),
in the census real-font environment: 45 exact, 251 different, 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json),
`"complete": true`).

| Profile | Device scale | Exact (of 74) | Geometry the same (of 74, diagnostic) |
| --- | --- | --- | --- |
| `legacy-800x600@1` | 1, none | 36 | 62 |
| `mobile-375x667@2` | 2, forced | 9 | 56 |
| `desktop-1280x720@1.25` | 1.25, forced | 0 | 36 |
| `desktop-1920x1080@1.5` | 1.5, forced | 0 | 36 |

At 1x, ten rows moved since the input run at `3bee58d5`; the other 64
have the same images (both hashes) and status, and Chromium's images of
all 74 are unchanged:

- **card-action** (grid auto-placement cursor, div#14, `6e7f5f02`): the
  card header is 44px tall instead of 52px. 10,196 and 9,059 pixels
  different become 8 (dark, 1 in a channel) and 107 (light, up to 12),
  the same remainder as card-basic; every box is the same as Chromium's.
- **button-disabled** (opacity groups keep LCD text, div#69, `5231c6cf`):
  489 and 502 pixels different become 0. Both pages are exact.
- **tabs**: the text inside every tabs page now matches. The three dark
  pages are exact (358, 358 and 352 pixels before) and the three light
  pages differ in 44 pixels, up to 8 in a channel: the active trigger's
  corners, below. Not attributed to one of the engine changes merged
  between `3bee58d5` and `5231c6cf`.

10 of 11 behavior traces are the same
([`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json)),
as before; `tabs-tab-out` still differs.

### Remaining failures

- **Box edges at fractional scales.** At 1.25 and 1.5 Chromium snaps box
  edges to device pixels and Div paints the fraction as coverage. The
  separator's 1px line is 1.25 device pixels at 1.25: Chromium paints one
  row (`rgb(229 229 229)` at y 20), Div that row and a partial one
  (`rgb(249 249 249)` at y 21). Every page at these two profiles differs.
  Owner: engine track, layout and paint
  ([RFC 0011](../../alpha/rfcs/0011-device-pixel-layout.md)).
- **Text at 2x.** Glyph ink sits one CSS pixel (two device pixels) higher
  in Div: on label-basic, Chromium's ink rows are 36–59 and Div's 34–58,
  in the same columns. Likely the same baseline rounding as the font
  pages', below. Owner: engine track, text (RFC 0011).
- **Line height as `calc()`.** Tailwind's `text-sm` line height is
  `calc(1.25 / 0.875)` and `text-xs`'s is `calc(1 / 0.75)`; Div does not
  accept `calc()` in `line-height`. The font pages use the number
  `1.4285714` (19.9999996px at 14px), so Geist's baseline rounds one pixel
  high (4,451 pixels per page). The components write the computed lengths
  instead: `20px` in `button.rs`, `card.rs`, `input.rs`, `tabs.rs` and the
  tabs panels in `render/src/main.rs`, and `16px` in `badge.rs` and
  `button.rs` (size xs). All switch to shadcn's `calc()` when Div accepts
  it. Owner: engine track, style.
- **Active trigger corners (light)**, from the 12 November entry: 44
  pixels on each light tabs page at 1x.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison, all profiles | `python3 tools/qualification/census_lock.py run --wait -- python3 tests/fidelity/components/harness/reference_harness.py compare --renderer <release div-components-render> --repository-commit 84fe5051… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 45 of 296 exact, 251 different, 0 errors; complete |
| Behavior traces | `reference_harness.py trace --renderer <release div-components-render> --repository-commit 84fe5051… --output tests/fidelity/components/evidence/traces-v1.json` in WSL | 10 of 11 the same; `tabs-tab-out` as before |
| Geometry, all profiles (diagnostic) | `reference_harness.py geometry --renderer <release div-components-render>` in WSL | 190 of 296 pages with every box the same (62, 56, 36, 36) |
| Renderer | `sha256sum` of the release `div-components-render` | `2dd7d79e…`, the SHA-256 in both evidence files |
| Harness unit tests | `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` | 25 passed, among them `test_profiles_above_scale_one_use_real_zoom` |

## 16 November 2026: input (C3)

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Styled input, shadcn's `input.tsx`, on Div's native text field with its native appearance off: `h-9`, `w-full`, `rounded-md`, the `border-input` border, `px-3 py-1`, `shadow-xs`, `dark:bg-input/30`, the disabled styles, `aria-invalid`'s destructive border (a prop, `invalid`), and `placeholder:text-muted-foreground` through `Element::set_pseudo_style` with `PseudoStyleTarget::Placeholder`. | components | `crates/div-components/src/input.rs` |
| Interims for the cascade (RFC 0003, request 6). `:focus-visible` (the `ring` border and the 3px ring at 50%, or the destructive ones when invalid) is set inline from `focus` and `blur` events: a text field matches `:focus-visible` whenever it has focus, so the two agree. `md:text-sm` is always applied: inline styles have no media queries, so below 768px the input keeps 14px text where Chromium gives `text-base` (16px). The ring's `color-mix(in oklab, <color> p%, transparent)` is the token's color with its alpha scaled by p, since `box-shadow` takes a typed color. | components | `crates/div-components/src/input.rs` |
| Fixtures `input-placeholder`, `-value`, `-disabled` and `-invalid` (`type="email"` in a 256px column), light and dark; trace pages for the empty and the disabled input; traces `input-keyboard`, `input-pointer` and `input-disabled`. The traces now also record `placeholder` and `aria-invalid`. shadcn's `input.tsx` is vendored unchanged from `shadcn@4.23.0`. | components | `tests/fidelity/components/reference/`, `traces.json`, `render/src/main.rs` |

### Measurement

All 74 fixtures at `legacy-800x600@1`, in the census real-font environment,
at `3bee58d5`: 31 exact, 43 different, 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json)).
The 66 earlier rows have the same images (both hashes) and status as the
switch run at `6df8ba30`. No input page is exact: each differs inside its
text (221 to 800 pixels), and the disabled pages also on the border and the
shadow, by at most 2 in a channel.

10 of 11 behavior traces are the same
([`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json)):
the three input traces (Tab and a click focus the input; a disabled input
takes no focus) and the seven that were the same before.

Outside the evidence, the 1x geometry diagnostic matches every input
element's border box with Chromium's on all 8 pages (60 of 74 pages in
all, the 52 of the switch run and these 8). Within the box, the text's
rows and the column runs of its glyphs are the same in both renderings.

### Remaining failures

- **Text in a text field.** Every glyph after the first is drawn at a
  slightly different sub-pixel position (up to 115 in a channel), while
  Geist text outside a text field (the label, most buttons) is exact. Div paints a text
  field's text in `paint_text_input_control`, which turns on
  `native_button_text_metrics` (`crates/div-paint/src/painter.rs:4035`,
  `:4076`); the difference likely comes from there. Owner: engine track,
  forms (text controls).
- **Opacity.** On the disabled pages (`opacity: 0.5`) the border and the
  `shadow-xs` differ by 1 or 2 in a channel; the same box without opacity
  is exact. Owner: engine track, paint (group opacity).
- **Media queries.** `md:text-sm` cannot be inline; the mobile profile will
  differ until the cascade reaches components. Owner: framework track
  (typed style rules, A1), RFC 0003 request 6.
- Not built: `transition-[color,box-shadow]`; the `selection:` colors (Div
  has no `::selection` pseudo style target); the `file:` styles of a file
  input's button. The focus ring is not a pixel fixture (the static pages
  are not focused); a render test checks the inline ring and border appear
  on focus and go on blur.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison at 1x | `reference_harness.py compare --renderer <release div-components-render> --profile legacy-800x600@1 --repository-commit 3bee58d5… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 31 of 74 exact, 43 different, 0 errors; `"complete": false` (1x only) |
| Earlier rows unchanged | each row of the new evidence compared with the `6df8ba30` evidence by fixture and profile | 66 of 66 the same images and status; 8 new |
| Behavior traces | `reference_harness.py trace --renderer <release div-components-render> --repository-commit 3bee58d5… --output tests/fidelity/components/evidence/traces-v1.json` in WSL | 10 of 11 the same; `tabs-tab-out` as before |
| Geometry at 1x (diagnostic) | `reference_harness.py geometry --renderer <release div-components-render> --profile legacy-800x600@1` | 60 of 74 pages with every box the same, all 8 input pages among them |
| Renderer | `sha256sum` of the release `div-components-render` | `6d95e0a0…`, the SHA-256 in both evidence files |
| Reference pages | `node build.mjs --check` in `tests/fidelity/components/reference` | 85 pages and 27 assets up to date |
| Tests | `cargo test --locked -p div-components -p div-components-render` in WSL | 21 passed, among them `a_focused_input_draws_the_focus_ring` |
| Formatting and lints | `cargo fmt --all --check`; `cargo clippy -p div-components -p div-components-render --all-targets` in WSL | clean; no warnings in the components crates |

## 16 November 2026: switch (C2 group 2, C3)

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Switch primitive, ported from Radix 1.8.1 (`react-switch` 1.4.1): a `button` with `role="switch"`, `aria-checked`, `data-state`, `value`, and `data-disabled` and `disabled` when disabled, toggled by its click (so by a pointer, Space and Enter, through Div's button activation); the thumb carries the same `data-state`. Controlled or uncontrolled, as Radix. Radix's hidden form input is left out until forms ([RFC 0003](../../alpha/rfcs/0003-component-primitives.md)). | components | `crates/div-components/src/primitives/switch.rs` |
| Styled switch, shadcn's `switch.tsx` at both sizes. The `data-state` colors and the thumb's offset are set inline by an effect (RFC 0003, request 6); `translate-x-[calc(100%-2px)]` is `transform: translateX()` of the thumb's width less 2px, because Div has no `translate` property. | components | `crates/div-components/src/switch.rs` |
| Fixtures `switch-unchecked`, `-checked`, `-disabled`, `-disabled-checked`, `-sm-unchecked` and `-sm-checked`, light and dark; trace pages for the unchecked and the disabled switch; traces `switch-keyboard`, `switch-pointer` and `switch-disabled`. The traces now also record `value`, `aria-checked`, `aria-required` and `data-size`. shadcn's `switch.tsx` is vendored unchanged from `shadcn@4.23.0`. | components | `tests/fidelity/components/reference/`, `traces.json`, `render/src/main.rs` |

### Measurement

All 66 fixtures at `legacy-800x600@1`, in the census real-font environment,
at `6df8ba30`: 31 exact, 35 different, 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json)).
The 54 earlier fixtures give 30 exact, as main `154d0a72` did after the
Linux web-font change (#51), against 4 of 54 at 1x on 12 November; the
switch adds 1 exact of 12 (`switch-disabled-checked-dark`). The other 11
switch pages differ in 8 to 39 pixels, by at most 3 in a channel.

7 of 8 behavior traces are the same
([`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json)):
all three switch traces (Tab, Space, Enter, clicks, and a disabled switch
that takes no focus and stays off), and the four tabs traces that were the
same before. `tabs-tab-out` still differs, as below.

Outside the evidence, the 1x geometry diagnostic matches every switch
element's border box (root and thumb) with Chromium's, at both sizes and in
every state; each switch page differs only in the hidden input (below).

### Remaining failures

- **Pill edges.** Every switch page that differs does so only on the
  antialiased edge of the rounded track and the `shadow-xs` below it, by 1
  to 3 in a channel; the thumb is exact. The badge's pill edge is likely
  the same difference. Owner: engine track, paint (rounded rectangles and
  shadows).
- **The server-rendered form input.** The static pages hold the hidden
  checkbox Radix renders after the switch on the server (it is removed in
  the browser outside a `<form>`). The fixture adds the same invisible
  input so the trees line up; Div lays out a checkbox as 0x0 where Chromium
  gives 13x13 (the markdown sample's MD-11). It has `opacity: 0`, so the
  pixels are unaffected. Owner: engine track, forms.
- Not built: `focus-visible` (the 3px ring and border color) and the
  `transition-all` and `transition-transform` animations; the fixtures
  compare the settled states. `focus-visible` needs the stylesheet
  cascade (RFC 0003, request 6). Owner: framework track (typed style
  rules).
- Div has no `translate` property; the thumb uses `transform`, which
  places it the same while it has no other transform. Owner: engine track,
  style.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison at 1x | `reference_harness.py compare --renderer <release div-components-render> --profile legacy-800x600@1 --repository-commit 6df8ba30… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 31 of 66 exact, 35 different, 0 errors; `"complete": false` (1x only) |
| Behavior traces | `reference_harness.py trace --renderer <release div-components-render> --repository-commit 6df8ba30… --output tests/fidelity/components/evidence/traces-v1.json` in WSL | 7 of 8 the same; `tabs-tab-out` as before |
| Geometry at 1x (diagnostic) | `reference_harness.py geometry --renderer <release div-components-render> --profile legacy-800x600@1` | 52 of 66 pages with every box the same; on the 12 switch pages only the hidden input differs |
| Renderer | `sha256sum` of the release `div-components-render` | `f429be7f…`, the SHA-256 in both evidence files |
| Reference pages | `node build.mjs --check` in `tests/fidelity/components/reference` | 75 pages and 27 assets up to date |
| Tests | `cargo test --locked -p div-components -p div-components-render` and `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` in WSL | 20 passed, among them `space_and_enter_toggle_the_focused_switch` and `a_disabled_switch_stays_off`; harness 24 passed |
| Formatting | `cargo fmt --all --check` in WSL | clean |

## 12 November 2026: tabs and the roving focus primitive (C2 group 1)

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Primitives: Radix-like behaviors without style, ported from Radix 1.8.1 (`react-tabs`, and `react-roving-focus` 1.2.1): the roving focus group (arrow keys by orientation and direction, `Home`, `End`, `PageUp`, `PageDown`, `loop`, disabled items skipped, one tabbable item), the tabs state (automatic and manual activation, controlled or uncontrolled value) and their collection, controllable state, id and key helpers. Each sets Radix's DOM contract (`role`, `aria-*`, `data-state`, `data-orientation`, `tabindex`). The interims for what `divui` lacks are listed in [RFC 0003](../../alpha/rfcs/0003-component-primitives.md) (requests 1, 2, 3, 6, 7 and 8). | components | `crates/div-components/src/primitives/` |
| Styled tabs (`Tabs`, `TabsList`, `TabsTrigger`, `TabsContent`), shadcn's default variant, horizontal and vertical. The `line` variant is left out: its indicator is an `::after` box. `w-fit` and `h-fit` on the list are `align-self: flex-start` (see the findings), the trigger's `h-[calc(100%-1px)]` is the typed `calc()` length, and `flex-1` is its longhands. | components | `crates/div-components/src/tabs.rs`, `src/css.rs` |
| Fixtures `tabs-default`, `tabs-vertical` and `tabs-password` (the second tab selected): 24 new comparisons, 216 in all. | components | `tests/fidelity/components/reference/fixtures.tsx`, `tests/fidelity/components/render/src/main.rs` |
| Behavior traces: `reference_harness.py trace` runs each trace in `traces.json` on the interactive reference page (React and the pinned Radix bundled into `pages/interactive.js`) in Chromium and on the Div fixture (the renderer's `trace` command, which dispatches through `Document::dispatch_key_input` and `dispatch_mouse_event`), and compares the focused element and the listed attributes before and after each step. | components | `tests/fidelity/components/traces.json`, `harness/reference_harness.py`, `render/src/main.rs`, `reference/build.mjs`, `reference/interactive.tsx` |

### Measurement

The complete run in the census real-font environment is 16 of 216
comparisons exact, 200 different and 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json)).
The 192 earlier comparisons have the same images (both hashes) and status
as in the open-ui `916d2735` run; the renderer's SHA-256 changed because
it now includes the tabs and main's engine changes since the import. The
first measurement, at this branch's code before its last rebase onto main
`7e7fa67`, had the same rows and traces. All 24 tabs comparisons differ: at 800x600, 2,403 to
2,788 pixels, all in the list and the panel, which is where the text is.

4 of 5 behavior traces are the same
([`traces-v1.json`](../../../tests/fidelity/components/evidence/traces-v1.json)):
the focused element and every listed attribute (`role`, `aria-selected`,
`aria-controls`, `aria-labelledby`, `tabindex`, `data-state`, `hidden` and
the others in `traces.json`) match after every key press and click of the
keyboard, pointer, vertical and manual traces. `tabs-tab-out` differs at
its last step, below.

Outside the evidence, the geometry diagnostic finds every tabs element's
border box the same as Chromium's except the panel (below); a panel hidden
with `display: none` has no box in Div and an all-zero one in Chromium,
which the diagnostic now counts as the same.

### Remaining failures

- **Leaving the document with Shift+Tab** (`tabs-tab-out`). From the first
  tabbable element, Chromium moves focus out of the page (to `body`); Div
  wraps to the last tabbable element, the selected panel. Div's sequential
  navigation wraps at the ends of the document by design. Owner: engine
  track, interaction (`crates/div-engine/src/interaction.rs`, the
  `% candidates.len()` wrap).
- **Text.** Every tabs page differs inside the text, as every other page
  that draws text does. Owner: engine track, A6 real-font text.
- **The panel is too tall.** `TabsContent` is a column flex item holding
  one line, laid out at its min-content height (100px where Chromium gives
  20px); the column flex finding of 9 November. Owner: engine track,
  layout (flex).
- **Active trigger corners (light).** The active trigger's rounded corners,
  which sit at a fractional y (19.5px), differ by up to 8 in 42 pixels at
  800x600; the dark active trigger's edges are exact. Owner: engine track,
  paint.

### Engine findings

Found while styling the tabs; each has an interim in this change and none
is fixed by it:

- `Length::fit_content()` (`fit-content(0px)`) works on a flex item's main
  axis (the badge), but on the cross axis of a column flex container it
  sizes the item to its padding: the tabs list collapsed to 6px wide.
  Interim: `align-self: flex-start`, which gives the same box here.
  Owner: engine track, layout (flex).
- A percentage `calc()` height whose base is indefinite (a column flex
  container's main size) resolves against 0 instead of behaving as `auto`:
  vertical triggers were 10px tall. Interim: vertical triggers set no
  height. Owner: engine track, layout.
- No user-agent rule `[hidden] { display: none }`: an inactive panel with
  `hidden` is still laid out. Interim: an inline `display: none` (RFC 0003
  request 8). Owner: engine track, style.

### Framework notes

The Div side of a trace reads only the attributes `traces.json` lists, since
an element's attributes cannot be enumerated through `divui`; the Chromium
side also reports any attribute it does not list, so a missing entry shows
up there. Owner: framework track.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison | `reference_harness.py compare --renderer <release div-components-render> --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL at `6fc0c1a7`, under `census_lock.py --lock /mnt/e/open-ui/out/census.lock run --wait`, renderer built with the CI `SKIA_GN_ARGS` | 16 of 216 exact, 200 different, 0 errors; complete |
| Earlier rows unchanged | each row of the new evidence compared with the open-ui `916d2735` evidence by fixture and profile | 192 of 192 the same images and status; 24 new |
| Behavior traces | `reference_harness.py trace --renderer <release div-components-render> --output tests/fidelity/components/evidence/traces-v1.json` in WSL at `6fc0c1a7` | 4 of 5 the same; `tabs-tab-out` step 2 focuses `body` in Chromium and `tabs-content#0` in Div |
| Renderer reproduces | `cargo clean --release -p div-components-render -p div-components`, rebuild, `sha256sum` | `1aab9e96…`, the SHA-256 in both evidence files |
| Reference pages | `node build.mjs --check` in `tests/fidelity/components/reference` | 61 pages and 27 assets up to date |
| Tests | `cargo test --locked -p div-components -p div-components-render` and `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` in WSL | 18 passed; harness 24 passed |
| Formatting | `cargo fmt --all --check` in WSL | clean |

## 12 November 2026: imported into Div

### Change

The track moved with the Div import (`d98e9747`): the crate is now
`crates/div-components` in the main workspace, the reference renderer
`tests/fidelity/components/render` (`div-components-render`), and the
reference pages, harness and evidence `tests/fidelity/components`. The code
uses the public `divui` crate. No behavior changed.

### Measurement

All 192 images rendered at `d98e9747` are byte-identical to the renderer's
images in the last open-ui comparison (`reference-v1.json`, open-ui `916d2735`),
so its results held for Div and the evidence was not measured again.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Images unchanged | `cargo build --locked --release -p div-components-render` in WSL with the CI `SKIA_GN_ARGS`; each fixture rendered at the four profiles in the census environment and its RGBA bytes compared with the `openui.png` images of the open-ui `916d2735` run | 192 of 192 identical |
| Crate tests | `cargo test --locked --no-fail-fast -p div-components -p div-components-render`, WSL | 10 passed |
| Harness unit tests | `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` | 18 passed |
| Formatting | `cargo fmt --all --check`, WSL | clean |

## 11 November 2026: theme tokens as `oklch()`

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Each theme token is now shadcn's own value, an `oklch()` color (`Token`), which Div resolves since [open-ui#95](https://github.com/zhuowcui/open-ui/pull/95), in place of the sRGB stand-in floats. Tailwind's opacity modifier is the real `color-mix(in oklab, <token> N%, transparent)`. Custom properties are still missing, so the token's value is written where shadcn writes `var(--token)`. | components | `crates/div-components/src/theme.rs`, `src/css.rs` |
| The theme test checks each token's value against the value Chromium's page specifies, and the color Div resolves it to against Chromium's serialized sRGB color (six significant digits). | components | `crates/div-components/tests/theme_evidence.rs` |

### Measurement

All 192 Div images are byte-identical to the ones in
[`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json),
measured at `916d2735`, so the comparison results are unchanged and the
evidence was not measured again.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Images unchanged | the release renderer, built in WSL with the CI `SKIA_GN_ARGS`, renders every fixture at the four profiles in the census environment, and each image's RGBA bytes are compared with the `openui.png` of the 916d2735 run; repeated on this branch rebased on main `c48c2e38` ([open-ui#111](https://github.com/zhuowcui/open-ui/pull/111)) | 192 of 192 identical |
| Workspace tests | `cargo test --locked` in `components/`, on Windows and, rebased on `c48c2e38`, in WSL | 10 passed |
| Formatting | `cargo fmt --all --check` in `components/`, in WSL | clean |

## 11 November 2026: measured on main after A2 colors and WOFF2

### Change

No components code changed. Main gained the engine's `oklch()`,
`oklab()` and `color-mix()` support, which also stopped clamping color
channels when painting ([open-ui#95](https://github.com/zhuowcui/open-ui/pull/95)), and WOFF and WOFF2
decoding before the font scaler ([open-ui#97](https://github.com/zhuowcui/open-ui/pull/97)). The evidence was measured again at main `916d2735`.

### Measurement

The complete run in the census real-font environment is 16 of 192
comparisons exact, 176 different and 0 errors
([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json)):

- **Every separator is exact (16 of 16).** The 8 dark separators differed
  only where the painter clamped the slightly out-of-gamut white tokens;
  the stand-in floats are now painted unclamped, as Chromium does.
- **The fonts load.** The 176 errors (`MalformedFont`) are gone; every
  fixture that draws text now renders and is compared.
- Run with the system FreeType, as the 10 November preview at `183f3d1b`
  was, no image has more differing pixels than in that preview. 37 have fewer: the 8 dark separators
  and dark buttons, badges and cards. 152 are byte-identical, and 3 changed
  bytes with the same count. The census FreeType and the system one give
  the same image for 188 of 192 pages.

### Remaining failures

- **Text.** Every button, badge, card, font and label page still differs
  inside the text (the C4 vertical placement finding). Owner: engine
  track, A6 real-font text.
- **Column flex and grid layout.** The card and font fixtures' layouts
  differ (152 of 192 fixture layouts the same, as before): the column flex
  min-content height, the grid `auto` column width and the grid
  auto-placement cursor, recorded on 9 November. The engine track has
  fixes for all three on its branches (`96a9fb47`, `00bfc5b8`, `9f8e5dba`);
  a preview with them, outside the evidence, matched all 192 layouts.
  Owner: engine track, layout.
- **Edges at 800x600.** The dark outline button's translucent border is now
  exact (228 differing edge pixels before, 0 now). The dark outline badge
  went from 183 to 106 and still differs. Filled badge ends (15 to 78
  pixels; the dark destructive badge went from 22 to 15) and the disabled
  button's opacity corners (17 and 35) still differ. Owner: engine track, paint.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Comparison | `reference_harness.py compare --renderer <release div-components-render> --repository-commit 916d2735… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, under `census_lock.py run --wait`, renderer built with the CI `SKIA_GN_ARGS` | 16 of 192 exact, 176 different, 0 errors; complete |
| Renderer reproduces | `cargo clean --release -p div-components-render -p div-components`, rebuild at `916d2735`, `sha256sum` | `d8c31acc…`, the SHA-256 in the evidence |
| Geometry (preview) | `reference_harness.py geometry --renderer <wrapper running the release renderer with the system FreeType>` in WSL | 152 of 192 fixture layouts the same; differences only in the card and font fixtures |
| Edges (preview) | each button and badge diff at 800x600 split into pixels within 3px of the component's edge and inside it | as listed above |

## 9 November 2026: button, badge, card and label

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Button (variants `default`, `destructive`, `outline`, `secondary`, `ghost`, `link`; sizes `default`, `xs`, `sm`, `lg`; disabled), badge (the same six variants), card (`Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardAction`, `CardContent`, `CardFooter`) and label, over the public `openui` crate. The icon sizes are left out until Div can draw Lucide icons; hover, focus and `aria-invalid` styles wait for the interactive work. | components | `crates/div-components/src/{button,badge,card,label}.rs` |
| Styles are the computed values of Tailwind's compiled classes, set declaration by declaration after preflight's reset. Where Div's literals cannot express a value the typed value is set (`box-shadow`, `width: fit-content`, `container-type`, `container-name`, grid placement), `border` is set as longhands, and `line-height` lengths stand for Tailwind's `calc()` ratios. `Theme::dark` stands in for the `.dark` class, and `css::with_opacity` for `color-mix(in oklab, X N%, transparent)`. | components | `crates/div-components/src/css.rs`, `src/theme.rs` |
| 19 fixtures (38 pages): each button variant at the default size, the three text sizes, disabled, each badge variant, the docs example card without and with an action, and a label. Buttons and badges sit in a `flex` row. | components | `tests/fidelity/components/reference/fixtures.tsx`, `tests/fidelity/components/render/src/main.rs` |
| Geometry diagnostic: the renderer's `geometry` prints each fixture element's border box, and `reference_harness.py geometry` compares it with Chromium's `getBoundingClientRect` (same within 1/128 px), to tell layout from paint differences. It writes no evidence. | components | `tests/fidelity/components/render/src/main.rs`, `tests/fidelity/components/harness/reference_harness.py` |

### Measurement

In the census real-font environment, every new fixture is an error: each
draws text, and the bundled WOFF2 fonts do not load there (the C4
failure below). The complete run is 8 of 192 comparisons exact (the light
separators), 8 different (the dark separators, unchanged) and 176 errors
(`Engine(Font(MalformedFont))`) ([`reference-v1.json`](../../../tests/fidelity/components/evidence/reference-v1.json)).
It was measured again at `183f3d1b`, after the rebase onto main
(`806356cb`); every row is the same as at `df1c5b5d`, the commit before the
rebase, which has the same `components/` tree. The previews below ran at
`df1c5b5d`.

### Preview findings

These are **not part of the evidence**: Div was run with the system
FreeType (which reads WOFF2) instead of the census one, so the text loads.
They locate the failures that the font error hides, for the engine track.

- **Layout matches for every button, badge, label and separator fixture.**
  The geometry diagnostic finds each element's border box within 1/128 px
  of Chromium's at all four profiles (152 of 192 fixture layouts; the other
  40 are the font and card fixtures below).
- **Plain rounded rectangles paint exactly; text does not.** For each
  `default`, `secondary`, `ghost` and `link` button and every size, all
  differing pixels at 800x600 are inside the button, away from its edges:
  the glyphs (the C4 vertical placement finding).
- **Three edge differences** remain at 800x600 (pixels within 3px of the
  component's edge):
  - Filled badges (a background, a transparent 1px border,
    `overflow: hidden`, a radius of half the height): 29 to 78 pixels on
    the rounded ends. An exact 11px radius gives the same pixels as the
    `9999px` stand-in for `calc(infinity * 1px)`, so the stand-in is not
    the cause; the outline badge, with a border and no background, has
    exact edges. Owner: engine track, paint.
  - Translucent dark borders: the dark outline button (`--input`, 15%) has
    228 differing edge pixels and the dark outline badge (`--border`, 10%)
    183. Both are drawn in the translucent, slightly out-of-gamut white
    tokens whose clamp the dark separator measures, so they are expected to
    change with the A2 color fix; not yet confirmed. Owner: engine track,
    A2 colors.
  - The disabled button (`opacity: 0.5`): 17 and 35 pixels on its rounded
    corners, which are exact on the same button without the opacity.
    Owner: engine track, paint (opacity layers).
- **Column flex items are as tall as their min-content layout.** A
  `display: flex; flex-direction: column` box 382px wide holding one `div`
  with the words "Card Content" lays the `div` out 48px tall (two lines);
  Chromium gives 24px. Every card section is a column flex item, so the
  card is 274px tall (290px with an action) where Chromium's is 190px.
  This is the column flex finding from C4. Owner: engine track, layout
  (flex).
- **A grid `auto` column beside a `1fr` column is sized at min-content.**
  Outside any flex container, `CardHeader` with an action
  (`grid-template-columns: 1fr auto`, the action at `grid-column: 2;
  grid-row: 1 / span 2`) makes the `auto` column 46.73px wide, so "Card
  Action" wraps onto two lines, and the grid 76px tall; Chromium's column
  is the max-content width, 85.61px, and the grid 44px. Owner: engine
  track, layout (grid).

### Framework notes

Div's style literals (`view!` `style:` and `typed_style::parse_literal`)
cannot express several values the components need. Each is set another way
and none changes the computed value:

- No `box-shadow` literal: the shadow is built as `Vec<BoxShadow>`.
- `border` needs a width, a style and a color, and rejects
  `currentcolor`; `border-*-width` takes a bare integer, not `1px`.
- No intrinsic lengths (`fit-content`), `container-type` and
  `container-name`, grid placement (`grid-column: 2`) or CSS-wide
  keywords (`inherit`) as literals; no `user-select` property.
- A button must opt out of native appearance
  (`Element::set_native_appearance(false)`); Chromium drops it once a page
  styles the button's background or border.

Owner: engine track (style parsing, RFC 0001) and framework track.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Reference pages and assets are current | `node build.mjs --check` in `tests/fidelity/components/reference`, after `npm ci` | current, 51 pages and 27 assets |
| Harness unit tests | `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` | 18 passed |
| Comparison | `reference_harness.py compare --renderer <release div-components-render> --repository-commit 183f3d1b… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 8 of 192 exact, 8 different, 176 errors; complete |
| Renderer reproduces | `cargo clean --release -p div-components-render -p div-components`, rebuild at `183f3d1b`, `sha256sum` | `d82152c3…`, the SHA-256 in the evidence |
| Geometry (preview) | `reference_harness.py geometry --renderer <wrapper running the release renderer with the system FreeType>` in WSL | 152 of 192 fixture layouts the same; differences only in the card and font fixtures |
| Workspace tests (Windows) | `cargo test --locked` in `components/`, at `183f3d1b` | 10 passed |
| Formatting | `cargo fmt --all --check` in `components/`, in WSL (on Windows the file list exceeds the command-line limit, os error 206) | clean |

## 9 November 2026: bundled fonts and icons (C4)

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Bundled assets: Geist and Geist Mono variable WOFF2 (`geist` 1.9.2), Inter variable WOFF2 (`inter-ui` 4.1.1) and the 20 Lucide icons the new-york-v4 components import (`lucide-static` 1.70.0), copied unmodified from the locked npm packages with their OFL and ISC license files. `build.mjs` writes them with `assets/manifest.json` (SHA-256 of each file) and `--check` fails if any differs. | components | `tests/fidelity/components/assets/`, `tests/fidelity/components/reference/build.mjs` |
| The reference pages load the fonts with `@font-face` (`font-display: block`, no fallback family) and use Geist as `--font-sans` and Geist Mono as `--font-mono`, as the shadcn/ui site does, plus `--font-inter`. | components | `tests/fidelity/components/reference/build.mjs` (`FONT_FACES`, `fontCss`) |
| `div_components::fonts::register_fonts` registers the same files with the same descriptors through `Document::register_font_face`; a test checks the SHA-256 Div reports for each registered face against the manifest. | components | `crates/div-components/src/fonts.rs`, `tests/assets_manifest.rs` |
| Font specimen fixtures: three `text-sm` lines at weights 400, 500 and 600 in each face. Fixtures without text skip font registration, so a font-loading failure cannot hide their results. | components | `tests/fidelity/components/reference/fixtures.tsx`, `tests/fidelity/components/render/src/main.rs` |
| `reference_harness.py fonts` records the face Chromium draws each specimen line with (`CSS.getPlatformFontsForNode`) and fails on any fallback glyph. | components | `tests/fidelity/components/harness/reference_harness.py` |

### Measurement

- Chromium draws every specimen line entirely with the bundled face: Geist
  (`Geist-Regular`), Geist Mono (`GeistMono-Regular`) and Inter
  (`InterVariable`), with no fallback glyphs, and each family's `FontFace`
  is loaded
  ([`fonts-v1.json`](../../../tests/fidelity/components/evidence/fonts-v1.json)).
- Div cannot load the fonts in the census real-font environment: all 24
  font comparisons are errors (`Engine(Font(MalformedFont))` from
  `register_font_face`). The separator results are unchanged: 8 exact, and 8
  dark comparisons different by one level on the line pixels.

### Remaining failures

- **WOFF2 fonts do not load on Linux with the census FreeType, 24
  comparisons.** On Linux, `platform_font_data` passes WOFF and WOFF2 bytes
  to FreeType undecoded; FreeType reads WOFF2 only when built with Brotli,
  and the pinned real-font FreeType
  (`tools/accountability/data/fonts/sp16_linux_x86_64/libfreetype.so.6`) is
  not, so registration fails with `MalformedFont`. With the system FreeType
  (which links Brotli) the same render succeeds, and on Windows the bytes are
  decoded with `wuff` first, so the Windows tests pass. Chromium decodes web
  fonts with its OpenType Sanitizer before FreeType on every platform.
  Owner: engine track, text (`crates/div-text/src/font/collection.rs`,
  the `#[cfg(not(windows))] platform_font_data`). Not worked around: the
  bundled files stay WOFF2.

Two further text failures were seen in a local preview that is **not part
of the evidence**: Div was given the Geist TTF files from the same
`geist` package (which loads with the census FreeType), and only the Geist
fixtures were compared. They are recorded for the engine track and will be
measured once WOFF2 loads.

- **A column flex item with text is as tall as its min-content layout.** In
  the specimen's `display: flex; flex-direction: column; gap: 4px`
  container, each `<p>` is laid out 768px wide but is as tall as one line
  per word (7 words: 140px, where Chromium gives 20px); the text still paints
  on one line at the top of the box. Owner: engine track, layout (flex).
- **Glyphs sit at a different vertical position.** Rebuilt with block layout
  and the same spacing, the first line's ink spans the same columns as
  Chromium's (x 17 to 256 at 1x) but each line covers different rows (at
  1x, Chromium 21–32, 45–56 and
  68–80; Div 19–31, 43–55 and 67–79). No whole-pixel shift aligns them,
  so the difference is in baseline or vertical glyph placement, not an
  offset. Owner: engine track, text.

### Framework notes

- `style:line-height="calc(1.25 / 0.875)"` (Tailwind's `text-sm` line
  height) is rejected by `view!`; the fixture uses the number `1.4285714`.
  Owner: engine track (style parsing, RFC 0001).

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Reference pages and assets are current | `node build.mjs --check` in `tests/fidelity/components/reference`, after `npm ci` | current, 13 pages and 27 assets |
| Harness unit tests | `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` | 14 passed |
| Fonts Chromium uses | `reference_harness.py fonts --output tests/fidelity/components/evidence/fonts-v1.json` in WSL | 6 pages, 0 problems |
| Comparison | `reference_harness.py compare --renderer <release div-components-render> --repository-commit c98950ef… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 8 of 40 exact, 8 different, 24 errors; complete |
| Workspace tests (Windows) | `cargo test --locked` in `components/` | 8 passed |
| Formatting | `cargo fmt --all --check` in `components/` | clean |

`c98950ef` is `2cc95b7a` before its rebases onto main; the two have the same
`components/` tree.

## 9 November 2026: reference harness and separator

### Change

| Change | Owner | Source |
| --- | --- | --- |
| Pinned shadcn/ui reference: shadcn `4.23.0` (`96da9cc`, `new-york-v4` on Radix, neutral theme), React 19.3.1, Radix 1.8.1, Tailwind 4.3.5 and tw-animate-css 1.4.0 from a locked npm tree. `build.mjs` renders fixtures to static light and dark pages and compiles shadcn's `globals.css`; `--check` fails if the committed pages are stale. | components | `tests/fidelity/components/reference/` |
| Harness: captures each page with Chrome-for-Testing 147.0.7727.50 in the census real-font environment, renders the same fixture with Div (`chromium-linux-lcd`), compares RGBA bytes and writes evidence. `tokens` records Chromium's sRGB conversion of every theme token. | components | `tests/fidelity/components/harness/reference_harness.py` |
| Theme stand-ins: the 31 neutral tokens in both themes, as the unclamped float sRGB values Chromium computes, checked against the evidence by a test. To be replaced by typed theme tokens (A2). | components | `crates/div-components/src/theme.rs`, `tests/fidelity/components/evidence/theme-tokens-v1.json` |
| Separator, the first component, and `div-components-render`, which renders each fixture with the public `openui` crate. | components | `crates/div-components/src/separator.rs`, `tests/fidelity/components/render/` |
| CI: a `components` workflow checks the reference pages, runs the harness unit tests, and formats and tests the components workspace on Linux. | components | `.github/workflows/components.yml` |

Licenses are listed in [`crates/div-components/THIRD_PARTY.md`](../../../crates/div-components/THIRD_PARTY.md):
shadcn/ui, Tailwind, Radix, React and tw-animate-css are MIT,
class-variance-authority is Apache-2.0, Lucide is ISC, and Geist and Inter
are OFL 1.1.

### Measurement

| Theme | `legacy-800x600@1` | `mobile-375x667@2` | `desktop-1280x720@1.25` | `desktop-1920x1080@1.5` |
| --- | --- | --- | --- | --- |
| light (2 fixtures) | 2 exact | 2 exact | 2 exact | 2 exact |
| dark (2 fixtures) | 2 different | 2 different | 2 different | 2 different |

Every dark difference is confined to the separator's own pixels (for
example 256 pixels, the 256 × 1 line, at 1x), and each differs by one level:
Chromium paints the line `rgb(34 35 35)` and Div `rgb(34 34 34)`.

### Remaining failures

- **Extended-range token colors are clamped before painting, 8
  comparisons.** The dark `--border` is `oklch(1 0 0 / 10%)`, which Chromium
  converts to `color(srgb 0.999871 1.00005 1.00007 / 0.1)` and paints from
  those floats: green is 0.1 × 1.00005 + 0.9 × 10/255 = 0.13530, 34.501 of
  255, which rounds to 35. Div clamps each channel to [0, 1] before
  painting, giving 34.4999, which rounds to 34. Owner: engine track, colors
  (A2), `div-paint/src/painter.rs` (`set_paint_css_color_with_alpha`
  clamps `r`, `g` and `b`). The same clamp will affect any token outside
  sRGB, including `destructive` and the chart colors. The stand-in keeps
  Chromium's floats, so no component code works around it.

### Framework notes

- `view!` does not fill an omitted `#[prop(optional)]` prop: the props
  struct needs every field, and an optional prop must be passed as
  `Some(..)`. `Separator` takes `orientation` as a required prop for now.
- A `{expr}` child in `view!` is always text; a child view can only be
  embedded through a component. The fixture renderer uses a one-line
  `FixtureView` component for this.

These are API requests for the framework track, not blockers.

### Verification

| Check | Command | Result |
| --- | --- | --- |
| Reference pages are current | `node build.mjs --check` in `tests/fidelity/components/reference`, after `npm ci` | current, 7 files |
| Harness unit tests | `python3 -m unittest tests/fidelity/components/harness/test_reference_harness.py` | 10 passed |
| Theme tokens | `reference_harness.py tokens --output tests/fidelity/components/evidence/theme-tokens-v1.json` in WSL | 62 tokens (31 per theme) |
| Comparison | `reference_harness.py compare --renderer <release div-components-render> --repository-commit 7fc2c227… --evidence tests/fidelity/components/evidence/reference-v1.json` in WSL, renderer built with the CI `SKIA_GN_ARGS` | 8 of 16 exact, 8 different, 0 errors; complete |
| Workspace tests (Linux) | `cargo test --release --locked` in `components/`, WSL | 6 passed |
| Formatting | `cargo fmt --all --check` in `components/` | clean |

## Next

1. Theme tokens as custom properties, and `.dark` as a class, once typed
   style rules and tokens (A1, A2) land.
2. Build the Radix-like primitives (C2) as proposed in
   [RFC 0003](../../alpha/rfcs/0003-component-primitives.md), as a
   `primitives` module of `div-components`, on the public `divui` API and
   the RFC's interims. Roving focus, tabs, switch, input and checkbox
   are built. Dialog waits for the top layer, which this track builds as
   H4 ([RFC 0015](../../alpha/rfcs/0015-top-layer-and-dialog.md)): the
   Chromium oracle, then the WPT porter's dialog mutations, the engine
   top layer, `::backdrop` and the UA rules, and the `divui` API. Their
   `data-state` styles are inline until typed style rules (A1) land.
3. Lucide icons in Div: every Lucide icon is an inline `svg` of stroked
   `<path>` data in `currentColor`, and Div's DOM has no SVG shapes yet.
   The checkbox, the first component with an icon, builds the outer `svg`
   only. Owner: the SVG lane and the framework track
   ([RFC 0009](../../alpha/rfcs/0009-inline-svg.md), V1 stage 1, whose gate
   covers the 20 icons the components import); draw the icons when it
   lands.
