//! Pointer Events dispatch (RFC 0007 section 4), in Chromium 147's order.
//!
//! The order and suppression rules are pinned by the oracle
//! `tests/fidelity/fixtures/pointer-events-v1` (a Windows and a Linux X11
//! capture; `crates/divui/tests/pointer_events_oracle.rs`):
//!
//! - Pending capture is processed first: `lostpointercapture`, then
//!   `gotpointercapture`.
//! - Boundary events follow: `pointerout`, `pointerleave` (innermost
//!   first), `pointerover`, `pointerenter` (outermost first), then the same
//!   four as mouse events for a mouse or pen.
//! - The pointer event goes to the target, then the compatibility mouse
//!   event. A second button pressed or a button released while another is
//!   held is a `pointermove`. A cancelled `pointerdown` suppresses
//!   `mousedown`, `mousemove` and `mouseup` until every button is released;
//!   focus is `mousedown`'s default, so it does not move either.
//! - After the last button is released, capture ends with
//!   `lostpointercapture`, then the release clicks: `click` for the main
//!   button, `auxclick` for another, and `contextmenu` after the secondary
//!   button's `auxclick`. A pointer whose capture ended is hit-tested again.
//! - A touch is captured by its `pointerdown` target before that event, and
//!   leaves its whole hover path when lifted. A tap (a touch that never moved
//!   more than 15px, `max_touch_move_in_pixels_for_click_` in
//!   `ui/events/gesture_detection/gesture_configuration.h`) then fires the
//!   compatibility mouse events through the mouse and a `click`.
//! - Click counts (`detail`) come from [`crate::click_count`]: the input's
//!   own count, or the document's counter. `detail` is the count on
//!   `mousedown`, `mouseup`, `click`, `auxclick` and `dblclick`, and 0
//!   elsewhere. A main-button `click` (or a tap's) whose count is 2 is
//!   followed by `dblclick` at the same target. On Linux `contextmenu`
//!   follows the secondary button's `mousedown`; on Windows it follows its
//!   `auxclick`. The oracle is `tests/fidelity/fixtures/click-count-v1`.
//!   Inputs are timed when the document receives them, since platform
//!   input carries no time of its own here; Chromium uses the message's
//!   time.

use super::*;
use crate::click_count::{
    ClickCountRule, ClickCounter, ClickInput, DoubleClickSettings, TapCounter,
};
use crate::pointer::{
    Buttons, MouseInfo, Point, PointerId, PointerInfo, PointerInput, PointerInputKind, PointerType,
};
use div_engine::{CaptureChange, PointerUpdate};

/// The distance a touch may move and still tap, in CSS pixels.
const TAP_SLOP: f32 = 15.0;

/// Whether `contextmenu` follows the secondary button's press, as on Linux,
/// rather than its release, as on Windows.
const CONTEXTMENU_ON_PRESS: bool = !cfg!(target_os = "windows");

#[derive(Debug)]
pub(crate) struct PointerDispatchState {
    pointers: HashMap<PointerId, ActivePointer>,
    clicks: ClickCounter,
    taps: TapCounter,
    /// The clock of the click and tap counters.
    epoch: std::time::Instant,
}

