# RFC 0015: The top layer and `<dialog>`

**Status:** Proposed, 21 November 2026, for the user's approval. It adds
public API and changes what `set_modal_root` does.

**Owner:** the components track, taking platform-plan item H4
([`platform-plan.md`](../platform-plan.md), section 1, "HTML elements").
The engine lane reviews the engine changes, and the framework lane
reviews the `divui` API.

**Consumers:**
- the components track (shadcn's `Dialog`; later `AlertDialog`, `Sheet`
  and `Drawer`);
- the interactions and Git samples (Radix `Dialog`, cmdk);
- later, H5 (`popover`), H6 (anchor positioning) and H7 (`inert`), which
  build on the same top layer and inertness.

## Problem

Radix, cmdk and shadcn open modals with `showModal()` or emulate it. Div
has none of what a modal needs:

- **No element or top layer.** There is no `dialog` element:
  `tag_definition` in `crates/divui/src/element.rs` rejects the tag. Div
  has no top layer and no `::backdrop`. `:modal` parses but fails to match
  as `unsupported` (`crates/div-engine/src/selector_match.rs`).
- **A focus-only modal root.** The only modal mechanism is
  `Document::set_modal_root` (`crates/div-engine/src/interaction.rs`). It
  confines focus, Tab and the accessibility `modal` flag to one subtree.
  It does not block pointer input outside the subtree and does not change
  stacking. Escape does not close it. It restores focus unconditionally.
  So a ported modal needs a hand-made portal, a full-screen overlay and
  its own key handling, each of which differs from Chromium.
- **WPT.** `html/semantics/interactive-elements/the-dialog-element` at the
  WPT pin `09d377d9` has 180 files, among them 33 reftests. Div cannot run
  any of them.

## Proposal

The behavior is Chromium 147.0.7727.50's (BSD
`third_party/blink/renderer/core/html/html_dialog_element.cc`), and the
HTML and CSS Position 4 specifications where Blink follows them. Blink
features that 147 ships only as experimental are left out:
- `DialogNewFocusBehavior`: 147 runs the legacy dialog focusing steps;
- `DialogCloseWhenOpenRemoved`: removing `open` does not close a dialog.

A Chromium oracle measures what the source leaves open (see Migration).

**1. The element.** `dialog` becomes an element (`ElementTag::Dialog`) with
the HTML rendering section's UA rules:
- `dialog { position: absolute; inset-inline: 0; width: fit-content;
  height: fit-content; margin: auto; border: solid; padding: 1em;
  background: Canvas; color: CanvasText }`;
- `dialog:not([open]) { display: none }`;
- `dialog:modal { position: fixed; overflow: auto; inset-block: 0;
  max-width: calc(100% - 6px - 2em); max-height: calc(100% - 6px - 2em) }`;
- `::backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.1) }`.

These are taken from the specification, not from Blink's `html.css`,
which is LGPL. They go into the fidelity lane's UA stylesheet (RFC 0004).
If that sheet has not landed yet, they go in a small engine sheet of
origin `CascadeOrigin::UserAgent`, which the fidelity lane absorbs later.

**2. The `divui` API**, on `Element`. Each method returns
`Err(Error::InvalidArgument("not a dialog"))` on other elements.

```rust
impl Element {
    pub fn show_modal(&self) -> Result<(), Error>;
    pub fn show(&self) -> Result<(), Error>;
    pub fn close(&self, return_value: Option<&str>) -> Result<(), Error>;
    pub fn request_close(&self, return_value: Option<&str>) -> Result<(), Error>;
    pub fn return_value(&self) -> Result<String, Error>;
    pub fn set_return_value(&self, value: &str) -> Result<(), Error>;
}

// The shadcn pattern: open from a trigger, close from inside.
trigger.on("click", move |_| dialog.show_modal().unwrap())?;
close.on("click", move |_| dialog.close(None).unwrap())?;
```

**Errors.** Blink's `InvalidStateError` cases return a new
`Error::InvalidState(&'static str)` with Blink's message:
- `show()` on a modal dialog;
- `show_modal()` on a non-modal dialog that is open;
- `show_modal()` on a dialog that is not in the document.

A call that finds the dialog already open in the same mode does nothing.

**Events**, with Blink's targets and timing:
- `beforetoggle`: cancelable when opening; cancelling it stops the open.
- `toggle`: a task, coalesced as Blink coalesces it.
- `cancel`: cancelable as the close-watcher rules allow. A second Escape
  without user activation in between cannot be cancelled.
- `close`: queued and fired with the next animation frame's events.

Div has no toggle events yet. The two toggle events carry `old_state` and
`new_state` (`"open"` or `"closed"`) on the event, as `ToggleEvent` does.

**Focus.**
- On open, the legacy focusing steps run. They focus the first autofocus
  focusable descendant, else the first focusable descendant, else the dialog
  itself; nested dialogs are skipped. If nothing can be focused, focus is
  cleared.
- `close` restores focus to the element focused before the dialog opened,
  without scrolling. For a non-modal dialog it does so only if focus is
  inside the dialog.

