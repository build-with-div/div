//! Normalized framework input events.

use crate::element::{Element, WeakElement};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The native editing operation reported by beforeinput and input callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputType {
    InsertText,
    InsertLineBreak,
    DeleteContentBackward,
    DeleteContentForward,
    HistoryUndo,
    HistoryRedo,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteByCut,
    InsertFromPaste,
}

impl InputType {
    /// Name used by the pinned Chromium reference for the equivalent operation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InsertText => "insertText",
            Self::InsertLineBreak => "insertLineBreak",
            Self::DeleteContentBackward => "deleteContentBackward",
            Self::DeleteContentForward => "deleteContentForward",
            Self::HistoryUndo => "historyUndo",
            Self::HistoryRedo => "historyRedo",
            Self::DeleteWordBackward => "deleteWordBackward",
            Self::DeleteWordForward => "deleteWordForward",
            Self::DeleteByCut => "deleteByCut",
            Self::InsertFromPaste => "insertFromPaste",
        }
    }
}

/// Immutable, owned metadata for a native editing callback.
/// Clones retain the original intent and nullable data after dispatch returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEventInfo {
    input_type: InputType,
    data: Option<String>,
    is_composing: bool,
}

impl InputEventInfo {
    pub fn input_type(&self) -> InputType {
        self.input_type
    }

    /// Inserted text, or None for deletion, history and line-break operations.
    pub fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    pub fn is_composing(&self) -> bool {
        self.is_composing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

#[derive(Debug, Default)]
struct EventState {
    bubbles: bool,
    cancelable: bool,
    input_info: Option<InputEventInfo>,
    mouse_info: Option<crate::pointer::MouseInfo>,
    pointer_info: Option<crate::pointer::PointerInfo>,
    timestamp_ns: u64,
    default_prevented: Cell<bool>,
    propagation_stopped: Cell<bool>,
    immediate_propagation_stopped: Cell<bool>,
    /// Set while a passive listener runs, as the DOM's "in passive listener
    /// flag": `prevent_default` does nothing then.
    in_passive_listener: Cell<bool>,
    dispatching: Cell<bool>,
    completed_propagation_stopped: Cell<bool>,
    completed_immediate_propagation_stopped: Cell<bool>,
    phase: Cell<Option<EventPhase>>,
    last_dispatch_phase: Cell<Option<EventPhase>>,
    target: RefCell<Option<WeakElement>>,
    current_target: RefCell<Option<WeakElement>>,
    related_target: RefCell<Option<WeakElement>>,
}

impl EventState {
    fn for_type(event_type: &str) -> Rc<Self> {
        Rc::new(Self {
            bubbles: !matches!(
                event_type,
                "focus"
                    | "blur"
                    | "mouseenter"
                    | "mouseleave"
                    | "pointerenter"
                    | "pointerleave"
                    | "scroll"
            ),
            // Native input/change notifications report a completed mutation.
            // Chromium creates them as noncancelable; beforeinput remains
            // the application callback that may reject the pending edit.
            cancelable: !matches!(
                event_type,
                "focus"
                    | "blur"
                    | "focusin"
                    | "focusout"
                    | "input"
                    | "change"
                    | "select"
                    | "selectionchange"
                    | "scroll"
                    // Chromium 147.0.7727.50 PointerEventFactory::
                    // SetEventSpecificFields and MouseEventManager's native
                    // boundary event initializer. Explicit Event::new options
                    // are independent of these native source defaults.
                    | "pointerenter"
                    | "pointerleave"
                    | "pointercancel"
                    | "mouseenter"
                    | "mouseleave"
                    | "mousecancel"
                    | "gotpointercapture"
                    | "lostpointercapture"
            ),
            ..Self::default()
        })
    }
}

pub(crate) struct DispatchScope<'a> {
    event: &'a Event,
}

impl Drop for DispatchScope<'_> {
    fn drop(&mut self) {
        // Chromium's post-dispatch cleanup resets both stop flags so the same
        // owned event may be dispatched again. Cancellation remains set. The
        // C facade's existing result flags use completed-route snapshots.
        self.event
            .state
            .completed_propagation_stopped
            .set(self.event.state.propagation_stopped.replace(false));
        self.event
            .state
            .completed_immediate_propagation_stopped
            .set(
                self.event
                    .state
                    .immediate_propagation_stopped
                    .replace(false),
            );
        self.event.state.dispatching.set(false);
        *self.event.state.current_target.borrow_mut() = None;
        self.event.state.phase.set(None);
    }
}

