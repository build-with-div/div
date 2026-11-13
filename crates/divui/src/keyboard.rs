//! Keys, as named by keyboard events, menu accelerators and test input.

/// A key on the keyboard, as the DOM's `KeyboardEvent.key` names it.
///
/// [`Event::key`](crate::Event::key) reports it for `keydown` and `keyup`,
/// [`Document::dispatch_key`](crate::Document::dispatch_key) sends it, and
/// menu accelerators match it. Letters are `Key::A` to `Key::Z`, or
/// `Key::Char` for any character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Key {
    /// A key that produces a character. An event reports it in the case it
    /// was typed (`'a'` or `'A'`); an accelerator matches letters in either
    /// case.
    Char(char),
    Enter,
    Escape,
    Tab,
    /// The space bar. An event never reports it as `Char(' ')`.
    Space,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// A function key, `Function(1)` to `Function(24)`.
    Function(u8),
}

macro_rules! letter_keys {
    ($($letter:ident)*) => {
        impl Key {
            $(
                #[doc = concat!("The ", stringify!($letter), " key.")]
                pub const $letter: Key = Key::Char(stringify!($letter).as_bytes()[0] as char);
            )*
        }
    };
}

letter_keys!(A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);

impl Key {
    /// The platform key code this key arrives with, if it has one.
    pub(crate) fn code(self) -> Option<i32> {
        Some(match self {
            Self::Char(character) if character.is_ascii_alphanumeric() => {
                character.to_ascii_uppercase() as i32
            }
            Self::Char(_) => return None,
            Self::Enter => 13,
            Self::Escape => 27,
            Self::Tab => 9,
            Self::Space => 32,
            Self::Backspace => 8,
            Self::Delete => 46,
            Self::Home => 36,
            Self::End => 35,
            Self::PageUp => 33,
            Self::PageDown => 34,
            Self::ArrowLeft => 37,
            Self::ArrowUp => 38,
            Self::ArrowRight => 39,
            Self::ArrowDown => 40,
            Self::Function(number @ 1..=24) => 111 + i32::from(number),
            Self::Function(_) => return None,
        })
    }

    /// The key a keyboard event's code and key text name. Named keys come
    /// from the code, so native input (whose text is the DOM name, such as
    /// "ArrowLeft") and headless input (often without text) agree.
    pub(crate) fn from_code_and_text(key_code: i32, key_text: &str) -> Option<Self> {
        let named = match key_code {
            8 => Some(Self::Backspace),
            9 => Some(Self::Tab),
            13 => Some(Self::Enter),
            27 => Some(Self::Escape),
            32 => Some(Self::Space),
            33 => Some(Self::PageUp),
            34 => Some(Self::PageDown),
            35 => Some(Self::End),
            36 => Some(Self::Home),
            37 => Some(Self::ArrowLeft),
            38 => Some(Self::ArrowUp),
            39 => Some(Self::ArrowRight),
            40 => Some(Self::ArrowDown),
            46 => Some(Self::Delete),
            112..=135 => Some(Self::Function((key_code - 111) as u8)),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
        let mut characters = key_text.chars();
        match (characters.next(), characters.next()) {
            (Some(' '), None) => return Some(Self::Space),
            (Some(character), None) if !character.is_control() => {
                return Some(Self::Char(character))
            }
            _ => {}
        }
        // A letter or digit key without text, as the code names it.
        u8::try_from(key_code)
            .ok()
            .map(char::from)
            .filter(|character| character.is_ascii_digit() || character.is_ascii_uppercase())
            .map(Self::Char)
    }

    /// The DOM `key` string an event for this key carries: the character for
    /// `Char`, `" "` for `Space`, and the DOM name otherwise.
    pub(crate) fn dom_text(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Space => " ".into(),
            Self::Enter => "Enter".into(),
            Self::Escape => "Escape".into(),
            Self::Tab => "Tab".into(),
            Self::Backspace => "Backspace".into(),
            Self::Delete => "Delete".into(),
            Self::Home => "Home".into(),
            Self::End => "End".into(),
            Self::PageUp => "PageUp".into(),
            Self::PageDown => "PageDown".into(),
            Self::ArrowUp => "ArrowUp".into(),
            Self::ArrowDown => "ArrowDown".into(),
            Self::ArrowLeft => "ArrowLeft".into(),
            Self::ArrowRight => "ArrowRight".into(),
            Self::Function(number) => format!("F{number}"),
        }
    }

