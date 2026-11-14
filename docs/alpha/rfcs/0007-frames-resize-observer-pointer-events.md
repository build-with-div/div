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
    pub device_pixel_content_box_size: Option<ResizeObserverSize>,
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
  framework's equivalent of `window.onerror`, which receives the message.
  With no handler the error goes to standard error, as Chromium's goes to
  the console. Tests read the errors reported so far with
  `TestApp::take_reported_errors`.
- `device_pixel_content_box_size` is `Some` only for targets observed with
  `DevicePixelContentBox`, as Chromium reports it.
- Each entry carries one size per box. Div does not fragment boxes, so the
  web's arrays always have one element.
- An observer that is dropped stops observing, as a garbage-collected
  observer does. Callbacks run with the document current and untracked.

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

## Affected tracks

- **Components.** Primitives may move from `mouse*` to `pointer*` and use
  capture for drags (slider, resizable panels, scroll area). Code that
  reads `event.pointer_id` sees 1 for the mouse instead of 0. Code that
  compares `event.mouse_button` with `MouseButton::Left` keeps working.
  Tests that click and then assert on async results need `wait_for`.
- **Engine.** `resize_observation` after layout (section 3), the local
  point of a hit for `offset`, and implicit capture release in
  `interaction.rs`. Agreed through the parent before 0.11 starts.
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
3. **0.12**: Pointer Events and the test input of section 5. This changes
   the mouse id and the event order and makes `TestApp` input stop
   waiting, so it updates the template, the showcase snippet and any
   components test that relies on the old behavior in the same PR, and
   lists the changes in the progress log.

Each PR names its WPT areas and the cases ported; cases that need the
script translation of RFC 0005 are listed as pending until it lands.
