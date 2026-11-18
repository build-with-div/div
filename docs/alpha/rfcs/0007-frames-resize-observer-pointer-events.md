# RFC 0007: Animation frames, `ResizeObserver` and Pointer Events

Status: accepted, 13 November 2026. Owner: framework track (`divui`,
`div-platform`). Consumers: the components track, the sample ports (Cv,
Ch, Ed, In, Mo, Git) and the docs. Requests to: the engine track (resize
observations after layout, the local point of a hit). Implements
directions Q4 to Q6 of the platform plan, decided on 13 November 2026
(decision #4). Covers Tier 0 items 0.10 to 0.12. Details reviewed and
accepted on 13 November 2026; the engine part of 0.11 is agreed in
principle, and its PR split is settled before 0.11 starts.

## Problem

Five of the six sample investigations need the same three platform
features, and Div has none of them in web form:

- **Animation frames.** There is no `requestAnimationFrame`. The nearest
  hook, `PlatformEvent::Presented`, runs after a frame is shown and only
  for `App::builder()` windows. A headless test can render at a time
  (`render_at`) but cannot step frames.
- **Resize observation.** There is no `ResizeObserver`. Code that sizes
  itself to a container (charts, editors, virtual lists, panels) polls
  `bounding_rect` or guesses.
- **Pointer Events.** The mouse fires only `mouse*` events, with
  `pointer_id` 0; only touch fires `pointer*`. Pointer capture is never
  released on `pointerup` and fires no `gotpointercapture` or
  `lostpointercapture`. Events carry viewport coordinates only, with no
  `offsetX/Y`, `buttons`, `pointerType`, `isPrimary` or pressure.
- **Tests** (sample-git gap 29). `TestApp::click`, `type_text` and `press`
  call `run_until_idle`, so a test cannot click Cancel while a fetch runs.

The behavior must match Chromium. It is measured with WPT where WPT
covers it (`html/webappapis/animation-frames`,
`html/webappapis/update-rendering`, `resize-observer`, `pointerevents`).
Most of these are testharness scripts, which need the script translation
in [RFC 0005](0005-wpt-script-translation.md).

## Proposal

### 1. The frame (per document, every window)

`Document::begin_frame(time_ms)` follows HTML's "update the rendering" for
one document:

1. run the resize and scroll steps (pending scroll events);
2. update animations and send their events (today's `advance_time`);
3. **run the animation frame callbacks**;
4. update style and layout, then **gather and deliver resize
   observations**, repeating at increasing depth (section 3);
5. paint.

The other steps of "update the rendering" are out of scope for now and
slot in at their places when their features land: evaluating media
queries and reporting changes (before step 2, with the engine's custom
properties and media work), the fullscreen steps (before step 3), and
focus fix-up and `IntersectionObserver` observations (after step 4). The
engine's pipeline changes in [RFC 0006](https://github.com/build-with-div/div/pull/35)
keep this order.

Every window runs through `begin_frame`, whether it was built with
`App::builder()`, `AppSpec` or `HeadlessApp`, so this works in every
window. A minimized window takes no part in frame rounds, so its callbacks
wait until it is shown, as a hidden page's do in Chromium.

### 2. `requestAnimationFrame` (Q4)

```rust
pub struct AnimationFrameHandle(NonZeroU64);     // Copy, Eq, Hash, Debug

impl Document {
    pub fn request_animation_frame(
        &self,
        callback: impl FnOnce(f64) + 'static,
    ) -> AnimationFrameHandle;
    pub fn cancel_animation_frame(&self, handle: AnimationFrameHandle);
}

let document = current_document();
let frame = document.request_animation_frame(move |time_ms| {
    position.set(start + speed * (time_ms - started_at));
});
on_cleanup(move || document.cancel_animation_frame(frame));
```

- The callback receives the frame's time in milliseconds: the `time_ms` of
  `begin_frame`, which is the clock of the document's animation timeline.
  Every callback of one frame gets the same time.
- Callbacks run in the order they were requested. A callback requested
  while callbacks run waits for the next frame. Cancelling a callback that
  has not yet run in this frame stops it. Cancelling a stale or foreign
  handle does nothing, as the web's ids do.
- A callback runs with its document current (`current_document()` works)
  and untracked: it does not subscribe to the signals it reads.
- A pending callback makes the window want a frame, as a running animation
  does today (`is_animating`), so the platform schedules one.
- Handles are per document. There is no free function. A component gets
  its document from `current_document()`.

**Tests (T1).** `HeadlessApp::advance_frame(delta_ms)` and
`TestApp::advance_frame(delta_ms)` run one frame at the document's
animation time plus `delta_ms`; `advance_frames(n, delta_ms)` runs `n`.
`render_at(t)` also runs the frame steps before it paints.

### 3. `ResizeObserver` (Q5)

```rust
pub struct ResizeObserver;                       // Clone; handle to shared state
pub enum ResizeObserverBox { ContentBox, BorderBox, DevicePixelContentBox }
pub struct ResizeObserverSize { pub inline_size: f64, pub block_size: f64 }
pub struct ResizeObserverEntry {
    pub target: Element,
    pub content_rect: Rect,                      // x, y: padding left and top
    pub border_box_size: ResizeObserverSize,
    pub content_box_size: ResizeObserverSize,
    pub device_pixel_content_box_size: ResizeObserverSize,
}

impl ResizeObserver {
    pub fn new(
        document: &Document,
        callback: impl Fn(&[ResizeObserverEntry], &ResizeObserver) + 'static,
    ) -> Self;
    pub fn observe(&self, target: &Element, r#box: ResizeObserverBox);
    pub fn unobserve(&self, target: &Element);
    pub fn disconnect(&self);
}
```

- **Engine part.** After layout, the engine answers, for an element,
  `Engine::resize_observation(handle) -> ResizeObservation`: its border and
  content box sizes in its writing mode's inline and block axes, its
  content box in device pixels snapped as Chromium snaps them, and its
  flat-tree depth. An element with no box reports zero sizes. The engine
  does not store observers.
- **Framework part.** The framework keeps the observers, their targets and
  each target's last reported size in the observed box. After each layout
  of step 4 it gathers the targets whose size changed, at a depth greater
  than the shallowest one delivered in this frame, and calls each
  observer once with its entries in observation order. It then lays out
  again and repeats. The first observation follows the specification's
  initial last-reported size. Targets skipped by the depth rule are
  delivered next frame. As in Chromium, the loop error ("ResizeObserver
  loop completed with undelivered notifications") is reported, not
  thrown: nothing unwinds and the frame continues. Div adds
  `Document::on_error(impl Fn(&ReportedError) + 'static)`, the
  framework's equivalent of the window's `error` event, which receives the
  message. Handlers are added, as listeners are, and run in order.
  With no handler the error goes to standard error, as Chromium's goes to
  the console. Tests read the errors reported so far with
  `TestApp::take_reported_errors`.
- Every entry carries `device_pixel_content_box_size`, whichever box was
  observed. (Amended in the 0.11 PR: this said `Option`, `Some` only for
  `DevicePixelContentBox`. Chromium 147's `ResizeObserverEntry` constructor,
  `third_party/blink/renderer/core/resize_observer/resize_observer_entry.cc`,
  `PopulateFromLayoutBox`, fills all three sizes for every entry, and a
  target with no box gets 0 × 0 for each. The test
  `a_target_is_delivered_when_first_observed_and_when_it_resizes` in
  `crates/divui/tests/resize_observer.rs` observes the content box only
  and checks the device-pixel size.)
- Each entry carries one size per box, where the web has arrays. Chromium
  147 reports one size per entry, stitched across a box's fragments (such
  as multicolumn continuations), and Div stitches them the same way.
- An observer that is dropped stops observing, as a garbage-collected
  observer does. Callbacks run with the document current and untracked.
- Observing a native document's root (`Document::body`) reports the
  browser's `<html>`. The root is both the viewport and the root element,
  and the viewport's scrollbars are outside its box, so with a scrollbar its
  border box is the viewport's client area. (Added after the 0.11 PR's
  engine review.) The oracle, pinned Chromium 147 on Windows at a 200 × 100
  viewport, is `tests/fidelity/fixtures/resize-observer-root-v1`, and the
  test `the_native_root_matches_chromium_for_the_root_element` checks Div
  against it. At 1x the inline sizes match exactly: 185 and 173 with a
  scrollbar. Two gaps remain:
  - `root-block-size`: the root's block size is the viewport's, less a
    horizontal scrollbar, where `<html>`'s `auto` height is its content's
    (border box 100, 85 and 100 against 32, 32 and 512).
  - `device-scale-edges`: at 1.25x Chromium lays out at device scale, so a
    1px border takes 0.8px and the scrollbar 15.2px (inline content box
    188.39 and 173.19 against Div's 188 and 173). Div lays out in CSS
    pixels at every scale. This is engine-wide.

### 4. Pointer Events (Q6)

The `Event` struct keeps its fields, so existing code compiles. Typed data
is read through accessors, as `Event::key` and `input_info` are today:

```rust
pub enum PointerType { Mouse, Pen, Touch }
pub struct PointerId(pub i32);                   // PointerId::MOUSE == PointerId(1)
pub enum MouseButton { Main = 0, Auxiliary = 1, Secondary = 2, Back = 3, Forward = 4, Eraser = 5 }
bitflags! { pub struct Buttons: u16 {           // web `buttons` values
    const PRIMARY = 1; const SECONDARY = 2; const AUXILIARY = 4;
    const BACK = 8; const FORWARD = 16; const ERASER = 32; } }

pub struct MouseInfo {                           // MouseEvent
    pub button: Option<MouseButton>,             // None for the web's -1
    pub buttons: Buttons,
    pub client: Point, pub offset: Point, pub screen: Point,
    pub modifiers: Modifiers,
}
pub struct PointerInfo {                         // PointerEvent
    pub mouse: MouseInfo,
    pub pointer_id: PointerId,
    pub pointer_type: PointerType,
    pub is_primary: bool,
    pub width: f64, pub height: f64,
    pub pressure: f32, pub tangential_pressure: f32,
    pub tilt_x: i32, pub tilt_y: i32, pub twist: i32,
    pub altitude_angle: f64, pub azimuth_angle: f64,
}

impl Event {
    pub fn mouse_info(&self) -> Option<&MouseInfo>;     // mouse*, click, wheel, pointer*
    pub fn pointer_info(&self) -> Option<&PointerInfo>; // pointer*, click, auxclick, contextmenu
}
impl Element {
    pub fn set_pointer_capture(&self, id: PointerId) -> Result<(), Error>;
    pub fn release_pointer_capture(&self, id: PointerId) -> Result<(), Error>;
    pub fn has_pointer_capture(&self, id: PointerId) -> bool;
}
```

`MouseButton` keeps its numbering: `Left`, `Middle` and `Right` become
deprecated associated constants for `Main`, `Auxiliary` and `Secondary`
until their users move. The values are the web's, so a port copies them
unchanged.

Behavior, for mouse, pen and touch from the platform and from tests:

- **Ids.** The mouse is `pointerId` 1, as in Chromium. Each new touch or
  pen contact gets an id above 1 that no active pointer uses; today's
  touch ids (`touch.id + 1`) could collide with the mouse, so the platform
  layer remaps them. The mouse is always primary; the first active touch
  is primary.
- **Order.** A device event fires the pointer event first, then the
  compatibility mouse event: `pointerover`, `pointerenter`, `pointerdown`,
  then `mouseover`, `mouseenter`, `mousedown`, and likewise for move, up,
  out and leave. `preventDefault` on `pointerdown` suppresses the
  compatibility `mousedown`, `mousemove` and `mouseup` until the pointer
  is released; it never suppresses over, out, enter, leave or `click`.
- **Chorded buttons.** Pressing a second button while one is held fires
  `pointermove` with `button` set to the changed button, not a second
  `pointerdown`; the compatibility `mousedown` still fires.
- **Capture.** `set_pointer_capture` fails with `NotFound` for an inactive
  pointer and `InvalidState` for a disconnected element, and does nothing
  while no button is pressed. Capture takes effect at the next pointer
  event, which is preceded by `gotpointercapture`. It is released
  implicitly after `pointerup` and `pointercancel`, with
  `lostpointercapture`, and when the capturing element leaves the
  document. Touch contacts are captured implicitly by their `pointerdown`
  target, as in Chromium.
- **Coordinates.** `client` is the viewport point; `offset` is that point
  in the target's padding box, through its transforms (the engine's hit
  test supplies the local point); `screen` adds the window's position
  (equal to `client` headless).
- **Defaults.** A device without pressure reports 0.5 while a button is
  pressed and 0 otherwise; `width` and `height` are 1; tilt and twist are
  0, and the angles are their web defaults.
- `click`, `auxclick` and `contextmenu` are `PointerEvent`s with the id of
  the pointer that caused them. Which element `click` targets, `dblclick`,
  `detail` and the right-button rules belong to I2 and are unchanged here.

### 5. Test input (T1, sample-git gap 29)

`TestApp` input returns once the event is dispatched and the UI work it
made ready has run (`run_ready_tasks`, the microtask checkpoint's
equivalent). It no longer waits for timers or background work. Waiting is
explicit, as with Testing Library's `waitFor` and Playwright's assertions:

```rust
app.click(&app.by_text("Fetch"));
app.click(&app.by_text("Cancel"));               // the fetch is still running
app.wait_for(|app| app.find_text("Cancelled").is_some());
let row = app.find_by_text("Loaded");            // waits, then panics if absent
```

- `wait_for(condition)` runs tasks, timers and frames until `condition`
  holds, and panics after `testing::DEFAULT_WAIT_TIMEOUT`, a documented
  constant of 1 s, as Testing Library's default;
  `wait_for_with(timeout, condition)` sets another. `find_by_text` and
  `find_by_id` wait for an element. `run_until_idle` stays.
- New input helpers, each a sequence of pointer events with the defaults
  above: `pointer_move`, `pointer_down`, `pointer_up` (with a button and
  modifiers), `right_click`, `double_click`, `drag(from, to, steps)`,
  `wheel(at, delta)`, and `touch_*` equivalents. `click` moves the mouse
  to the element first, as today.

### 6. Click counting (amended 17 November 2026)

Amended after 0.12 C, agreed through the parent session: `dblclick` and
`detail` were the remaining part of the I2 note in section 4, and the
editor's double-click word selection and any multi-click UI need them.

```rust
pub struct MouseInfo { /* as above */ pub detail: u32 }   // UIEvent.detail
pub struct PointerInput { /* as before */ pub click_count: Option<u32> }
impl PointerInput { pub fn with_click_count(self, count: u32) -> Self; }

pub struct DoubleClickSettings { pub time: Duration, pub width: u32, pub height: u32 }
pub enum ClickCountRule { Windows, Linux }       // ClickCountRule::host()
pub struct ClickCounter;                         // new(rule, settings), count(ClickInput)
pub struct TapCounter;                           // tap(down, down_time, up_time)
```

- **Who counts.** The browser counts, as Chromium does, so each document
  counts its mouse and pen input with the rule of the platform it is
  built for. An input's own `click_count` overrides the count of its
  press or release; for a touch release it is the tap's count.
- **Windows.** The system's double-click time and box
  (`GetDoubleClickTime`, `SM_CXDOUBLECLK`, `SM_CYDOUBLECLK`), read by the
  native runtime for every input, as Chromium reads them for every mouse
  message. A press counts on within half the box of the series' first
  press, within the time of the previous press, with the same button.
  There is no limit. A move with no button held that leaves the box or
  comes after the time ends the series.
- **Linux.** Chromium 147's own limits (500 ms, 4×4 device pixels, from
  `ui/events/event.cc`); the desktop's GTK or X settings are ignored,
  because Div matches Chromium, not the desktop. A press counts on within
  the time and half the box of the previous press, with the same button
  and modifiers. The press after a triple click counts 1 again.