    /// The text this key types on keydown, if any.
    pub(crate) fn typed_text(self) -> Option<String> {
        match self {
            Self::Char(character) if !character.is_control() => Some(character.to_string()),
            Self::Space => Some(" ".into()),
            _ => None,
        }
    }

    /// Whether a key event with this code and text is this key; letters
    /// match either case.
    #[cfg_attr(not(feature = "menu"), allow(dead_code))]
    pub(crate) fn matches(self, key_code: i32, text: Option<&str>) -> bool {
        match (self.code(), self) {
            (Some(code), _) => code == key_code,
            (None, Self::Char(character)) => {
                text.is_some_and(|text| text.chars().eq(std::iter::once(character)))
            }
            (None, _) => false,
        }
    }

    /// How a menu shows the key, such as `Esc` or `PgUp`.
    #[cfg_attr(not(feature = "menu"), allow(dead_code))]
    pub(crate) fn accelerator_name(self) -> String {
        match self {
            Self::Char(' ') | Self::Space => "Space".into(),
            Self::Char(character) => character.to_uppercase().collect(),
            Self::Enter => "Enter".into(),
            Self::Escape => "Esc".into(),
            Self::Tab => "Tab".into(),
            Self::Backspace => "Backspace".into(),
            Self::Delete => "Del".into(),
            Self::Home => "Home".into(),
            Self::End => "End".into(),
            Self::PageUp => "PgUp".into(),
            Self::PageDown => "PgDn".into(),
            Self::ArrowUp => "Up".into(),
            Self::ArrowDown => "Down".into(),
            Self::ArrowLeft => "Left".into(),
            Self::ArrowRight => "Right".into(),
            Self::Function(number) => format!("F{number}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn named_keys_come_from_the_code_whatever_the_text() {
        for (code, key) in [
            (13, Key::Enter),
            (27, Key::Escape),
            (9, Key::Tab),
            (32, Key::Space),
            (36, Key::Home),
            (35, Key::End),
            (33, Key::PageUp),
            (34, Key::PageDown),
            (37, Key::ArrowLeft),
            (38, Key::ArrowUp),
            (39, Key::ArrowRight),
            (40, Key::ArrowDown),
            (8, Key::Backspace),
            (46, Key::Delete),
            (112, Key::Function(1)),
            (135, Key::Function(24)),
        ] {
            // Native input carries the DOM name; headless input may carry none.
            assert_eq!(Key::from_code_and_text(code, &key.dom_text()), Some(key));
            assert_eq!(Key::from_code_and_text(code, ""), Some(key));
            assert_eq!(key.code(), Some(code));
        }
    }

    #[test]
    fn characters_keep_their_case_and_letters_fall_back_to_the_code() {
        assert_eq!(Key::from_code_and_text(65, "a"), Some(Key::Char('a')));
        assert_eq!(Key::from_code_and_text(65, "A"), Some(Key::A));
        assert_eq!(Key::from_code_and_text(0, "é"), Some(Key::Char('é')));
        assert_eq!(Key::from_code_and_text(0, ","), Some(Key::Char(',')));
        assert_eq!(Key::from_code_and_text(65, ""), Some(Key::A));
        assert_eq!(Key::from_code_and_text(55, ""), Some(Key::Char('7')));
        assert_eq!(Key::from_code_and_text(0, " "), Some(Key::Space));
        assert_eq!(Key::from_code_and_text(0, ""), None);
        assert_eq!(Key::from_code_and_text(0, "Unidentified"), None);
        assert_eq!(Key::from_code_and_text(0, "\u{7f}"), None);
    }

    /// A focused input inside the body, and every key event the body sees as
    /// (type, key, repeat, key_text).
    fn focused_input() -> (
        crate::Document,
        crate::Element,
        std::rc::Rc<std::cell::RefCell<Vec<(String, Option<Key>, bool, String)>>>,
    ) {
        let document = crate::Document::new(64, 64).unwrap();
        let input = crate::Element::create(&document, "input").unwrap();
        document.body().append_child(&input).unwrap();
        input.focus().unwrap();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        for event_type in ["keydown", "keyup", "click"] {
            let seen = seen.clone();
            document
                .body()
                .on(event_type, move |event| {
                    seen.borrow_mut().push((
                        event.event_type.clone(),
                        event.key(),
                        event.repeat,
                        event.key_text.clone(),
                    ));
                })
                .unwrap();
        }
        (document, input, seen)
    }

    #[test]
    fn dispatched_keys_carry_their_name_and_repeat() {
        use crate::{KeyEventType, Modifiers};
        let (document, _input, seen) = focused_input();
        document
            .dispatch_key(KeyEventType::Down, Key::ArrowRight, Modifiers::NONE, true)
            .unwrap();
        document
            .dispatch_key(KeyEventType::Up, Key::ArrowRight, Modifiers::NONE, true)
            .unwrap();
        assert_eq!(
            *seen.borrow(),
            [
                (
                    "keydown".into(),
                    Some(Key::ArrowRight),
                    true,
                    "ArrowRight".into()
                ),
                // A keyup is never a repeat.
                (
                    "keyup".into(),
                    Some(Key::ArrowRight),
                    false,
                    "ArrowRight".into()
                ),
            ]
        );
    }

    #[test]
    fn key_input_without_names_still_reports_the_key() {
        use crate::{KeyEventType, Modifiers};
        let (document, input, seen) = focused_input();
        // As TestApp sent named keys before dispatch_key: a code, no text.
        document
            .dispatch_key_input(KeyEventType::Down, 27, None, None, Modifiers::NONE)
            .unwrap();
        // As the native adapter sends them: the DOM name as key text.
        document
            .dispatch_key_input(KeyEventType::Down, 36, Some("Home"), None, Modifiers::NONE)
            .unwrap();
        input.dispatch_click_event().unwrap();
        let keys: Vec<_> = seen.borrow().iter().map(|entry| entry.1).collect();
        assert_eq!(keys, [Some(Key::Escape), Some(Key::Home), None]);
        assert!(seen.borrow().iter().all(|entry| !entry.2));
    }

    #[test]
    fn dispatched_characters_and_space_type_into_the_focused_control() {
        use crate::{KeyEventType, Modifiers};
        let (document, input, seen) = focused_input();
        for key in [Key::Char('a'), Key::Char(' '), Key::B] {
            document
                .dispatch_key(KeyEventType::Down, key, Modifiers::NONE, false)
                .unwrap();
            document
                .dispatch_key(KeyEventType::Up, key, Modifiers::NONE, false)
                .unwrap();
        }
        assert_eq!(input.control_value().unwrap().as_deref(), Some("a B"));
        let keys: Vec<_> = seen.borrow().iter().map(|entry| entry.1).collect();
        assert_eq!(
            keys,
            [
                Some(Key::Char('a')),
                Some(Key::Char('a')),
                Some(Key::Space),
                Some(Key::Space),
                Some(Key::B),
                Some(Key::B),
            ]
        );
    }

    #[test]
    fn keys_type_only_characters_and_space() {
        assert_eq!(Key::Char('x').typed_text().as_deref(), Some("x"));
        assert_eq!(Key::Space.typed_text().as_deref(), Some(" "));
        assert_eq!(Key::Enter.typed_text(), None);
        assert_eq!(Key::ArrowLeft.typed_text(), None);
    }
}