pub(crate) struct ListenerScope<'a> {
    event: &'a Event,
}

impl Drop for ListenerScope<'_> {
    fn drop(&mut self) {
        *self.event.state.current_target.borrow_mut() = None;
        self.event.state.phase.set(None);
    }
}

/// Event value shared by capture, target, and bubble listeners.
#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: String,
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// A `wheel` event's deltas, as the DOM's `WheelEvent.deltaX` and
    /// `deltaY`: positive scrolls right or down, in [`Self::delta_mode`]'s
    /// unit. Zero for other events.
    pub delta_x: f32,
    pub delta_y: f32,
    /// The unit of [`Self::delta_x`] and [`Self::delta_y`], as the DOM's
    /// `WheelEvent.deltaMode`. Native wheels and touchpads report pixels.
    pub delta_mode: WheelDeltaMode,
    pub mouse_button: i32,
    pub key_code: i32,
    pub key_text: String,
    /// True for a `keydown` the platform repeats while the key is held, as
    /// the DOM's `KeyboardEvent.repeat`. False for every other event.
    pub repeat: bool,
    pub modifiers: i32,
    pub pointer_id: u64,
    pub is_composing: bool,
    state: Rc<EventState>,
}

impl Event {
    /// Create an application event with explicit propagation and cancellation
    /// options. Payload fields may be filled before native dispatch. This does
    /// not perform an edit or synthesize platform input.
    ///
    /// Use [`Self::native`] for the framework's native source defaults, or the
    /// normalized Document input methods for editing and platform defaults.
    pub fn new(event_type: impl Into<String>, bubbles: bool, cancelable: bool) -> Self {
        let mut event = Self::keyboard(event_type, 0, None, Modifiers::NONE);
        let state = Rc::get_mut(&mut event.state).expect("new event state");
        state.bubbles = bubbles;
        state.cancelable = cancelable;
        event
    }

    /// Create a raw event using the framework's normalized native source
    /// policy and an optional source timestamp (zero means unavailable).
    /// Native pointer enter/leave/cancel events are noncancelable; enter and
    /// leave do not bubble. Explicit [`Self::new`] options remain independent.
    /// Raw input events carry no typed editing intent; use Document's input
    /// operations when an edit and owned input metadata are required.
    pub fn native(event_type: impl Into<String>, timestamp_ns: u64) -> Self {
        let mut event = Self::keyboard(event_type, 0, None, Modifiers::NONE);
        event.is_composing = matches!(
            event.event_type.as_str(),
            "compositionstart" | "compositionupdate"
        );
        Rc::get_mut(&mut event.state)
            .expect("new event state")
            .timestamp_ns = timestamp_ns;
        event
    }

