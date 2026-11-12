//! Key names for keyboard events.
//!
//! The DOM gives listeners `KeyboardEvent.key` (`"ArrowRight"`, `"Home"`,
//! `" "`); Div's events carry the platform key code and the key's text
//! instead. This table names the keys the primitives handle from the key
//! code, falling back to the key text. It is RFC 0003's interim for request
//! 2 (key names on keyboard events) and goes away once the framework adds
//! them.

use div::prelude::{Event, Modifiers};

/// A named key, as in `KeyboardEvent.key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
    Escape,
    /// The space bar (`" "`).
    Space,
    PageUp,
    PageDown,
    End,
    Home,
    ArrowLeft,
    ArrowUp,
    ArrowRight,
    ArrowDown,
}

impl Key {
    /// The key of a `keydown` or `keyup` event, if it is one of these.
    pub fn of(event: &Event) -> Option<Self> {
        Self::from_code(event.key_code).or_else(|| Self::from_text(&event.key_text))
    }

    /// The key with this platform key code (the virtual key codes Div's
    /// platform layer and `divui::testing` report).
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            9 => Self::Tab,
            13 => Self::Enter,
            27 => Self::Escape,
            32 => Self::Space,
            33 => Self::PageUp,
            34 => Self::PageDown,
            35 => Self::End,
            36 => Self::Home,
            37 => Self::ArrowLeft,
            38 => Self::ArrowUp,
            39 => Self::ArrowRight,
            40 => Self::ArrowDown,
            _ => return None,
        })
    }

    /// The key with this key text: a DOM key name, or the text the key
    /// types (`"\t"`, `"\r"`, `" "`).
    pub fn from_text(text: &str) -> Option<Self> {
        Some(match text {
            "Tab" | "\t" => Self::Tab,
            "Enter" | "\r" | "\n" => Self::Enter,
            "Escape" | "Esc" => Self::Escape,
            " " | "Spacebar" => Self::Space,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            "End" => Self::End,
            "Home" => Self::Home,
            "ArrowLeft" | "Left" => Self::ArrowLeft,
            "ArrowUp" | "Up" => Self::ArrowUp,
            "ArrowRight" | "Right" => Self::ArrowRight,
            "ArrowDown" | "Down" => Self::ArrowDown,
            _ => return None,
        })
    }

    /// The DOM `KeyboardEvent.key` value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tab => "Tab",
            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Space => " ",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::End => "End",
            Self::Home => "Home",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowRight => "ArrowRight",
            Self::ArrowDown => "ArrowDown",
        }
    }

    /// The key's platform key code, as [rom_code](Self::from_code) reads it.
    pub fn code(self) -> i32 {
        match self {
            Self::Tab => 9,
            Self::Enter => 13,
            Self::Escape => 27,
            Self::Space => 32,
            Self::PageUp => 33,
            Self::PageDown => 34,
            Self::End => 35,
            Self::Home => 36,
            Self::ArrowLeft => 37,
            Self::ArrowUp => 38,
            Self::ArrowRight => 39,
            Self::ArrowDown => 40,
        }
    }

    /// The text the key types, as Div's platform layer reports it, if any.
    pub fn text(self) -> Option<&'static str> {
        match self {
            Self::Tab => Some("\t"),
            Self::Enter => Some("\r"),
            Self::Space => Some(" "),
            _ => None,
        }
    }
}

/// The modifier keys held during an event (`shiftKey`, `ctrlKey`,
/// `altKey`, `metaKey`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Held {
    pub fn of(event: &Event) -> Self {
        let bits = event.modifiers as u32;
        let has = |modifier: Modifiers| bits & modifier.bits() != 0;
        Self {
            shift: has(Modifiers::SHIFT),
            ctrl: has(Modifiers::CTRL),
            alt: has(Modifiers::ALT),
            meta: has(Modifiers::META),
        }
    }

    /// Whether any modifier is held.
    pub fn any(self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_texts_name_the_same_keys() {
        for key in [
            Key::Tab,
            Key::Enter,
            Key::Escape,
            Key::Space,
            Key::PageUp,
            Key::PageDown,
            Key::End,
            Key::Home,
            Key::ArrowLeft,
            Key::ArrowUp,
            Key::ArrowRight,
            Key::ArrowDown,
        ] {
            assert_eq!(Key::from_text(key.name()), Some(key), "{key:?}");
            assert_eq!(Key::from_code(key.code()), Some(key), "{key:?}");
            if let Some(text) = key.text() {
                assert_eq!(Key::from_text(text), Some(key), "{key:?}");
            }
        }
        assert_eq!(Key::from_code(39), Some(Key::ArrowRight));
        assert_eq!(Key::from_code(65), None);
        assert_eq!(Key::from_text("\r"), Some(Key::Enter));
        assert_eq!(Key::from_text("a"), None);
    }
}