impl Default for PointerDispatchState {
    fn default() -> Self {
        Self {
            pointers: HashMap::new(),
            clicks: ClickCounter::new(ClickCountRule::host(), DoubleClickSettings::DEFAULT),
            taps: TapCounter::new(),
            epoch: std::time::Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ActivePointer {
    pointer_type: PointerType,
    buttons: Buttons,
    is_primary: bool,
    /// The `pointerdown` was cancelled: compatibility mouse events are
    /// suppressed until every button is released.
    suppress_mouse: bool,
    down_at: (f32, f32),
    /// When the first button was pressed or the touch started, on the
    /// counters' clock.
    down_time: std::time::Duration,
    /// A touch moved beyond the tap slop.
    moved: bool,
}

/// What one input changed, as dispatch needs it.
#[derive(Clone, Copy)]
struct Step {
    id: PointerId,
    pointer_type: PointerType,
    is_primary: bool,
    buttons: Buttons,
    /// The button pressed or released, for `button`. The input's pointer
    /// events, its boundary events and its capture events all report it, as
    /// the oracle shows; `None` (the web's -1) for a move.
    changed: Option<MouseButton>,
    first_down: bool,
    /// An up that leaves no button pressed: `pointerup`, not `pointermove`.
    last_up: bool,
    x: f32,
    y: f32,
    modifiers: Modifiers,
    pressure: Option<f32>,
    /// The click count: of a mouse press or release, or of a tap.
    detail: u32,
}

impl Step {
    fn mouse_like(&self) -> bool {
        self.pointer_type != PointerType::Touch
    }
}

impl Document {
    /// Dispatch one input from a pointing device, as Chromium does: pointer
    /// events, then compatibility mouse events for a mouse or pen, then the
    /// release's click. See the module documentation of `pointer_dispatch`
    /// for the order.
    pub fn dispatch_pointer_input(&self, input: PointerInput) -> Result<(), Error> {
        if !input.x.is_finite() || !input.y.is_finite() {
            return Err(Error::InvalidArgument("pointer coordinates must be finite"));
        }
        if input.kind == PointerInputKind::Leave {
            return self.dispatch_window_leave(&input);
        }
        let step = self.record_pointer_input(&input)?;
        let step = self.count_clicks(&input, step)?;
        let kind = match input.kind {
            PointerInputKind::Down => PointerEventKind::Down,
            PointerInputKind::Up => PointerEventKind::Up,
            PointerInputKind::Move => PointerEventKind::Move,
            PointerInputKind::Cancel | PointerInputKind::Leave => PointerEventKind::Cancel,
        };
        let update = self.with_engine_mut(|engine| {
            engine.pointer_event_with_buttons(
                step.id.engine(),
                kind,
                step.x,
                step.y,
                step.buttons.bits(),
            )
        })?;
        self.dispatch_capture_changes(&step, &update.capture_changes)?;
        self.dispatch_boundaries(&step, &update, true, step.mouse_like())?;
        let Some(target) = update.target else {
            return self.finish_pointer(&step, input.kind, None, &update);
        };
        if step.pointer_type == PointerType::Touch && step.first_down {
            // Chromium's implicit touch capture is set before `pointerdown`
            // is dispatched, so its listeners can release it
            // (`PointerEventManager::SendTouchPointerEvent`).
            self.with_engine_mut(|engine| engine.set_pointer_capture(step.id.engine(), target))?;
        }
        let name = match input.kind {
            PointerInputKind::Down if step.first_down => "pointerdown",
            PointerInputKind::Up if step.last_up => "pointerup",
            PointerInputKind::Cancel => "pointercancel",
            _ => "pointermove",
        };
        // A listener can remove the target, so its path is read first.
        let path = self.target_path(target)?;
        let event = self.pointer_event(&step, name, target, step.changed, step.is_primary)?;
        self.dispatch_to(target, &event)?;
        let mut prevented = event.default_prevented();
        if step.first_down && prevented {
            self.update_pointer(step.id, |pointer| pointer.suppress_mouse = true)?;
        }
        let suppressed = self.pointer_suppresses_mouse(step.id)?;
        if step.mouse_like() && !suppressed {
            let mouse_name = match input.kind {
                PointerInputKind::Down => Some("mousedown"),
                PointerInputKind::Up => Some("mouseup"),
                PointerInputKind::Move => Some("mousemove"),
                PointerInputKind::Cancel | PointerInputKind::Leave => None,
            };
            if let Some(mouse_name) = mouse_name {
                let mouse_target = self.surviving_target(&path)?;
                let mouse = self.mouse_event(&step, mouse_name, mouse_target, step.changed)?;
                self.dispatch_to(mouse_target, &mouse)?;
                prevented |= mouse.default_prevented();
                // Focus is `mousedown`'s default action: a cancelled or
                // suppressed `mousedown` leaves it (sample-canvas gap 16).
                if input.kind == PointerInputKind::Down && !mouse.default_prevented() {
                    self.press_focus(self.surviving_target(&path)?, step.x, step.y)?;
                }
            }
        }
        if CONTEXTMENU_ON_PRESS
            && step.mouse_like()
            && input.kind == PointerInputKind::Down
            && step.changed == Some(MouseButton::Secondary)
        {
            let menu_target = self.surviving_target(&path)?;
            let menu =
                self.pointer_event(&step, "contextmenu", menu_target, step.changed, false)?;
            self.dispatch_to(menu_target, &menu)?;
        }
        if !prevented {
            if let Some((range, fraction)) = update.range_value {
                if self.with_engine_mut(|engine| engine.set_range_fraction(range, fraction))? {
                    self.dispatch_to(range, &Event::keyboard("input", 0, None, step.modifiers))?;
                }
                if input.kind == PointerInputKind::Up {
                    self.dispatch_to(range, &Event::keyboard("change", 0, None, step.modifiers))?;
                }
            }
        }
        let target = self.surviving_target(&path)?;
        self.finish_pointer(&step, input.kind, Some(target), &update)
    }

    /// Whether `node` is live and connected. A removed node's handle is stale.
    fn is_in_document(&self, node: NodeHandle) -> Result<bool, Error> {
        self.with_engine(|engine| engine.is_connected(node).unwrap_or(false))
    }

    /// `target` and its ancestors, innermost first.
    fn target_path(&self, target: NodeHandle) -> Result<Vec<NodeHandle>, Error> {
        self.with_engine(|engine| {
            let mut path = vec![target];
            let mut node = target;
            while let Some(parent) = engine.parent(node)? {
                path.push(parent);
                node = parent;
            }
            Ok::<_, div_engine::EngineError>(path)
        })?
        .map_err(Into::into)
    }

    /// The innermost node of `path` still in the document, where Chromium
    /// sends the rest of a pointer step after a listener removes its target
    /// (`PointerEventManager::SendMousePointerEvent`,
    /// `MouseEventManager::HandleRemoveSubtree`).
    fn surviving_target(&self, path: &[NodeHandle]) -> Result<NodeHandle, Error> {
        self.with_engine(|engine| {
            path.iter()
                .copied()
                .find(|&node| engine.is_connected(node).unwrap_or(false))
                .unwrap_or_else(|| engine.root())
        })
    }

    /// Update the pointer's buttons for `input` and describe the step.
    fn record_pointer_input(&self, input: &PointerInput) -> Result<Step, Error> {
        let mut state = self
            .inner
            .pointers
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantMutation)?;
        let now = state.epoch.elapsed();
        let other_touch = state.pointers.iter().any(|(id, pointer)| {
            *id != input.pointer_id && pointer.pointer_type == PointerType::Touch
        });
        let pointer = state
            .pointers
            .entry(input.pointer_id)
            .or_insert(ActivePointer {
                pointer_type: input.pointer_type,
                buttons: Buttons::NONE,
                // The mouse is always primary; the first active touch is.
                is_primary: input.pointer_type != PointerType::Touch || !other_touch,
                suppress_mouse: false,
                down_at: (input.x, input.y),
                down_time: now,
                moved: false,
            });
        pointer.pointer_type = input.pointer_type;
        let before = pointer.buttons;
        let (after, changed) = match input.kind {
            PointerInputKind::Down => (before.with(Buttons::of(input.button)), Some(input.button)),
            PointerInputKind::Up => (
                before.without(Buttons::of(input.button)),
                Some(input.button),
            ),
            PointerInputKind::Cancel => (Buttons::NONE, None),
            PointerInputKind::Move | PointerInputKind::Leave => (before, None),
        };
        let first_down = input.kind == PointerInputKind::Down && before.is_empty();
        if first_down {
            pointer.suppress_mouse = false;
            pointer.down_at = (input.x, input.y);
            pointer.down_time = now;
            pointer.moved = false;
        }
        if !before.is_empty()
            && (input.x - pointer.down_at.0).hypot(input.y - pointer.down_at.1) > TAP_SLOP
        {
            pointer.moved = true;
        }
        pointer.buttons = after;
        Ok(Step {
            id: input.pointer_id,
            pointer_type: input.pointer_type,
            is_primary: pointer.is_primary,
            buttons: after,
            changed,
            first_down,
            last_up: input.kind == PointerInputKind::Up && after.is_empty(),
            x: input.x,
            y: input.y,
            modifiers: input.modifiers,
            pressure: input.pressure,
            // A touch's tap count is known when it lifts (finish_pointer).
            detail: match (input.pointer_type, input.kind) {
                (PointerType::Touch, PointerInputKind::Up) => input.click_count.unwrap_or(0),
                _ => 0,
            },
        })
    }

    /// The click count of a mouse or pen input: its own, or the document's
    /// counter's. Every such input reaches the counter, because a move can
    /// end a series on Windows.
    fn count_clicks(&self, input: &PointerInput, mut step: Step) -> Result<Step, Error> {
        if !step.mouse_like() {
            return Ok(step);
        }
        let scale = self.with_engine(|engine| engine.viewport().device_scale_factor())?;
        let mut state = self
            .inner
            .pointers
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantMutation)?;
        let time = state.epoch.elapsed();
        let counted = state.clicks.count(ClickInput {
            kind: input.kind,
            button: input.button,
            buttons_held: !step.buttons.is_empty(),
            modifiers: input.modifiers,
            x: f64::from(input.x) * scale,
            y: f64::from(input.y) * scale,
            time,
        });
        step.detail = match input.kind {
            PointerInputKind::Down | PointerInputKind::Up => input.click_count.unwrap_or(counted),
            _ => 0,
        };
        Ok(step)
    }

    /// Use `settings` for the document's click counter from the next input
    /// on. The native runtime reads the platform's before each input.
    pub(crate) fn set_double_click_settings(
        &self,
        settings: DoubleClickSettings,
    ) -> Result<(), Error> {
        self.inner
            .pointers
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantMutation)?
            .clicks
            .set_settings(settings);
        Ok(())
    }