- **Touch.** Chromium's gesture detector, on both platforms: a tap whose
  press is within 400 ms of the previous tap's release and less than
  20 CSS pixels from its press counts on, 1, 2, 3, then 1 again.
- **Events.** `detail` is the count on `mousedown`, `mouseup`, `click`,
  `auxclick` and `dblclick`, and 0 on every other event. A main-button
  `click` (or a tap's `click`) whose count is 2 is followed by
  `dblclick`, a `MouseEvent`, at the same target, even when `pointerdown`
  was cancelled. `contextmenu` follows the secondary button's `auxclick`
  on Windows and its `mousedown` on Linux, as in Chromium.
- **Tests.** `TestApp::click`, `right_click` and `tap` give each press
  count 1, as Playwright's and Puppeteer's `click` do, so tests do not
  depend on the clock; `double_click` gives 1 then 2, and the new
  `triple_click` 1, 2, 3. `pointer_down` and `pointer_up` leave the count
  to the document.
- **Oracle.** `tests/fidelity/fixtures/click-count-v1`: real platform
  input to pinned Chrome-for-Testing 147.0.7727.50 (messages posted to
  the harness's own window on Windows, XTEST on Linux), at scales 1 and
  1.25, each capture taken twice and required to match.

### 7. Keyboard scrolling and `keypress` (amended 17 November 2026)

Amended for I2 (sample-markdown MD-23, the last Tier 0 item), agreed
through the parent session. Div had no `keypress` and no keyboard
scrolling: arrows, Page Up, Page Down, Home, End and the space bar did
nothing outside form controls.

```rust
// div-engine (engine-reviewed)
pub enum ScrollGranularity { Line, Page, Document }
impl Engine {
    pub fn scroll_logical(&mut self, direction: ScrollButtonDirection,
                          granularity: ScrollGranularity) -> Result<bool, EngineError>;
    pub fn mouse_press_node(&self) -> Option<NodeHandle>;
}
// divui: no new API. `keypress` is a new event type; `Event::key` reports it.
```

- **`keypress`.** It follows a `keydown` that was neither cancelled nor
  handled by a default action (focus moved, the caret moved, a control
  changed, a scroller scrolled), as Chromium suppresses the character
  event after such a keydown. It goes to the focused element, else the
  root. A key that commits text without Ctrl or Meta, the space bar, and
  Enter (also with Shift or Ctrl) have one; Escape and Backspace do not.
  On Linux Delete has one (X11 sends it the character 127); on Windows it
  does not. Its `key_code` is the character's code, as the DOM's
  `charCode`.
- **Its default action** commits the key's text, so cancelling `keypress`
  now stops the text, as in Chromium. On a space that nothing consumed it
  scrolls a page towards the block end, or the block start with Shift;
  Ctrl, Alt and Meta stop it. Text editing, buttons, checkboxes, radios
  and selects consume the space.
- **Keydown scrolling.** After the existing key defaults, an unhandled
  keydown scrolls as Chromium's `MapKeyCodeForScroll` maps it on Windows
  and Linux: arrows a line (40 px) on their physical axis, Page Up and
  Page Down a page, Home and End the whole range, in the block direction.
  Shift, Meta and Alt never scroll; Ctrl scrolls only with Home and End.
  Text controls and editable content keep arrows, Home and End for the
  caret, but Page Up and Page Down scroll. A radio scrolls when it has no
  other radio to move to.
- **The chain** (`ScrollManager::LogicalScroll`): from the focused element,
  else the node the latest press hit (kept when `mousedown` is cancelled;
  its former parent when it is removed), else the root. Each scroller on
  the way out scrolls if its overflow lets the user scroll on that axis;
  the first that moves ends it. A page is 87.5% of the pixel-snapped
  scroll snapport (the client area less `scroll-padding`), at least 1 px,
  as the native scroll buttons page.
- **Oracle.** `tests/fidelity/fixtures/keyboard-scroll-v1`: real key input
  to pinned Chrome-for-Testing 147.0.7727.50 (Win32 key messages to the
  foreground window on Windows, run once with the user's approval; XTEST
  on X11 in WSL), at scales 1 and 1.25, each capture taken twice and
  required to match. 19 scenarios on Linux, 17 on Windows (Windows has no
  modifier scenarios: posted modifiers do not reach Chrome).
- **Gaps** (named in the test, each still differing):
  - `keyboard-focusable-scrollers` (engine and framework): a scroller with
    no focusable descendant is keyboard focusable, and sequential
    navigation starts at the clicked node. I2b, a separate PR.
  - `overscroll-behavior` (engine): Div's style system has no
    `overscroll-behavior`, so `contain` does not end the chain.
  - `edit-caret-reveal` (framework): typing into a control scrolled out of
    view does not reveal the caret.
  - `device-scale-scroll-offset` (engine, RFC 0011): at 1.25 Chromium's
    scrollbars are 19 device pixels and offsets land on device pixels.
  - `keyboard-smooth-scroll` (engine): Chromium animates keyboard
    scrolls; Div jumps. The oracle records settled offsets.
  - `keyboard-scrollend` (framework): Chromium fires `scrollend` after
    the keyup; Div has no `scrollend` event yet.
  - `windows-modified-keys-unmeasured`: posted Win32 modifiers do not
    reach Chrome, so Windows uses the modifier rules measured on Linux.
  - `touch-press-node` (engine and framework, unmeasured): Div sets the
    press node on every pointer's press. Blink sets it in the mouse press
    path, which touch reaches only through a tap's synthesized `mousedown`
    at the tap, so a touch that pans or is cancelled leaves it alone
    there. The oracle uses only the mouse.
  - Unmeasured and left as Chromium's source reads: snapping after a
    keyboard scroll (not applied yet), Enter's `keypress` with Alt or
    Meta (none), Delete's with modifiers (none).

## Affected tracks

- **Components.** Primitives may move from `mouse*` to `pointer*` and use
  capture for drags (slider, resizable panels, scroll area). Code that
  reads `event.pointer_id` sees 1 for the mouse instead of 0. Code that
  compares `event.mouse_button` with `MouseButton::Left` keeps working.
  Tests that click and then assert on async results need `wait_for`.
- **Components (section 7).** A component that handles arrows, Page Up,
  Page Down, Home, End or the space bar itself (menus, listboxes, sliders,
  tabs) must call `prevent_default` on the `keydown` (or on the `keypress`
  for the space bar), or the page also scrolls, as in a browser. Text
  committed by a key now arrives after `keypress`, and cancelling
  `keypress` stops it.
- **Engine.** `resize_observation` after layout (section 3), the local
  point of a hit for `offset`, and implicit capture release in
  `interaction.rs`. Agreed through the parent before 0.11 starts.
- **Engine (section 7).** `scroll_logical`, `ScrollGranularity` and the
  press node (`mouse_press_node`), in one engine-reviewed commit;
  `scroll_button`'s page step is shared, unchanged.
- **FFI.** The C ABI is frozen for v0.2; its capture functions keep their
  `u32` ids, and the mouse id it sees becomes 1.
- **Docs and templates.** The `default` template's test and the showcase
  snippet move to `wait_for` where they await work.

## Migration

One PR each, in the framework lane, after this RFC merges:

1. **0.10**: the frame steps of section 1, `request_animation_frame`, and
   `advance_frame`. No existing behavior changes.
2. **0.11**: the engine's `resize_observation` (engine track, or in this
   PR with its agreement), then `ResizeObserver`.
3. **0.12**, in three PRs (amended 16 November 2026, agreed through the
   parent session):
   - **0.12 A**: listener handles and options (`passive`, `once`), which
     the wheel work (0.9) needs first.
   - **0.12 B**: Pointer Events (section 4), with the engine's capture and
     local-point changes reviewed by the engine lane. This changes the
     mouse id and the event order, so it updates any test that relies on
     the old behavior and lists the changes in the progress log.
   - **0.12 C**: the test input of section 5, which makes `TestApp` input
     stop waiting, so it updates the template, the showcase snippet and
     any components test that awaits work.
4. **Click counting** (section 6, amended 17 November 2026): `detail`,
   `dblclick`, the per-platform counters and the per-platform
   `contextmenu` timing. The C ABI is unchanged.

Each PR names its WPT areas and the cases ported; cases that need the
script translation of RFC 0005 are listed as pending until it lands.
