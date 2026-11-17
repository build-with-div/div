# Framework track progress

Workstream B of the [alpha plan](../../alpha/plan.md): the public `openui`
crate, `div-platform` and the CLI and tooling crates. Names are working
names until D5 picks the product name.

## Plan for alpha.1

The order and verification of each PR are in
[RFC 0002](../../alpha/rfcs/0002-cli-and-app-layout.md#6-delivery).

| # | PR | State |
| --- | --- | --- |
| 1 | RFC 0002: CLI, app layout and async model | Merged (#90) |
| 2 | `div-cli` skeleton: `new`, `dev`, `build`, `doctor` | Merged (#93) |
| 3 | Multi-window runtime | Merged (#96) |
| 4 | Async runtime integrated with the UI thread and signals | Merged (#98) |
| 5 | Clipboard service | Merged (#99) |
| 6 | File dialogs | Merged (#100) |
| 7 | App menu | Merged (#101, #103) |
| 8 | HTTP client | Merged (#105) |
| 9 | `default` template and test utilities | Merged (#106) |

## RFC 0003 requests

The framework's part of
[RFC 0003](../../alpha/rfcs/0003-component-primitives.md#affected-tracks),
one PR each, smallest first.

| RFC request | Change | State |
| --- | --- | --- |
| 7 | `Element::contains` | Merged (#25) |
| 2 | Key names on keyboard events, and `repeat` | Merged (#27) |
| 3 | `view!` fills omitted optional props | Not started |
| 1 | `provide_context` / `use_context` | Not started |

## Dogfood gaps (sample-git)

Gaps the first dogfood app, `build-with-div/sample-git`, found in the
framework (its capability probe, run against `03573e7d`, and its
`docs/framework-gaps.md`). One PR
each, small fixes first, in the order 1, 2, 6, 5, RFC 0003 requests 3 and
1, then 3, 4 and 7.

| # | Gap | State |
| --- | --- | --- |
| 1 | `on:` on a component tag was silently dropped | Merged (#29) |
| 2 | `{expr}` children that are views (`For`, `Show`) | Merged (#30) |
| 3 | App services: `contextmenu` and popup menus, window activation, app data directory, reveal in file manager | Not started |
| 4 | Headless pointer events and pointer capture for drags | Not started |
| 5 | `current_document()` in effect re-runs (sample-git 25) | Not started |
| 6 | zbus fails to build with a fresh lockfile (sample-git 24) | Merged (#31) |
| 7 | Public SVG drawing, for icons | Folded into RFC 0009 (inline SVG); the framework part is F1 below |

## Platform plan, Tier 0

The framework's foundations from the platform plan (six sample
investigations, consolidated on 13 November 2026), with the API of
[RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md).
They come before the remaining dogfood gaps and RFC 0003 requests, which
jump the queue only when they block a sample, in the order 0.10, 0.11,
0.12 A, 0.9, 0.12 B, 0.12 C, then keyboard scrolling. (0.12 was split in
three and 0.9 moved after 0.12 A on 16 November 2026, agreed through the
parent session: 0.12 A brings passive listeners, which the wheel work
uses.)

| # | Item | State |
| --- | --- | --- |
| 0.9 | Wheel direction, lines per notch and `deltaMode` (I3) | In review |
| 0.10 | `requestAnimationFrame` and headless frame advance (R1, T1) | Merged (#47) |
| 0.11 | `ResizeObserver`, with the engine's part (O1) | Merged (#52) |
| 0.12 A | Listener handles and options (sample-editor gap 29) | Merged (#61) |
| 0.12 B | Pointer Events (I1), with sample-canvas gap 16 (focus on `mousedown`) | Not started |
| 0.12 C | Test input (T1), with sample-git gap 29 and dogfood gap 4 | Not started |
| I2 | Keyboard scrolling (sample-markdown MD-23) | Not started |

## After Tier 0

Queued after keyboard scrolling, in this order, unless one blocks a sample
outright. F1 moves up to follow 0.10 once the engine's E1 (namespaces in
element identity) merges, because the samples' icons are written in `view!`.

| # | Item | State |
| --- | --- | --- |
| editor 26 | `innerWidth` and `innerHeight` on `Document` | Not started |
| editor 27 | `offsetWidth`, `offsetHeight` and `offsetParent` | Not started |
| editor 23 | `attributes` and `attribute_names` in insertion order | Not started |
| editor 25 | `createElement` for any tag, `nodeName` and `tagName` (waits on the engine's `HTMLUnknownElement`; designed with F1) | Not started |
| editor 24 | Node type, `childNodes`, `insertBefore` over all node types, identity | Not started |
| editor 22 | Inline `style` and `cssText` (waits on the engine's R0b; with decision 2's runtime custom-property writes) | Not started |
| charts TZ1 | System time zone in `div-platform` | Not started |
| F1 | RFC 0009: `Element::create_ns`, and `view!` with SVG's namespace and spellings (waits on the engine's E1) | Not started |

## Log

### 16 November 2026: native wheel direction, pixels per notch and `deltaMode` (Tier 0 item 0.9)

Problem (sample-interactions F2, I3): native wheels scrolled the wrong way,
since `div-platform` passed winit's deltas (positive when the content moves
down) as the DOM's (positive when the view does). A notch was 40px on every
platform, Shift+wheel did not scroll horizontally, and events had no
`deltaMode`.

Oracle: `tools/qualification/capture_wheel_oracle.py` gives pinned Chrome
147.0.7727.50 real platform wheel input, not CDP's, which takes deltas in
CSS pixels and skips the conversion under test.
- Windows: `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` are posted to the headed
  browser window, with the key state in `wParam`. The window is made topmost,
  because Chromium drops a wheel whose point is not over its own window.
- Linux: X11 wheel buttons 4 to 7 and the Shift key go through XTEST, with
  `--ozone-platform=x11` on WSLg's X server.
- Each OS is captured at 1x and 1.25x with `--force-device-scale-factor`
  (RFC 0010), and every capture runs twice and must match.
- Output: `tests/fidelity/fixtures/wheel-events-v1/chromium-windows.json` and
  `chromium-linux-x11.json`.

Chromium's behavior, from the capture:
- **Notch size:** 100px per notch on Windows (the default 3 lines, at 100/3
  px a line, and 3 characters horizontally) and 120px on Linux, in CSS px at
  both scales. `deltaMode` is 0, and `button` and `buttons` are 0.
- **Shift+wheel:** the event keeps `deltaY`, and the page scrolls
  horizontally.
- **Scroll limit:** a scroll moves at most the scroller's client size, so 3
  notches (300px) moved a 290px-high scroller 290px.

Ctrl+wheel is not captured: Chromium zooms the page, and whether the zoom
lands before the event varies between runs.

Change:
- `div-platform` (`wheel.rs`): `WheelSettings` converts winit's line deltas
  as Chromium does: `WebMouseWheelEventBuilder::Build` on Windows, which
  reads `SPI_GETWHEELSCROLLLINES`/`CHARS` on every event, and
  `kWheelScrollAmount` on Linux. Touchpad pixel deltas are flipped.
  `PlatformEvent::Wheel` carries `delta_mode`, and Windows' page setting
  reports pages.
- `divui`:
  - `Event::delta_mode` (`WheelDeltaMode`), and wheel events report button 0.
  - `Document::dispatch_wheel_event_with_mode` scrolls horizontally for
    Shift with no horizontal delta (`MouseWheelEventQueue`), and doesn't
    scroll for Ctrl (`GetPlatformSpecificDefaultEventAction`; Div has no
    page zoom).
- **Engine** (`crates/div-engine/src/lib.rs`, in its own commit for the
  engine track's review): `scroll_wheel_by` and `scroll_wheel_from_target_by`
  take a `WheelGranularity`. Following cc's `LimitDeltaToScrollerSize`
  (`kLimitScrollDeltaToScrollerSize`) and `ResolveScrollGranularityToPixels`,
  each scroller in the chain limits a pixel or line scroll to its client
  size, resolves a line as 40px, and resolves a page as
  0.875 × min(scroller, viewport). These are clean-room readings of the
  BSD-licensed `cc/input` sources.
- **C ABI:** the frozen v0.2 raw event has no delta mode, so Windows'
  page-mode notch reaches C as one pixel.

Named gaps:
- **`device-scale-scroll-offset`** (engine): at 1.25x Chromium snaps scroll
  offsets to device pixels, so a 50px scroll ends at 150.4. Div keeps CSS
  pixels.
- **`wheel-cancelable`:** with only passive listeners, Chromium's wheel
  events are not `cancelable`; Div's are, though passive listeners still
  cannot cancel them.
- **Untested sources:**
  - X11 smooth-scroll valuators, Wayland and Ctrl with precise touchpad
    deltas follow source reading only.
  - `wheelDeltaX`/`wheelDeltaY` and `deltaZ` are not exposed.

Verification (Windows, MSVC, on main `eed71eab`):
- `cargo test --locked -p divui --features "windows testing" --test wheel_events`
  passes 2 tests. One replays both oracles at both scales, the other covers
  Ctrl, lines and pages from source.
- `cargo test --locked -p div-engine --test wheel_scroll_limits` passes 3
  tests.
- `cargo test --locked -p div-platform --features windows --lib wheel`
  passes 3 tests.
- Mutants: the old direction fails the oracle test; no Shift swap fails both
  divui tests; no clamp fails 2 engine tests.

### 16 November 2026: listener handles and options (Tier 0 item 0.12 A)

Problem (sample-editor gap 29): `Element::on` and `on_capture` returned
nothing, and `remove_event(type)` removed every listener of the type, so a
port could not remove one listener, as `removeEventListener` does. There
were no listener options: `once` and `passive` were missing.

Change, following Chromium 147's `EventTarget` (`event_target.cc`, BSD:
`FireEventListeners` and `SetDefaultAddEventListenerOptions`):

- `on` and `on_capture` return a `ListenerHandle`. It is a plain handle:
  dropping it leaves the listener, and `remove()` removes exactly that
  listener. Removing it again, after its element was removed or after the
  document is gone does nothing. `el.on(..)?;` still compiles.
- `Element::on_with(type, ListenerOptions { capture, once, passive },
  callback)`:
  - a `once` listener is removed before it is called, as Chromium does,
    so a nested dispatch doesn't reach it;
  - `prevent_default` inside a passive listener does nothing (the DOM's
    "in passive listener flag");
  - `passive: None` is the web's default: `touchstart`, `touchmove`,
    `wheel` and `mousewheel` listeners on a top-level node are passive.
    Documents made through `divui` are native, so their root stands for
    the window, document, `<html>` and `<body>`.
- **Behavior change:** a `wheel`, `mousewheel`, `touchstart` or
  `touchmove` listener added with `on` to `Document::body()` can no longer
  cancel the event, as in Chromium; pass `passive: Some(false)` to cancel.
- A listener removed during a dispatch is not called for the rest of it;
  one added during a dispatch waits for the next, as before.
- `remove_event(type)` stays and is documented as having no web
  equivalent. Listeners added through the C ABI keep their behavior (the
  ABI is frozen for v0.2).

WPT `dom/events` (`AddEventListenerOptions-once.any.js`,
`AddEventListenerOptions-passive.any.js`) is testharness script, so its
cases wait for RFC 0005; the tests cover the same behavior.

Verification (Windows, MSVC, this branch on main `33f1828d`): `cargo test
--locked -p divui --features "windows testing" --test listener_options`
passes 7 tests: a handle removes exactly its listener, dropping a handle
keeps the listener, removal during dispatch, `once`, passive
`prevent_default`, Chromium's passive defaults on the root (and `passive:
Some(false)` overriding them), and `capture` in the options. Disabling each
of `once` removal (the nested dispatch then recurses until the test aborts),
the passive flag, the passive default and removal by id fails the tests that
cover it. The full framework test set, the example builds and `cargo fmt
--all --check` pass.

### 16 November 2026: observing the native root leaves the viewport scrollbars out

Problem (from the 0.11 engine review on #52): observing a native
document's root (`Document::body`) reported the viewport's size, with the
viewport's scrollbars inside its content box. Div lays out the root's
content inside them: at a 200 × 100 viewport with `overflow: scroll`, a 1px
border and 5px padding, `clientWidth` is 185 and an auto-width child is 173
wide, but the entry said 200 and 188.

Change: `Engine::resize_observation`
(`crates/div-engine/src/resize_observation.rs`, reviewed by the engine
track) observes the native root as the browser's
`<html>`: its border box is the root scroll area's client rect. RFC 0007
section 3 and `ResizeObserver::observe` say so. The same PR fixes review
notes 3 (RFC wording: each entry has one size because Chromium 147 reports
one stitched size) and 4 (a blank line). The engine track takes note 1, a
broader Chromium oracle for device-pixel snapping.

Oracle: `tools/qualification/capture_resize_observer_root_oracle.py`
captures pinned Windows Chrome-for-Testing 147.0.7727.50 observing `<html>`
styled as the test styles the root, with `overflow` `visible`, `scroll` and
`auto` (500px of content), at 1x and 1.25x, into
`tests/fidelity/fixtures/resize-observer-root-v1/chromium-windows.json`.
Each scale launches Chrome with `--force-device-scale-factor`: with CDP
emulation alone Chrome 147 laid out at 1x scale at 1.25x
(`devicePixelContentBoxSize` 173 rather than 217). This is the real-zoom
oracle of RFC 0010 (#55).

Result: at 1x the inline border, content and device-pixel sizes and the
content rect's origin match Chromium exactly (200, 188 and 188; 185, 173
and 173 with a scrollbar). Two named gaps remain, pinned with their numbers
in `the_native_root_matches_chromium_for_the_root_element`:

- `root-block-size`: the root's block size is the viewport's, less a
  horizontal scrollbar. Its border box is 100, 85 and 100 where `<html>`'s
  `auto` height gives 32, 32 and 512.
- `device-scale-edges`: at 1.25x Chromium lays out at device scale, so a
  1px border takes 0.8px and the scrollbar 15.2px. The inline content box
  is 188.390625 and 173.1875 (236 and 217 device pixels) against Div's 188
  and 173 (235 and 216). This is engine-wide.

Verification (Windows, MSVC, this branch on main `82d27bec`):
`cargo test --locked -p div-engine --lib resize_observation` passes 7 tests
and `cargo test --locked -p divui --features "windows testing" --test
resize_observer` passes 11. With the fix disabled,
`the_native_root_leaves_the_viewport_scrollbars_out` and
`the_native_root_matches_chromium_for_the_root_element` fail (`scroll` at
1x reports 200, 100, 188, 88, 188 and 88 instead of 185, 85, 173, 73, 173
and 73). A second capture reproduces the fixture byte for byte. The full
framework test set, the example builds, `cargo fmt --all --check` and the
regenerated style inventory (unchanged) pass.

### 16 November 2026: dropping a `TextNode` no longer removes it

Problem (reported by the fidelity lane): a text node made with
`Element::create_text_child` never painted, even directly under `body`,
while `set_text` with the same text did. `create_text_child` returned a
`TextNode` whose `Drop` removed the node from the document
(`remove_on_drop`, set only there), so `element.create_text_child("…")?;`
removed the text when the statement ended, and a named handle removed it
at the end of its scope. The only code relying on this was `view!`'s
dynamic text (`view_child.rs`), which dropped its handle in `on_cleanup`.

Change: `TextNode` is a plain handle, as `Element` is and as a reference to
a node is on the web. Dropping it never removes the node; `remove()` does.
The `remove_on_drop` flag and `TextNode`'s `Drop` are gone, and the dynamic
text's scope cleanup calls `TextNode::remove()`. No other handle type
removes on drop: `Element`, `WeakElement`, `WeakTextNode` and
`ScrollButton` have no `Drop`, and `divui` has no comment or fragment
handles.

Verification (Windows, MSVC, this branch on main `154d0a72`): `cargo test
--locked -p divui --features windows --test text_child_ownership` passes 3
tests: a dropped or discarded text child stays (text content `droppedkept`),
a text child renders the same PNG as `set_text` with the same text (and not
a blank one), and `view!`'s dynamic text is removed when its scope is
disposed and no longer updates. On main's code the first two fail and the
third passes. With the fix and a cleanup that only drops the handle, the
third fails. The full framework test set, the example builds and `cargo fmt
--all --check` pass.

### 15 November 2026: `ResizeObserver` (Tier 0 item 0.11)

Problem: Div had no way to learn that an element's box changed size, so
components that size themselves to their container (charts, virtual
lists, editors) had to poll `bounding_rect` after each frame (platform
plan item O1).

Change, as [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 3 specifies, with the engine part in this PR as the parent agreed
(the engine track reviews it):

- **Engine** (`crates/div-engine/src/resize_observation.rs`, new; a
  `mod` and re-export in `lib.rs`; `collect_boxes`, `source_parts` and
  `stitched_size` in `element_dimensions.rs` become `pub(crate)`;
  `docs/renderer/generated/author-style-inventory.json` is regenerated with
  `generate_renderer_contract.py`'s `style_inventory`, which lists the new
  file as a reader of three style fields):
  `Engine::resize_observation(handle) -> ResizeObservation`, read-only after
  layout. It reports Chromium 147's `ResizeObserverUtilities` and
  `ResizeObserverEntry` values (`third_party/blink/renderer/core/resize_observer/`):
  - border and content box sizes in the element's writing mode;
  - the content box in device pixels, `SnapSizeToPixel` at the first box's
    paint offset;
  - the physical content rectangle;
  - the flat-tree depth.

  An element without a layout box (`display: none` or `contents`, a
  non-atomic inline, a detached element) reports zeros. The engine keeps
  no observers.
- **Framework** (`crates/divui/src/resize_observer.rs`, new):
  - `ResizeObserver::new(&document, callback)`, with `observe(&element, ResizeObserverBox)`,
    `unobserve` and `disconnect`;
  - `ResizeObserverEntry`, carrying all three sizes and the content rectangle.

  After `begin_frame`'s layout the document gathers targets at depth 0,
  delivers them (document current, untracked), lays out again and gathers
  deeper targets until none is active, as the specification's "update the
  rendering" steps do. Every target is delivered once when first observed
  (the initial last-reported size is -1 × -1, Chromium's
  `kInitialObservationSize`). Entries use the sizes measured when they were
  gathered, as Chromium's entries read the layout of that pass.

  Skipped targets report "ResizeObserver loop completed with undelivered
  notifications." and wait for the next frame. A new or skipped
  observation makes the window want a frame.
- **Errors** (`crates/divui/src/reported_error.rs`, new):
  - `Document::on_error(handler)` adds a handler for `ReportedError`s;
  - with no handler, errors go to standard error;
  - `TestApp::take_reported_errors` collects them in tests.
- **RFC 0007 amended:**
  - `device_pixel_content_box_size` is a plain `ResizeObserverSize` on every
    entry, as Chromium's `ResizeObserverEntry` constructor fills it;
  - `on_error` handlers are added rather than replaced.

Verification (Windows, MSVC, this branch on main `ee7907e8`):

- `cargo test --locked -p div-engine --lib resize_observation` passes 6
  tests:
  - border and content boxes and the content rect, with `box-sizing`;
  - zeros for `display: none` and `contents`, a non-atomic inline and a
    detached element, and a box for `inline-block`;
  - `vertical-rl` swapping the axes;
  - a stable scrollbar gutter leaving the content box;
  - device-pixel snapping at offsets 0 and 10.5 at scale 1, 1.5 and 2;
  - depth.
- `cargo test --locked -p divui --features "windows testing" --test
  resize_observer` passes 10 tests:
  - first delivery and resize, including a content-box-only observation
    reporting the device-pixel size;
  - only the observed box's change;
  - a box-less target delivered once as 0 × 0;
  - entry and observer order;
  - `unobserve`, `disconnect` and drop;
  - a deeper target resized by a callback delivered in the same frame;
  - a shallower one waiting a frame with the loop error;
  - the document current and untracked;
  - the frame request;
  - `TestApp::take_reported_errors`.
- With the depth rule removed (every changed target active), the two
  loop-error tests fail.
- The full framework test set, the example builds and `cargo fmt --all
  --check` pass.

WPT `resize-observer` is testharness script, so its cases wait for the
translation in [RFC 0005](../../alpha/rfcs/0005-wpt-script-translation.md).
SVG targets (bounding-box sizes) follow with RFC 0009.

### 13 November 2026: `requestAnimationFrame` (Tier 0 item 0.10)

Problem: Div had no `requestAnimationFrame`. The nearest hook,
`PlatformEvent::Presented`, ran after a frame was shown and only for
`App::builder()` windows, and a headless test could render at a time but
not step frames. Five of the six sample investigations need it (R1).

Change, as [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 2 specifies:

- `Document::request_animation_frame(FnOnce(f64)) -> AnimationFrameHandle`
  and `cancel_animation_frame`. `Document::begin_frame` runs the callbacks
  after the animations and their events and before style and layout, in
  request order, each with the frame's time. A callback requested during
  the run waits for the next frame, and a cancel stops a callback due
  later in the same frame. Callbacks run with their document current and
  untracked (`runtime::untracked`, new and crate-private).
- A pending callback makes the window want a frame
  (`PlatformApplication::is_animating`), so it works in every window,
  `App::builder()` and `AppSpec` alike. `Document::is_animating` keeps its
  meaning (engine animations only).
- `HeadlessApp::advance_frame(delta_ms)` and `advance_frames`, and the same
  on `TestApp` (which then runs the ready UI tasks). `TestApp::render_png`
  now renders at the last frame's time instead of 0, so stepping frames
  and then rendering does not rewind the animations.

Verification (Windows, MSVC, this branch on `dd8ba04d`): `cargo test
--locked -p divui --features "windows testing" --test animation_frames`
passes 7 tests: request order and frame time, a callback that requests
itself running once per frame, cancel before and during the frame and of
stale or foreign handles, the document current and its animation time
already advanced, no subscription for the effect that runs the frame,
headless `advance_frame(s)`, and `TestApp` stepping a callback that
resizes an element and rendering at that time. With the callback step
removed from `begin_frame`, 6 of the 7 fail; without `untracked`, the
seventh fails. The `divui` unit test
`a_pending_animation_frame_callback_asks_the_window_for_a_frame` passes.
The full framework test set and the example builds pass on Windows, and
`cargo fmt --all --check` passes. WPT `html/webappapis/animation-frames`
is testharness script, so its cases wait for the translation in
[RFC 0005](../../alpha/rfcs/0005-wpt-script-translation.md).

### 13 November 2026: zbus builds with a fresh lockfile (sample-git gap 24)

Problem: `div-platform` pinned `zbus = "=5.13.0"` (the Linux `dialog`
feature) but not the crates zbus asks for with caret ranges, so an app
that resolved its own lockfile took `zbus_macros` 5.19.0 and `zvariant`
5.15.1, and zbus failed to compile on Linux with 8 errors (`E0425`,
`E0433`: `DispatchResult2` missing from `zbus::object_server`). Div's CI
never met this, because every build, the cli-template job included, used
Div's lockfile.

Change: `div-platform` pins `zbus_macros = "=5.13.0"`, `zvariant =
"=5.9.0"` and `zvariant_derive = "=5.9.0"` beside zbus, Linux-only and
behind `dialog`, at the versions in Div's lockfile. `Cargo.lock` gains only
the three dependency lines. `tools/framework/check_cli_template.py` gains
step 5: for the `default` template it discards the seeded lockfile, runs
`cargo generate-lockfile` and `cargo check --all-targets`, and reports
which packages resolved differently from Div's lockfile. It runs in the
existing cli-template job on Ubuntu and Windows. The lockfile hash in
`docs/v02/generated/dependency-policy.json` is regenerated with
`python3 tools/release/generate_v02_contract.py`; without it the first CI
run failed `--check` in windows-msvc and python-accountability.

Verification (this branch rebased on `ac4f40d8`): a scratch app depending
only on `div-platform` with `dialog` (`out/zbus-probe`), with a fresh
lockfile, in WSL (Ubuntu): on `main` `cargo check` fails with the 8 zbus
errors; with the pins the resolve takes `zbus_macros` 5.13.0, `zvariant`
5.9.0 and `zvariant_derive` 5.9.0 and `cargo check` passes (12.6 s).
`cargo check --locked -p div-platform` passes with `linux,dialog,menu`
(WSL) and `windows dialog menu` (Windows, MSVC). The full step 5 runs in
the PR's cli-template jobs.

### 13 November 2026: views as `{expr}` children (dogfood gap 2)

Problem: `view!` turned every `{expr}` child into text through `ToString`,
so `{For(..)}`, `{Show(..)}` and `{children}` could not be children. The
sample app wrapped them in a `Slot` component.

Change: the expression's type decides. Text (`ToString`) is a text node
re-run in an effect, as before; a view (`IntoView`) is built once and
mounted as itself; anything else is a compile error naming the type.
`view!` evaluates the expression into the doc-hidden
`div::__view_child::Value` and method resolution picks the impl (autoref
specialization). A closure's `Fn`/`FnOnce` kind cannot steer that choice,
so the macro reads the expression by its shape: a variable moves into a
held value the closure borrows (text reads it in place, a view takes it
once), another place such as `{props.title}` is borrowed as before (text
only), and any other expression is evaluated by value. Text expressions
that compiled before compile to the same text.

Verification (Windows, MSVC, this branch rebased on `565db46a`): `cargo
test --locked -p divui --test view_children` passes 4 tests (`For` and
`Show` children updating with their signals, a top-level `For`, text from
a signal, a variable, a `Path::display()`, a field behind an `Rc` and an
owned field staying reactive or static as before with no wrapper
elements, and a `ViewNode` variable, a borrowing `Vec<ViewNode>`, an
`Option<ViewNode>` and a `children: Vec<ViewNode>` prop as views). With
the `view!` from before this change the same file fails to compile, with
7 errors of the form "`ViewNode: ToString` is not satisfied". `cargo test
--locked -p div-macros` passes. A `Signal` written as `{count}` reports
"`Signal<i32>` in a `view!` `{..}` child is neither text (`ToString`) nor a
view (`IntoView`)". The full framework test set and the example builds
pass on Windows.

### 13 November 2026: `on:` on a component tag (dogfood gap 1)

Problem: `view!` dropped every `on:` attribute on a component tag, so
`<Button on:click=...>` compiled and the handler never ran. `style:` and
valueless attributes were dropped the same way.

Choice: forward the listener rather than reject it. `on:event=handler` on a
component goes on the element the component returns, or on each top-level
element of a fragment, through the doc-hidden `div::__forward_listener`.
Reasons: it is what the attribute says when the component renders one
element (as in Leptos), it makes `div-components`' `Button` and every other
component clickable without a handler prop in each, so the components
track needs no API change, and bubbling means clicks inside the component
reach it. `Show`, `For` and `DynChild` render a `display: contents`
container, which receives the listener. A component that returns no
element (only text, or nothing) panics when built, naming the component
and event, instead of dropping the listener. `style:` and valueless
attributes on a component are now compile errors naming the component.

Verification (Windows, MSVC, this branch rebased on `774cf940`):
`cargo test --locked -p divui --test component_events` passes 4 tests (a
root element, clicks bubbling from inside it, each element of a fragment,
a `Show` container, the panic). With the forwarding switched off, as on
`main`, all 4 fail (no clicks recorded; no panic). `cargo test --locked -p
div-macros` passes, including new tests of the forwarding and the two
compile errors.

### 12 November 2026: key names and `repeat` (RFC 0003, request 2)

Change: one always-compiled `div::Key` (it was `menu::Key`, behind the
`menu` feature, plus a copy in `testing`; both now re-export it, and it is
in the prelude). `Event::key()` answers as the DOM's `KeyboardEvent.key`
for `keydown` and `keyup`: named keys come from the key code, the space bar
is `Space`, and a character keeps the case it was typed in, falling back to
the code for a letter or digit sent without text. `Event::repeat` is true
for a `keydown` the platform repeats; `App` now passes winit's flag on
instead of dropping it. `Document::dispatch_key(event_type, key, modifiers,
repeat)` sends a key as the native adapter does (code, DOM name as key
text, typed text on keydown), and `TestApp::press` uses it, so headless and
native named keys carry the same key text.

Verification (Windows, MSVC, this branch, rebased on `322db82`):
`cargo test --locked -p divui --features "menu testing windows" --lib --
keyboard native_key_repeat menu:: testing::` passes the new tests (the key
table both ways, dispatched names and repeat on keydown and keyup, key
input with and without key text, a click reporting no key, characters and
Space typing into an input, native repeat reaching keydown listeners) and
the existing menu accelerator and `TestApp` tests. The Windows CI test set
passes.

### 12 November 2026: `Element::contains` (RFC 0003, request 7)

Change: `Element::contains(&self, other: &Element) -> Result<bool, Error>`
answers as the DOM's `Node.contains`: inclusive, true for any descendant,
false for an element of another document. It walks `other`'s parents in the
engine, so a detached subtree answers the same way, and it runs no layout.
Either element being removed is a `StaleHandle` error, as for
`is_same_node`. The components track's dismissable layer can replace its
parent walk with it.

Verification (Windows, MSVC, this branch):
`cargo test --locked -p divui --lib contains` passes
`element::tests::contains_is_inclusive_and_follows_the_parent_chain`
(self, child, grandchild, sibling, reversed order, a detached subtree,
another document, removed elements).

### 10 November 2026: `default` template and test utilities

Change: `div::testing` (`testing` feature, for dev-dependencies) wraps
`HeadlessApp` as `TestApp`: `mount`, lookups (`by_id`, `by_text`,
`find_text`), input (`click` presses and releases the primary button at an
element's centre through hit testing; `type_text`, `press`, `press_with`)
and `run_until_idle`, which every helper runs after its input. Helpers
panic with what was missing. `assert_snapshot!(app, "name")` compares exact
RGBA, decoded with `png` 0.18.1 (already locked), with
`tests/snapshots/name.<os>.png` in the calling crate. A missing or
different snapshot fails unless `DIV_UPDATE_SNAPSHOTS=1`; a mismatch
writes `name.<os>.actual.png` beside it.

`div new` gains the `default` template, now the template used when none
is named: an `App` with a shared `State`, a menu bar (Open... with a file
dialog, Close Window, Quit, the Edit roles, Reset Count enabled reactively,
Check Connection over HTTP, About), a root view and `tests/ui.rs` with a
click test and a snapshot test. A small `src/lib.rs` holds the state and
modules so the tests can import the views. Templates choose their own
`openui` features through a new `{{framework_source}}` placeholder; `new`
also fills `{{app_id}}` and prints template-specific next steps (recording
the first snapshots). `tools/framework/check_cli_template.py` now checks
both templates, and for `default` requires that `cargo test` fails on the
missing snapshot, records it with `DIV_UPDATE_SNAPSHOTS=1`, then passes
against it.

Also fixed: `HeadlessApp::run_ready_tasks` carried `run_until_idle`'s
documentation.

Verification (Windows, MSVC, this branch): `cargo test -p div-cli`
passes 26 tests; `cargo test -p divui --features testing --lib testing`
passes 2. An app generated with `div new demo --framework-path .`
(template `default`) builds; its first `cargo test` passes the click test
and fails the snapshot test with "there is no accepted snapshot" and writes
`first-screen.windows.actual.png`; `DIV_UPDATE_SNAPSHOTS=1 cargo test`
passes and records `first-screen.windows.png` (5,277 bytes); a plain
`cargo test` then passes 2 of 2, and the dev executable renders its first
frame with `DIV_RENDER_PNG`.

Not done: dialog stand-ins for headless tests, promised in the file dialog
entry, are not in this PR.

### 10 November 2026: HTTP client

Change: `div::http` (`http` feature, which enables `async`) sends
requests with `get`, `post`, `put`, `patch` and `delete`, or through a
`Client` built with a timeout, connect timeout, default headers, user agent,
redirect limit and body limit (10 MiB by default). A `Request` adds
headers, percent-encoded query pairs, a timeout and a raw, text or JSON
body. `send().await` runs it on the blocking pool; `send_blocking()` runs it
on the calling thread. A `Response` holds the status, final URL, headers and
whole body, with `text`, `json` and `error_for_status`; every status is a
response until that call. Errors are a small `http::Error` enum (invalid
URL, host not found, connect, timeout, TLS, protocol, body too large,
status, decode, I/O).

It uses ureq 3.5.1 with rustls 0.23.46 on the ring provider, passed to
ureq explicitly so a process-wide provider set by another crate cannot
change it, and rustls-platform-verifier, so certificates are checked
against the Windows certificate store and the Linux system bundle. The
lockfile gains 24 entries: 15 are built on Windows and 17 on Linux, all
MIT, Apache-2.0, ISC or BSD-3-Clause with MSRV 1.85 or lower. The rest are
macOS, Android or wasm32 dependencies that are locked but never built,
including `webpki-root-certs` (CDLA-Permissive-2.0, wasm32 only). ureq's
`json` feature is not used: it names `cookie_store` weakly, which makes
Cargo lock cookie_store, url and ICU 2 (Rust 1.88) without building them;
JSON goes through serde_json instead. New example: `fetch`.

Verification:

- `cargo test -p divui --features http --lib http --test http` (Windows):
  query encoding and status handling; against a local server, a GET sends
  its query, client and request headers and user agent and reads status,
  headers and body; a JSON POST round-trips; a 404 is a response until
  `error_for_status`; a stalled server times out, an oversized body fails,
  a closed port and an invalid URL fail; `https` to a plain server fails in
  TLS; a UI task awaits a request and sets a signal under
  `HeadlessApp::run_until_idle`.
- The same 9 tests pass on Linux (WSL2, `--features "linux http"`).
- With `DIV_NETWORK_TESTS=1` (local only, not CI), `https://example.com/`
  verifies against the system roots and a self-signed host is rejected as a
  TLS error, on Windows and on WSL2.
- The `fetch` smoke fetches from a server inside the example and exits
  (Windows: software and auto; WSLg: software and OpenGL). CI runs the tests in `rust-parity` and
  `windows-msvc` and the smoke on Windows, X11 and Wayland.

### 10 November 2026: app menu, part 2 (drawn menu bar)

Change: where the platform has no native menu bar (Linux), each window
draws the app menu at its top (`openui/src/menu_bar.rs`). The bar is a
`position: fixed` strip with a spacer before the app's root, so the page
lays out below it and scrolls under it. A click, F10, or Alt with a menu's
mnemonic opens a menu; the arrow keys move between items (skipping
separators and disabled items) and menus, Enter or Space chooses, Escape
closes one level, and an item's mnemonic chooses it. While a menu is open
it takes every key, as native menus do; a full-window layer under the
menus closes them on an outside click. Items show check marks, accelerator
text and submenu arrows, and nested submenus open as flyouts. The bar uses
the `MenuBar`, `Menu`, `MenuItem` and `MenuItemCheckBox` accessibility
roles. A chosen item runs after the event's dispatch, with the window
current, through the same path as native menu commands. Elements of an
earlier drawing are detached during dispatch and destroyed afterwards,
because the engine rejects an event whose target was destroyed by its own
listener (`StaleHandle`).

Remaining gaps, owned by this track unless noted:

- Checked and disabled states reach assistive tools only as an
  accessibility description ("checked", "unavailable"): the engine has no
  toggled or disabled state for authored elements. This needs an engine
  RFC (Workstream A).
- The keyboard highlight does not move accessibility focus, so a screen
  reader does not announce items as the arrows move.
- Mnemonic underlines appear only after F10 or Alt, and Alt alone does not
  open the bar.
- The bar has one light appearance; it does not follow a dark theme.

Verification:

- `cargo test -p divui --features "windows menu" --lib menu_bar` (Windows)
  and `--features "linux menu"` (WSL): a headless document with the drawn
  bar is clicked through real hit testing (a top item opens its menu below
  the bar, an item click chooses and closes, an outside click closes
  without choosing, the closed bar renders the same pixels as before
  opening) and driven by keys (F10, arrows, Escape, Alt+mnemonic, item
  mnemonics, disabled items skipped).
- `menus` smoke passes on WSLg with software and OpenGL presentation.

### 10 November 2026: app menu, part 1

Change: `div::menu` (`menu` feature) declares a typed menu bar:
`Menu`, `MenuItem` (accelerator, reactive `enabled` and `checked`,
`on_select`), `Role` (Quit, Close Window, Undo, Redo, Cut, Copy, Paste,
Select All, About) and `Accelerator`. `AppSpec::menu(build)` gives every
window the menu; an effect in the window's root scope re-resolves it when
the signals it reads change. Accelerators are matched before the focused
document sees the key. Edit roles leave their shortcuts to the document,
which already handles them, so key behaviour is unchanged. On Windows,
`div-platform` shows the menu natively with muda 0.19.3
(`WindowCommand::SetMenu`, `PlatformApplication::menu_selected`). muda adds
four crates: muda, keyboard-types, crossbeam-channel and png, all MIT or
Apache-2.0 and MSRV 1.73 or lower. The runtime now reports F1 to F12 key
codes. The drawn Linux menu bar is the next PR; until then Linux has the
shortcuts and roles without a visible bar.

Verification:

- `cargo test -p divui --features "windows menu" --lib menu` (also on
  Linux): resolution to entries, ids, reactive state, accelerator matching,
  and platform labels and shortcuts.
- Windows: `cargo test -p div-platform --features "windows menu" --test
  menu` reads the native bar through Win32 (labels, accelerator text,
  disabled, checked, separators), posts item commands and sees them reach
  the window's app, then replaces and removes the bar.
- Windows, end to end on the `menus` example: View > Large Text toggles its
  check mark and enables Reset Text Size reactively, File > New Window opens
  a window with the same menu, Close Window closes it and Exit ends the app.
- `menus` smoke passes on Windows (software, auto) and WSL (software,
  OpenGL).

### 9 November 2026: file dialogs

Change: `div::dialog::{open_file, save_file, pick_folder}` build native
dialogs; `pick` and `pick_many` show one on a background thread, owned by
the window that was current when it was built. `div-platform::dialog`
shows the Windows common item dialog (`IFileDialog`) and, on Linux, the XDG
desktop portal's file chooser over D-Bus, falling back to `zenity` when no
portal answers. The runtime now gives controllers each window's native
handle (`WindowController::window_opened`).

Design change from RFC 0002: rfd 0.16.1 would add about 50 crates,
including ICU 2.4 (Rust 1.88, above the 1.85 MSRV). The dialogs use the
`windows` 0.62.2 and zbus 5.13 crates already locked through AccessKit, so
the lockfile gains no packages. The RFC row is updated.

Verification:

- Windows: `cargo test -p div-platform --features "windows dialog" --test
  dialog` shows real dialogs and answers them: save returns the suggested
  non-ASCII name in the given folder, open returns the named file, the
  folder picker returns its folder, and cancel returns nothing.
- Windows, end to end: UI Automation clicks the `dialogs` example's "Save
  as..." button; the dialog is owned by the app window and disables it
  while open, and saving to a typed path writes the note.
- Linux (WSL2): unit tests cover portal options, URI decoding and zenity
  arguments; `dialog_portal` runs a stand-in portal on the session bus and
  checks open, save, folder, cancel, failure responses and the zenity
  fallback. CI runs it under `dbus-run-session`.
- No headless test can show a dialog; `div::testing` will add stand-ins.

### 9 November 2026: clipboard service

Change: `div::clipboard::{read_text, write_text}` use the runtime's
existing Win32, X11 and Wayland text clipboard, which `div-platform` now
hands to controllers as a `Clipboard` (`WindowController::clipboard`).
Without a native runtime they use a clipboard private to the UI thread, so
headless code and tests behave the same. No new dependencies.

Also fixed: with no windows open, an exit request waited for the next
Windows message; the runtime now posts itself a wake when it exits.
Controllers hold only a weak handle, so the runtime drops the clipboard
before the display connection it came from. Before that, every Wayland app
crashed on exit (`SIGSEGV`) when openui released its handle after the
connection closed; the WSL example smokes caught it.

Verification:

- Windows: `cargo test -p div-platform --features windows --test
  clipboard` round-trips non-ASCII text and restores the previous clipboard
  (checked against `Get-Clipboard` before and after); `wake` and
  `multi_window` still pass; the `clipboard` example's smoke reads back what
  it wrote (software and auto) and restores the clipboard.
- `cargo test -p divui --lib clipboard`: the private clipboard round-trips
  and is per thread.
- Linux (WSL2, WSLg Wayland): the compositor refuses clipboard access
  without input ("no events received on any seat"), which the test reports
  rather than fails; `wake` and `multi_window` pass. CI runs the round trip
  and the example smoke on X11 (xvfb) and the test on headless Weston.

### 9 November 2026: async runtime

Change: a new `async` feature of `openui` (enabled by `desktop`; headless
builds gain nothing without it) adds `div::task` with `spawn_local`,
`spawn`, `spawn_blocking`, `Task<T>` (dropping cancels, `detach`) and
`task::time::{sleep, interval}`, plus `Signal::sender()` and
`create_resource(source, fetch)` with `get`, `loading` and `refetch`. It uses
async-task, async-executor, async-io, blocking and futures-lite, all already in
the lockfile (no new crates). UI tasks run on the UI thread in bounded turns
of 8 ms; their wakers post one coalesced wake through a new
`div-platform::LoopWaker` and `WindowController::{started, run_tasks}`.
`Signal` and `Memo` are now `!Send`, so a signal moved into `spawn` is a
compile error (a `compile_fail` doctest checks it). `HeadlessApp` gains
`run_until_idle` (fails with `Error::TasksStillRunning` after 30 s) and
`run_ready_tasks`.

Differences from the RFC: a UI task cancelled with its scope never completes
when awaited (its awaiter normally shares the scope); background tasks are
cancelled by dropping their `Task`, not by scope; the optional `tokio` feature
is left for a later PR.

Verification on Windows (working branch):

- `cargo test -p divui --features async --test tasks`: 5 passed (UI task
  awaiting background work, a timer and blocking work; intervals; scope
  disposal and `Task` drop cancel and drop the futures; senders from another
  thread, including to a disposed signal; resources keep only the latest
  result and refetch).
- `cargo test -p divui --features async --doc task`: 2 passed, including
  the `compile_fail` signal-in-`spawn` check.
- `cargo test -p div-platform --features windows --test wake`: a wake
  from another thread reaches an idle loop (5 wakes sent, coalesced).
- `cargo check --workspace --tests`: passed with `!Send` signals.
- The `tasks` example with `DIV_SMOKE_EXIT=1` (software, auto) exits 0 after
  its background computation and resource fetch finish.
- Linux (WSL2, WSLg Wayland): `wake` and `multi_window` passed with software
  and OpenGL. CI now runs the async tests in `rust-parity` and
  `windows-msvc`, and the `wake` test and `tasks` smoke on X11 and Wayland.

### 9 November 2026: multi-window runtime

Change: `div-platform` runs any number of windows on one winit event loop
(`run_windows` with a `WindowController` that queues `WindowCommand`s); each
window keeps its own presenter, AccessKit adapter, IME and input state.
`openui` adds `App::new(id).name(..).window(WindowSpec, view).run()` and the
`window` module (`open`, `current`, `all`, `WindowKey::unique`,
`WindowHandle::{close, focus, set_title, is_open}`,
`WindowSpec::on_close_requested` returning `Close::Allow`/`Prevent`) and
`app::exit()`. The existing `App::builder()...run(view)` and the C ABI's
`run_document` now adopt their `App` as one window of the same runtime, so
`window::open` works from them too; `platform::run(app, options)` remains as a
single-window controller.

Findings:

- With two animating windows on Windows, the OS delivers paint requests to the
  top window first; re-requesting a frame as soon as a window drew starved the
  other completely (window 2 presented 41,590 frames and window 1 none in
  60 s). Frames are now requested in rounds: each window that wants a frame
  draws once per round; minimized windows are left out and a round ends after
  250 ms, so a window that is never asked to paint cannot stop the others.
- The OpenGL presenter assumed its context stayed current. With one context
  per window it now makes its context current before presenting and before
  deleting its GL objects.
- `window::all()` was added to the RFC's API. Dev-mode window geometry restore
  (RFC section 1) moves to its own small PR.

Verification on Windows (working branch):

- `cargo test -p div-platform --features windows --test multi_window` with
  `DIV_BACKEND` = auto, software and opengl: passed. Two windows open, both
  present, opening an open id focuses it and drops the new app, retitling
  works, closing one keeps the other presenting, and the loop exits after the
  last.
- `cargo test -p divui --features windows --lib window`: 5 passed (key
  dedupe, close, close veto with `window::current()`, exit flag reset).
- `DIV_SMOKE_EXIT=1` smokes of `hello` (software, auto) and the new
  `multi-window` example (software, auto, opengl): exit 0.
- `tools/platform/verify_windows_uia.ps1` against `accessibility.exe`,
  software and auto: passed, including closing the window.
- Linux (WSL2 Ubuntu, WSLg Wayland): the `multi_window` test passed with
  software and OpenGL. X11 could not run there (no `libxkbcommon-x11`); CI's
  hardening job now runs the test and the example smoke on X11 (software,
  Mesa OpenGL) and Wayland (software), and `windows-msvc` smokes the example.

### 9 November 2026: `div-cli` skeleton

Change: a new `crates/div-cli` crate (binary `openui`, no new
dependencies) with `new`, `dev`, `build` and `doctor`, and a `minimal`
template. `div new` writes the Skia build profile from CI into the app's
`.cargo/config.toml`, the workspace `[patch.crates-io]` table (rewritten to the
framework's git source or a local `--framework-path`) and a lockfile seeded from
the workspace's. `div dev` polls the app's sources, rebuilds, and restarts
the app; it stops the old app by closing its stdin (`DIV_DEV`, handled in
`div-platform/src/desktop.rs`). `openui` gains a `desktop` feature that
selects the host platform, and an `DIV_RENDER_PNG` hook that writes the
first frame of `App::run` to a PNG and exits.

Verification on Windows (working branch):

- `cargo test -p div-cli`: 26 passed. Two tests parse the `rust-parity`
  and `windows-msvc` env blocks in `.github/workflows/ci.yml` so the template's
  Skia profile cannot drift from CI.
- `cargo test -p divui render_png_hook_matches_the_headless_first_frame`:
  passed.
- `python tools/framework/check_cli_template.py`: an app made by
  `div new demo --framework-path <repo>` and built by `div build`
  (release) and by plain `cargo build` (dev), with the Skia variables removed
  from the environment, rendered byte-identical PNGs
  (SHA-256 `780C6C3A…261C`). Clean builds took 327 s (release) and 325 s (dev)
  with a cold Skia; an edit-and-rebuild took 3.7 s and the first frame
  appeared in 0.2–0.4 s (dev) and under 0.05 s (release).
- Linux is verified by the new `cli-template` CI job (ubuntu-24.04 and
  windows-2025); its report is uploaded as an artifact.

Follow-up for the engine track: census renders use release Skia (opt-level 3)
while workspace CI tests use dev; the template pins `skia-bindings` to
opt-level 3 in both profiles so apps match census.

### 9 November 2026: RFC 0002

Change: `docs/alpha/rfcs/0002-cli-and-app-layout.md` proposes the CLI
commands, the project template, typed declarations of windows, menus and
services, the async model and the PR order above. Docs only; no code changed.

Findings recorded in the RFC that later PRs depend on:

- A downstream app inherits neither the workspace `[patch.crates-io]` table
  nor the CI Skia build flags, so `div new` must write both.
- `Signal<T>` is `Send` today and panics off the UI thread
  (`openui/src/signal.rs`); the async PR makes it `!Send`.
- `div-platform` checks with `--features linux,windows` on Windows
  (`cargo check -p div-platform --features linux,windows --locked`,
  passed locally), so a single host-selecting `desktop` feature is viable;
  Linux is checked in the CLI PR.