    fn update_pointer(
        &self,
        id: PointerId,
        change: impl FnOnce(&mut ActivePointer),
    ) -> Result<(), Error> {
        let mut state = self
            .inner
            .pointers
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantMutation)?;
        if let Some(pointer) = state.pointers.get_mut(&id) {
            change(pointer);
        }
        Ok(())
    }

    fn pointer_suppresses_mouse(&self, id: PointerId) -> Result<bool, Error> {
        Ok(self
            .inner
            .pointers
            .try_borrow()
            .map_err(|_| Error::ReentrantMutation)?
            .pointers
            .get(&id)
            .is_some_and(|pointer| pointer.suppress_mouse))
    }

    /// The release's capture end, clicks and rehover, and a lifted touch's
    /// leave and tap.
    fn finish_pointer(
        &self,
        step: &Step,
        kind: PointerInputKind,
        target: Option<NodeHandle>,
        update: &PointerUpdate,
    ) -> Result<(), Error> {
        let ends =
            (kind == PointerInputKind::Up && step.last_up) || kind == PointerInputKind::Cancel;
        let mut capture_ended = false;
        if ends {
            let lost =
                self.with_engine_mut(|engine| Ok(engine.end_pointer_capture(step.id.engine())))?;
            capture_ended = !lost.is_empty();
            self.dispatch_capture_changes(step, &lost)?;
        }
        let touch = step.pointer_type == PointerType::Touch;
        if kind == PointerInputKind::Up && !touch {
            if let (Some(target), Some(button), Some(pressed)) =
                (target, step.changed, update.pressed)
            {
                // Chromium sends no click once the pressed element has left
                // the document (`MouseEventManager::HandleRemoveSubtree`).
                if self.is_in_document(pressed)? {
                    // Which element `click` targets is I2's: a mouse release
                    // clicks the released node, or the pressed one when they
                    // match and it can be activated.
                    let click_target = update.activation.unwrap_or(target);
                    self.dispatch_release_clicks(step, button, click_target)?;
                }
            }
        }
        if kind == PointerInputKind::Up && !touch && capture_ended {
            let update = self.with_engine_mut(|engine| {
                engine.pointer_rehover(step.id.engine(), step.x, step.y)
            })?;
            self.dispatch_boundaries(step, &update, true, true)?;
        }
        if touch && ends {
            let leave =
                self.with_engine_mut(|engine| Ok(engine.pointer_leave(step.id.engine())))?;
            self.dispatch_boundaries(step, &leave, true, false)?;
            let (tapped, suppressed, count) = {
                let mut state = self
                    .inner
                    .pointers
                    .try_borrow_mut()
                    .map_err(|_| Error::ReentrantMutation)?;
                let pointer = state.pointers.remove(&step.id);
                let tapped = kind == PointerInputKind::Up && pointer.is_some_and(|p| !p.moved);
                // A touch that moved beyond the tap slop is no tap, and the
                // next tap starts a new series. No oracle scenario covers
                // this.
                let count = match pointer {
                    _ if !tapped => {
                        state.taps.reset();
                        0
                    }
                    _ if step.detail > 0 => step.detail,
                    Some(pointer) => {
                        let now = state.epoch.elapsed();
                        let down = (f64::from(pointer.down_at.0), f64::from(pointer.down_at.1));
                        state.taps.tap(down, pointer.down_time, now)
                    }
                    None => 1,
                };
                (
                    tapped,
                    pointer.is_some_and(|pointer| pointer.suppress_mouse),
                    count,
                )
            };
            self.with_engine_mut(|engine| {
                engine.forget_pointer(step.id.engine());
                Ok(())
            })?;
            if tapped {
                let tap = Step {
                    detail: count,
                    ..*step
                };
                // A tap whose pressed element left the document still clicks
                // what remains under the finger, as pinned Chromium 147 does.
                let activation = match update.pressed {
                    Some(pressed) if self.is_in_document(pressed)? => update.activation,
                    _ => target,
                };
                self.dispatch_tap(&tap, activation, suppressed)?;
            }
        }
        if ends && !touch {
            self.update_pointer(step.id, |pointer| pointer.suppress_mouse = false)?;
        }
        Ok(())
    }

    /// `click` for the main button, `auxclick` for another, and
    /// `contextmenu` after the secondary button's `auxclick`, as Windows
    /// Chromium fires them on release.
    fn dispatch_release_clicks(
        &self,
        step: &Step,
        button: MouseButton,
        target: NodeHandle,
    ) -> Result<(), Error> {
        if button == MouseButton::Main {
            let click = self.pointer_event(step, "click", target, Some(button), false)?;
            self.dispatch_click(target, &click)?;
            return self.dispatch_double_click(step, target);
        }
        let auxclick = self.pointer_event(step, "auxclick", target, Some(button), false)?;
        self.dispatch_to(target, &auxclick)?;
        if button == MouseButton::Secondary && !CONTEXTMENU_ON_PRESS {
            let menu = self.pointer_event(step, "contextmenu", target, Some(button), false)?;
            self.dispatch_to(target, &menu)?;
        }
        Ok(())
    }

    /// A tap's compatibility mouse events, sent through the mouse as
    /// Chromium's gesture tap does: boundary events, `mousemove`,
    /// `mousedown` (whose default focuses), `mouseup`, then `click` with the
    /// touch's id.
    fn dispatch_tap(
        &self,
        step: &Step,
        activation: Option<NodeHandle>,
        suppressed: bool,
    ) -> Result<(), Error> {
        let mouse = PointerId::MOUSE.engine();
        let mouse_step = Step {
            id: PointerId::MOUSE,
            pointer_type: PointerType::Mouse,
            is_primary: true,
            buttons: Buttons::NONE,
            changed: Some(MouseButton::Main),
            first_down: false,
            last_up: false,
            ..*step
        };
        if !suppressed {
            let update = self.with_engine_mut(|engine| {
                engine.pointer_event_with_buttons(mouse, PointerEventKind::Move, step.x, step.y, 0)
            })?;
            self.dispatch_boundaries(&mouse_step, &update, false, true)?;
            if let Some(target) = update.target {
                let event = self.mouse_event(&mouse_step, "mousemove", target, None)?;
                self.dispatch_to(target, &event)?;
            }
            let update = self.with_engine_mut(|engine| {
                engine.pointer_event_with_buttons(mouse, PointerEventKind::Down, step.x, step.y, 1)
            })?;
            if let Some(target) = update.target {
                let pressed = Step {
                    buttons: Buttons::PRIMARY,
                    ..mouse_step
                };
                let event =
                    self.mouse_event(&pressed, "mousedown", target, Some(MouseButton::Main))?;
                self.dispatch_to(target, &event)?;
                if !event.default_prevented() {
                    self.press_focus(target, step.x, step.y)?;
                }
            }
            let update = self.with_engine_mut(|engine| {
                engine.pointer_event_with_buttons(mouse, PointerEventKind::Up, step.x, step.y, 0)
            })?;
            if let Some(target) = update.target {
                let event =
                    self.mouse_event(&mouse_step, "mouseup", target, Some(MouseButton::Main))?;
                self.dispatch_to(target, &event)?;
            }
        }
        if let Some(target) = activation {
            let tap = Step {
                buttons: Buttons::NONE,
                ..*step
            };
            let click =
                self.pointer_event(&tap, "click", target, Some(MouseButton::Main), false)?;
            self.dispatch_click(target, &click)?;
            self.dispatch_double_click(&tap, target)?;
        }
        Ok(())
    }

    /// `dblclick` after a main-button `click` whose count is 2, at the
    /// click's target, as Chromium follows such a trusted click.
    fn dispatch_double_click(&self, step: &Step, target: NodeHandle) -> Result<(), Error> {
        if step.detail != 2 || !self.with_engine(|engine| engine.is_connected(target))?? {
            return Ok(());
        }
        let event = self.mouse_event(step, "dblclick", target, Some(MouseButton::Main))?;
        self.dispatch_to(target, &event)
    }

    /// The pointer left the window: `pointerout` and `pointerleave` along
    /// its hover path, then the mouse's, as Blink's
    /// `EventHandler::HandleMouseLeaveEvent` sends them. The press survives.
    fn dispatch_window_leave(&self, input: &PointerInput) -> Result<(), Error> {
        let (is_primary, buttons) = self
            .inner
            .pointers
            .try_borrow()
            .map_err(|_| Error::ReentrantMutation)?
            .pointers
            .get(&input.pointer_id)
            .map_or((true, Buttons::NONE), |pointer| {
                (pointer.is_primary, pointer.buttons)
            });
        let step = Step {
            id: input.pointer_id,
            pointer_type: input.pointer_type,
            is_primary,
            buttons,
            changed: None,
            first_down: false,
            last_up: false,
            x: input.x,
            y: input.y,
            modifiers: input.modifiers,
            pressure: input.pressure,
            detail: 0,
        };
        let step = self.count_clicks(input, step)?;
        let update =
            self.with_engine_mut(|engine| Ok(engine.pointer_leave(input.pointer_id.engine())))?;
        self.dispatch_boundaries(&step, &update, true, step.mouse_like())
    }

    fn dispatch_capture_changes(
        &self,
        step: &Step,
        changes: &[CaptureChange],
    ) -> Result<(), Error> {
        for change in changes {
            let (name, node) = match *change {
                CaptureChange::Lost(node) => ("lostpointercapture", node),
                CaptureChange::Got(node) => ("gotpointercapture", node),
            };
            let event = self.pointer_event(step, name, node, step.changed, step.is_primary)?;
            self.dispatch_to(node, &event)?;
        }
        Ok(())
    }

    /// `out`, `leave`, `over` and `enter` for the pointer and, when asked,
    /// for the mouse.
    fn dispatch_boundaries(
        &self,
        step: &Step,
        update: &PointerUpdate,
        pointer: bool,
        mouse: bool,
    ) -> Result<(), Error> {
        let changed = update.previous_target != update.target;
        for (prefix, enabled) in [("pointer", pointer), ("mouse", mouse)] {
            if !enabled {
                continue;
            }
            let event = |name: &str, node| -> Result<Event, Error> {
                let name = format!("{prefix}{name}");
                if prefix == "pointer" {
                    self.pointer_event(step, &name, node, step.changed, step.is_primary)
                } else {
                    self.mouse_event(step, &name, node, None)
                }
            };
            if changed {
                if let Some(previous) = update.previous_target {
                    if self.with_engine(|engine| engine.is_connected(previous))?? {
                        self.dispatch_to(previous, &event("out", previous)?)?;
                    }
                }
            }
            for &node in &update.left {
                let leave = event("leave", node)?;
                leave.set_phase(EventPhase::Target);
                self.invoke(node, node, &leave, None)?;
            }
            if changed {
                if let Some(target) = update.target {
                    self.dispatch_to(target, &event("over", target)?)?;
                }
            }
            for &node in &update.entered {
                let enter = event("enter", node)?;
                enter.set_phase(EventPhase::Target);
                self.invoke(node, node, &enter, None)?;
            }
        }
        Ok(())
    }

    fn mouse_info(
        &self,
        step: &Step,
        name: &str,
        node: NodeHandle,
        button: Option<MouseButton>,
    ) -> Result<MouseInfo, Error> {
        let (offset_x, offset_y) =
            self.with_engine_mut(|engine| engine.local_point(node, step.x, step.y))?;
        let client = Point::new(f64::from(step.x), f64::from(step.y));
        Ok(MouseInfo {
            button,
            buttons: step.buttons,
            client,
            offset: Point::new(f64::from(offset_x), f64::from(offset_y)),
            screen: client,
            modifiers: step.modifiers,
            detail: match name {
                "mousedown" | "mouseup" | "click" | "auxclick" | "dblclick" => step.detail,
                _ => 0,
            },
        })
    }

    fn pointer_event(
        &self,
        step: &Step,
        name: &str,
        node: NodeHandle,
        button: Option<MouseButton>,
        is_primary: bool,
    ) -> Result<Event, Error> {
        let mouse = self.mouse_info(step, name, node, button)?;
        Ok(Event::from_pointer_info(
            name,
            PointerInfo::new(mouse, step.id, step.pointer_type, is_primary, step.pressure),
        ))
    }

    /// A compatibility mouse event: its `button` is the changed button, or
    /// 0 when none changed.
    fn mouse_event(
        &self,
        step: &Step,
        name: &str,
        node: NodeHandle,
        button: Option<MouseButton>,
    ) -> Result<Event, Error> {
        let mouse = self.mouse_info(step, name, node, Some(button.unwrap_or(MouseButton::Main)))?;
        Ok(Event::from_mouse_info(name, step.id.engine(), mouse))
    }
}