    pub(crate) fn pointer(
        event_type: impl Into<String>,
        pointer_id: u64,
        x: f32,
        y: f32,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> Self {
        let event_type = event_type.into();
        let state = EventState::for_type(&event_type);
        Self {
            event_type,
            mouse_x: x,
            mouse_y: y,
            delta_x: 0.0,
            delta_y: 0.0,
            delta_mode: WheelDeltaMode::Pixel,
            mouse_button: button as i32,
            key_code: 0,
            key_text: String::new(),
            repeat: false,
            modifiers: modifiers.bits() as i32,
            pointer_id,
            is_composing: false,
            state,
        }
    }

    pub(crate) fn keyboard(
        event_type: impl Into<String>,
        key_code: i32,
        key_text: Option<&str>,
        modifiers: Modifiers,
    ) -> Self {
        let event_type = event_type.into();
        let state = EventState::for_type(&event_type);
        Self {
            event_type,
            mouse_x: 0.0,
            mouse_y: 0.0,
            delta_x: 0.0,
            delta_y: 0.0,
            delta_mode: WheelDeltaMode::Pixel,
            mouse_button: -1,
            key_code,
            key_text: key_text.unwrap_or_default().to_owned(),
            repeat: false,
            modifiers: modifiers.bits() as i32,
            pointer_id: 0,
            is_composing: false,
            state,
        }
    }

    #[cfg(feature = "ffi-integration")]
    #[doc(hidden)]
    pub fn click_for_native_facade(timestamp_ns: u64) -> Self {
        let mut event = Self::pointer("click", 0, 0.0, 0.0, MouseButton::Main, Modifiers::NONE);
        Rc::get_mut(&mut event.state)
            .expect("new event state")
            .timestamp_ns = timestamp_ns;
        event
    }

    /// The key of a `keydown`, `keypress` or `keyup` event, as the DOM's
    /// `KeyboardEvent.key`: named keys from the key code, so native and
    /// headless input agree; `Space` for the space bar; `Char` for a key that
    /// produces one character, in the case it was typed. `None` for other
    /// event types and for keys Div does not name.
    ///
    /// A `keypress` carries the character's code in [`Self::key_code`], as
    /// the DOM's `charCode`, so its key comes from its text.
    pub fn key(&self) -> Option<crate::Key> {
        match self.event_type.as_str() {
            "keydown" | "keyup" => crate::Key::from_code_and_text(self.key_code, &self.key_text),
            "keypress" => match self.key_text.as_str() {
                "Enter" => Some(crate::Key::Enter),
                "Delete" => Some(crate::Key::Delete),
                text => crate::Key::from_code_and_text(0, text),
            },
            _ => None,
        }
    }

    /// Timestamp supplied by the native event source, or zero when unavailable.
    pub fn timestamp_ns(&self) -> u64 {
        self.state.timestamp_ns
    }

    pub(crate) fn input(
        event_type: &str,
        key_code: i32,
        key_text: Option<&str>,
        modifiers: Modifiers,
        input_type: InputType,
        data: Option<&str>,
        is_composing: bool,
    ) -> Self {
        let mut event = Self::keyboard(event_type, key_code, key_text, modifiers);
        event.is_composing = is_composing;
        Rc::get_mut(&mut event.state)
            .expect("new event state is uniquely owned")
            .input_info = Some(InputEventInfo {
            input_type,
            data: data.map(str::to_owned),
            is_composing,
        });
        event
    }

    /// Immutable native editing intent and nullable data.
    /// Returns None for events without a typed editing operation. The owned
    /// metadata remains unchanged in saved clones after dispatch or teardown.
    /// The web's `MouseEvent` data of a mouse, pointer, `click`,
    /// `auxclick` or `contextmenu` event from a pointing device.
    pub fn mouse_info(&self) -> Option<&crate::pointer::MouseInfo> {
        self.state
            .pointer_info
            .as_ref()
            .map(|info| &info.mouse)
            .or(self.state.mouse_info.as_ref())
    }

    /// The web's `PointerEvent` data of a pointer, `click`, `auxclick` or
    /// `contextmenu` event from a pointing device.
    pub fn pointer_info(&self) -> Option<&crate::pointer::PointerInfo> {
        self.state.pointer_info.as_ref()
    }

    /// A native pointer event with its typed data. The flat fields repeat
    /// it: `mouse_button` is -1 for no button and `pointer_id` is the id.
    pub(crate) fn from_pointer_info(
        event_type: impl Into<String>,
        info: crate::pointer::PointerInfo,
    ) -> Self {
        let mouse = &info.mouse;
        let mut event = Self::pointer(
            event_type,
            info.pointer_id.0 as u64,
            mouse.client.x as f32,
            mouse.client.y as f32,
            MouseButton::Main,
            mouse.modifiers,
        );
        event.mouse_button = mouse.button.map_or(-1, |button| button as i32);
        Rc::get_mut(&mut event.state)
            .expect("new event state")
            .pointer_info = Some(info);
        event
    }

    /// A native compatibility mouse event with its typed data. Its flat
    /// `pointer_id` names the pointer that caused it.
    pub(crate) fn from_mouse_info(
        event_type: impl Into<String>,
        pointer_id: u64,
        info: crate::pointer::MouseInfo,
    ) -> Self {
        let mut event = Self::pointer(
            event_type,
            pointer_id,
            info.client.x as f32,
            info.client.y as f32,
            MouseButton::Main,
            info.modifiers,
        );
        event.mouse_button = info.button.map_or(0, |button| button as i32);
        Rc::get_mut(&mut event.state)
            .expect("new event state")
            .mouse_info = Some(info);
        event
    }

    pub fn input_info(&self) -> Option<&InputEventInfo> {
        self.state.input_info.as_ref()
    }

    pub(crate) fn wheel(
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        mode: WheelDeltaMode,
        modifiers: Modifiers,
    ) -> Self {
        // Chromium 147 reports `button` 0 for a wheel event, as for every
        // mouse event without a changed button.
        let mut event = Self::pointer("wheel", 0, x, y, MouseButton::Main, modifiers);
        event.delta_x = dx;
        event.delta_y = dy;
        event.delta_mode = mode;
        event
    }

    pub(crate) fn focus(event_type: &str, related_target: Option<WeakElement>) -> Self {
        let event = Self::keyboard(event_type, 0, None, Modifiers::NONE);
        *event.state.related_target.borrow_mut() = related_target;
        event
    }

    pub(crate) fn set_phase(&self, phase: EventPhase) {
        self.state.phase.set(Some(phase));
        self.state.last_dispatch_phase.set(Some(phase));
    }

    pub(crate) fn is_dispatching(&self) -> bool {
        self.state.dispatching.get()
    }

    pub(crate) fn set_dispatch_target(&self, target: WeakElement) {
        *self.state.target.borrow_mut() = Some(target);
    }

    pub(crate) fn dispatch_scope(&self) -> Result<DispatchScope<'_>, crate::Error> {
        if self.state.dispatching.replace(true) {
            return Err(crate::Error::ReentrantMutation);
        }
        self.state.last_dispatch_phase.set(None);
        self.state.completed_propagation_stopped.set(false);
        self.state
            .completed_immediate_propagation_stopped
            .set(false);
        Ok(DispatchScope { event: self })
    }