**3. The top layer** (engine).
- **The list.** The document keeps an ordered top layer. `show_modal`
  appends the dialog, and `close` or removing the dialog from the document
  takes it out. `show` puts nothing in the top layer.
- **Layout.** A top-layer element and its `::backdrop` generate boxes whose
  containing block is the initial containing block. A computed `position`
  of `static` or `relative` is used as `absolute` (CSS Position 4, "top
  layer").
- **Paint and hit testing.** Top-layer elements paint after the document in
  top-layer order, each above its own `::backdrop`. Their ancestors' clip,
  opacity, transform and overflow do not apply. Hit testing runs in the
  reverse order, and a hit on a `::backdrop` targets its dialog (Blink's
  `FindNearestDialog`).
- **Matching.** `:modal` and `:open` match.

**4. Inertness.**
- While a modal dialog is open, everything outside the topmost modal
  dialog is inert:
  - it cannot be focused and is not in the Tab order;
  - it is not in the accessibility tree;
  - it receives no pointer events.
- Where a press on inert content is delivered is measured by the oracle.
- `set_modal_root` changes, and is deprecated: it now makes everything
  outside its root inert in the same way, but without the top layer or a
  backdrop. Its unconditional focus restore stays, so its callers keep
  working until they move to `show_modal`.

**5. Escape.** An Escape `keydown` that no listener cancels is a close
request for the topmost open modal dialog. It dispatches `cancel` and,
unless that is cancelled, calls `close` with no return value.
- A non-modal dialog ignores Escape, as Blink's default `closedby` (Auto,
  which means `none` when non-modal) does.
- `closedby` and light dismiss are left to H5.

**Out of scope:**
- moved to H5: `popover`, `closedby` and light dismiss;
- moved to H7: the `inert` attribute;
- not covered: `method="dialog"` form submission, iframes and `<object>`,
  close-watcher sources other than Escape, transitions of `overlay`, and
  the C ABI.

## Affected tracks

- **Engine.** Owns the code in `div-dom` (the tag), `div-engine` (the top
  layer, inertness, `:modal`, `:open` and `::backdrop`), `div-layout` (the
  top-layer containing block) and `div-paint` (the top-layer pass).
  - The changes stay within top-layer and dialog code.
  - The top-layer paint pass is based on the SVG lane's partial redraw
    (A1), which goes first in `painter.rs`.
  - The pseudo-class work follows #107 (selector state invalidation).
- **Framework.** Owns `divui`: the tag line, the API above,
  `Error::InvalidState`, the Escape default action, and the event types.
  - Adding the error variant breaks exhaustive matches on `divui::Error`.
  - `set_modal_root`'s doc comment records its deprecation.
  - The `tools/wpt/javascript_mutation_ir.mjs` extension (`showModal`,
    `show`, `close`, `remove`) lands before the framework's T3 rework
    (RFC 0005) and is reviewed by both lanes.
- **Fidelity.** Its UA sheet receives the four rules in Proposal 1.
- **Components and samples.** Move from `set_modal_root` and hand-made
  overlays to `show_modal`. Focus-preserving chrome cancels `mousedown`
  (RFC 0007 §8).

## Migration

One PR at a time. Each keeps `main` green and records its results in
`docs/progress/tracks/components.md`.

| # | PR | Gate |
| --- | --- | --- |
| 1 | **Chromium dialog oracle** (no product code). Captured twice from real input at 1x, with a replay test that lists every scenario as pending. | Byte-stable capture; the replay test passes with its pending list |
| 2 | **WPT porter mutations** (`showModal`, `show`, `close`, `remove`). | The porter's own tests |
| 3 | **Engine top layer:** the tag, the top-layer list, inertness (the `set_modal_root` change), `:modal`, `:open`, the containing block, the paint pass and hit testing. | Ported reftests: `top-layer-*` (stacking, nesting, position, containing block, `display: none`, parent clip, opacity, transform and overflow), `removed-element-is-removed-from-top-layer`, `element-removed-from-top-layer-has-original-position`, `modal-dialog-sibling`; the oracle's geometry and click scenarios |
| 4 | **`::backdrop` and the UA rules.** | `backdrop-*`, `modal-dialog-backdrop(-opacity)`, `dialogs-with-no-backdrop`, `green-dialog-and-backdrop`, `top-layer-backdrop-remove-add-ordering` |
| 5 | **The `divui` API, events, Escape and focus.** | The oracle's pending list is empty. Rust translations name each WPT file and assert they reproduce: `dialog-showModal`, `-close`, `-close-event`, `-cancel-events`, `-cancel-preventDefault`, `-canceling`, `-return-value`, `-focusing-steps-*`, `focus-after-close`, `modal-dialog-ancestor-is-inert`, `remove-dialog-should-unblock-document`, `closed-`/`non-modal-dialog-does-not-block-mouse-events`, `inert-node-is-unfocusable` |

When T3 (RFC 0005) lands, `the-dialog-element` is measured again through it
and the gap list is updated. The components track then builds `Dialog`.
