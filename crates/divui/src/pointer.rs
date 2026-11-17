//! Pointer Events types (RFC 0007 section 4): the web's `PointerEvent` and
//! `MouseEvent` data, read through [`crate::Event::pointer_info`] and
//! [`crate::Event::mouse_info`].

use crate::events::{Modifiers, MouseButton};

/// The device behind a pointer, as the web's `PointerEvent.pointerType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerType {
    Mouse,
    Pen,
    Touch,
}

impl PointerType {
    /// The web's name: `"mouse"`, `"pen"` or `"touch"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Pen => "pen",
            Self::Touch => "touch",
        }
    }
}

/// A pointer's id, as the web's `PointerEvent.pointerId`. The mouse is 1,
/// as in Chromium; each touch or pen contact gets an id above 1 that no
/// active pointer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointerId(pub i32);

impl PointerId {
    pub const MOUSE: Self = Self(1);

    pub(crate) fn engine(self) -> u64 {
        self.0 as u64
    }
}

/// The pressed buttons, as the web's `MouseEvent.buttons` bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Buttons(pub u16);

impl Buttons {
    pub const NONE: Self = Self(0);
    pub const PRIMARY: Self = Self(1);
    pub const SECONDARY: Self = Self(2);
    pub const AUXILIARY: Self = Self(4);
    pub const BACK: Self = Self(8);
    pub const FORWARD: Self = Self(16);
    pub const ERASER: Self = Self(32);

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The bit of `button`: the web numbers `button` and `buttons`
    /// differently for the auxiliary and secondary buttons.
    pub fn of(button: MouseButton) -> Self {
        match button {
            MouseButton::Main => Self::PRIMARY,
            MouseButton::Auxiliary => Self::AUXILIARY,
            MouseButton::Secondary => Self::SECONDARY,
            MouseButton::Back => Self::BACK,
            MouseButton::Forward => Self::FORWARD,
            MouseButton::Eraser => Self::ERASER,
        }
    }

    pub(crate) fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl std::ops::BitOr for Buttons {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.with(other)
    }
}

/// A point in CSS pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The web's `MouseEvent` data.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseInfo {
    /// The button that changed, or `None` for the web's -1, which pointer
    /// moves and pointer boundary events report.
    pub button: Option<MouseButton>,
    pub buttons: Buttons,
    /// The viewport point.
    pub client: Point,
    /// The point in the target's padding box, through its transforms.
    pub offset: Point,
    /// The point on the screen: the client point plus the window's
    /// position, which is zero headless.
    pub screen: Point,
    pub modifiers: Modifiers,
}

/// The web's `PointerEvent` data.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerInfo {
    pub mouse: MouseInfo,
    pub pointer_id: PointerId,
    pub pointer_type: PointerType,
    pub is_primary: bool,
    pub width: f64,
    pub height: f64,
    pub pressure: f32,
    pub tangential_pressure: f32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    pub twist: i32,
    pub altitude_angle: f64,
    pub azimuth_angle: f64,
}

impl PointerInfo {
    /// The web's defaults for a device that reports only position and
    /// buttons: pressure 0.5 while a button is pressed and 0 otherwise, a
    /// 1x1 contact, no tilt or twist, and the angles of a pen held upright.
    pub(crate) fn new(
        mouse: MouseInfo,
        pointer_id: PointerId,
        pointer_type: PointerType,
        is_primary: bool,
        pressure: Option<f32>,
    ) -> Self {
        let pressed = !mouse.buttons.is_empty();
        Self {
            mouse,
            pointer_id,
            pointer_type,
            is_primary,
            width: 1.0,
            height: 1.0,
            pressure: if pressed {
                pressure.unwrap_or(0.5)
            } else {
                0.0
            },
            tangential_pressure: 0.0,
            tilt_x: 0,
            tilt_y: 0,
            twist: 0,
            altitude_angle: std::f64::consts::FRAC_PI_2,
            azimuth_angle: 0.0,
        }
    }
}

/// One input from a pointing device, for [`crate::Document::dispatch_pointer_input`].
#[derive(Debug, Clone, PartialEq)]
pub struct PointerInput {
    pub pointer_id: PointerId,
    pub pointer_type: PointerType,
    pub kind: PointerInputKind,
    /// The viewport point.
    pub x: f32,
    pub y: f32,
    /// The button pressed or released by a `Down` or `Up`. Ignored for other
    /// kinds.
    pub button: MouseButton,
    pub modifiers: Modifiers,
    /// The device's pressure, from 0 to 1, when it reports one.
    pub pressure: Option<f32>,
}

impl PointerInput {
    /// A mouse input with the primary button and no modifiers.
    pub fn mouse(kind: PointerInputKind, x: f32, y: f32) -> Self {
        Self {
            pointer_id: PointerId::MOUSE,
            pointer_type: PointerType::Mouse,
            kind,
            x,
            y,
            button: MouseButton::Main,
            modifiers: Modifiers::NONE,
            pressure: None,
        }
    }

    /// A touch contact's input.
    pub fn touch(pointer_id: PointerId, kind: PointerInputKind, x: f32, y: f32) -> Self {
        Self {
            pointer_id,
            pointer_type: PointerType::Touch,
            ..Self::mouse(kind, x, y)
        }
    }

    pub fn with_button(mut self, button: MouseButton) -> Self {
        self.button = button;
        self
    }

    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

/// What a [`PointerInput`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerInputKind {
    Move,
    /// A button is pressed, or a touch or pen contact starts.
    Down,
    /// A button is released, or a touch or pen contact ends.
    Up,
    /// The platform cancelled the pointer.
    Cancel,
    /// The pointer left the window.
    Leave,
}