    /// Preserve the legacy facade's completed-route phase without retaining an
    /// active listener or duplicating the native route. Rust phase() is None
    /// once callbacks return.
    #[cfg(feature = "ffi-integration")]
    #[doc(hidden)]
    pub fn last_dispatch_phase_for_native_facade(&self) -> Option<EventPhase> {
        self.state.last_dispatch_phase.get()
    }

    #[cfg(feature = "ffi-integration")]
    #[doc(hidden)]
    pub fn completed_propagation_stopped_for_native_facade(&self) -> bool {
        self.state.completed_propagation_stopped.get()
    }

    #[cfg(feature = "ffi-integration")]
    #[doc(hidden)]
    pub fn completed_immediate_propagation_stopped_for_native_facade(&self) -> bool {
        self.state.completed_immediate_propagation_stopped.get()
    }

    pub(crate) fn listener_scope(
        &self,
        target: WeakElement,
        current_target: WeakElement,
    ) -> ListenerScope<'_> {
        *self.state.target.borrow_mut() = Some(target);
        *self.state.current_target.borrow_mut() = Some(current_target);
        ListenerScope { event: self }
    }

    /// The current dispatch phase, or `None` after callbacks have returned.
    pub fn phase(&self) -> Option<EventPhase> {
        self.state.phase.get()
    }

    /// The element that received the event, including a pointer-capture target.
    ///
    /// The event retains a weak, generation-checked handle. A saved event can
    /// still resolve its target while that element and document are live, but
    /// does not keep either alive. Returns `None` after they are destroyed.
    pub fn target(&self) -> Option<Element> {
        self.state.target.borrow().as_ref()?.upgrade()
    }

    /// The element whose listener is currently running.
    ///
    /// This follows the shared capture/target/bubble route and is cleared
    /// when callbacks return, including early returns and panic unwinding.
    /// Saved clones observe the same current listener while dispatch runs.
    pub fn current_target(&self) -> Option<Element> {
        self.state.current_target.borrow().as_ref()?.upgrade()
    }

    /// The other element in a native focus transfer.
    ///
    /// For `blur` and `focusout`, this is the pending focus destination; for
    /// `focus` and `focusin`, it is the previous focused element. Initial
    /// focus, clearing focus, and a transfer redirected by a blur callback
    /// can have no related element. Other event types return `None`.
    ///
    /// Like [`Self::target`], this uses a weak, generation-checked handle.
    /// Saved events do not retain the element or document and return `None`
    /// after either is destroyed.
    pub fn related_target(&self) -> Option<Element> {
        self.state.related_target.borrow().as_ref()?.upgrade()
    }

    #[cfg(feature = "ffi-integration")]
    #[doc(hidden)]
    pub fn related_node_for_native_facade(&self) -> Option<div_engine::NodeHandle> {
        self.related_target().map(|element| element.handle)
    }

    /// Whether this normalized event visits ancestor bubble listeners.
    pub fn bubbles(&self) -> bool {
        self.state.bubbles
    }

    /// Whether [`Self::prevent_default`] can cancel this event's default action.
    /// Native focus, blur, focusin and focusout notifications are noncancelable.
    pub fn cancelable(&self) -> bool {
        self.state.cancelable
    }

    /// Cancel the event's default action, if it is cancelable. Inside a
    /// passive listener this does nothing, as on the web.
    pub fn prevent_default(&self) {
        if self.cancelable() && !self.state.in_passive_listener.get() {
            self.state.default_prevented.set(true);
        }
    }

    pub(crate) fn set_in_passive_listener(&self, passive: bool) {
        self.state.in_passive_listener.set(passive);
    }

    pub fn default_prevented(&self) -> bool {
        self.state.default_prevented.get()
    }

    pub fn stop_propagation(&self) {
        self.state.propagation_stopped.set(true);
    }

    pub fn stop_immediate_propagation(&self) {
        self.state.propagation_stopped.set(true);
        self.state.immediate_propagation_stopped.set(true);
    }

    pub fn propagation_stopped(&self) -> bool {
        self.state.propagation_stopped.get()
    }

    pub fn immediate_propagation_stopped(&self) -> bool {
        self.state.immediate_propagation_stopped.get()
    }

    pub(crate) fn composition(event_type: &str, text: &str) -> Self {
        let mut event = Self::keyboard(event_type, 0, Some(text), Modifiers::NONE);
        event.is_composing = event_type != "compositionend";
        event
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventType {
    Down,
    Up,
    Move,
}

/// The unit of a wheel event's deltas, as the DOM's `WheelEvent.deltaMode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WheelDeltaMode {
    /// `DOM_DELTA_PIXEL` (0): CSS pixels.
    #[default]
    Pixel = 0,
    /// `DOM_DELTA_LINE` (1): lines.
    Line = 1,
    /// `DOM_DELTA_PAGE` (2): pages.
    Page = 2,
}

/// A mouse or pen button, numbered as the web's `MouseEvent.button`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum MouseButton {
    /// Usually the left button.
    Main = 0,
    /// Usually the wheel or middle button.
    Auxiliary = 1,
    /// Usually the right button.
    Secondary = 2,
    Back = 3,
    Forward = 4,
    /// A pen's eraser.
    Eraser = 5,
}

#[allow(non_upper_case_globals)]
impl MouseButton {
    #[deprecated(note = "use `MouseButton::Main`, the web's name")]
    pub const Left: Self = Self::Main;
    #[deprecated(note = "use `MouseButton::Auxiliary`, the web's name")]
    pub const Middle: Self = Self::Auxiliary;
    #[deprecated(note = "use `MouseButton::Secondary`, the web's name")]
    pub const Right: Self = Self::Secondary;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventType {
    Down,
    Up,
    Char,
}

impl KeyEventType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Down => "keydown",
            Self::Up => "keyup",
            Self::Char => "input",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers(pub u32);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(2);
    pub const ALT: Self = Self(4);
    pub const META: Self = Self(8);

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

pub(crate) type EventCallback = Rc<dyn Fn(&Event)>;

#[derive(Clone)]
pub(crate) struct Listener {
    pub options: ListenerOptions,
    pub callback: EventCallback,
}

/// How [`Element::on_with`] adds a listener, as the DOM's
/// `AddEventListenerOptions`.
///
/// [`Element::on_with`]: crate::Element::on_with
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListenerOptions {
    /// Listen in the capture phase, as [`Element::on_capture`] does.
    ///
    /// [`Element::on_capture`]: crate::Element::on_capture
    pub capture: bool,
    /// Remove the listener before it is first called.
    pub once: bool,
    /// Make [`Event::prevent_default`] do nothing inside the listener.
    ///
    /// `None` is the web's default, which Chromium 147 resolves in
    /// `EventTarget::SetDefaultAddEventListenerOptions`: passive for
    /// `touchstart`, `touchmove`, `wheel` and `mousewheel` on a top-level
    /// node (the window, the document, its element or its body, which a
    /// native document's root stands for), and not passive otherwise.
    pub passive: Option<bool>,
}

impl ListenerOptions {
    /// The passive flag the listener gets on `top_level` or another node.
    pub(crate) fn resolved_passive(self, event_type: &str, top_level: bool) -> bool {
        self.passive.unwrap_or(
            top_level
                && matches!(
                    event_type,
                    "touchstart" | "touchmove" | "wheel" | "mousewheel"
                ),
        )
    }
}

/// One listener added by [`Element::on`], [`Element::on_capture`] or
/// [`Element::on_with`], as the callback and capture flag identify a
/// listener for the DOM's `removeEventListener`.
///
/// It is a handle: dropping it leaves the listener in place, as forgetting a
/// callback does on the web. [`Self::remove`] removes exactly this listener.
///
/// [`Element::on`]: crate::Element::on
/// [`Element::on_capture`]: crate::Element::on_capture
/// [`Element::on_with`]: crate::Element::on_with
#[derive(Debug, Clone)]
pub struct ListenerHandle {
    pub(crate) document: std::rc::Weak<crate::document::DocumentInner>,
    pub(crate) id: u64,
}

impl ListenerHandle {
    /// Remove this listener. If it is removed during a dispatch, it is not
    /// called for the rest of it. Removing it again, after its element was
    /// removed, or after the document is gone does nothing.
    pub fn remove(&self) -> Result<(), crate::Error> {
        let Some(inner) = self.document.upgrade() else {
            return Ok(());
        };
        crate::Document { inner }.remove_listener(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_control_state_is_shared_by_clones() {
        let event = Event::pointer("click", 0, 1.0, 2.0, MouseButton::Main, Modifiers::CTRL);
        let clone = event.clone();
        clone.prevent_default();
        assert!(event.default_prevented());
        assert_eq!(event.modifiers, Modifiers::CTRL.bits() as i32);
    }
}
