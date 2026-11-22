//! Style presets (RFC 0004): the starting style of a document.
//!
//! A document starts unstyled ([`StylePreset::NONE`]): elements take CSS
//! initial values and their kind's native `display`. [`StylePreset::BROWSER`]
//! adds Div's user-agent style sheet for HTML elements, written from the HTML
//! Standard's rendering section and checked against Chromium 147. Its rules
//! rank in the user-agent origin, below every author rule, element style and
//! `style` attribute.
//!
//! The sheet comes in parts (`docs/architecture/user-agent-stylesheet.md`).
//! This part hides the elements and attributes the standard hides and
//! displays `html` and `body` as blocks.

use std::sync::LazyLock;

use div_style::stylesheet::{CascadeOrigin, StyleSheet};

/// Which user-agent style sheet a document uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum UserAgentStyle {
    /// No user-agent sheet: initial values and native `display` defaults.
    #[default]
    None,
    /// Chromium 147's defaults for HTML elements, which also cover Edge.
    Browser,
}

/// A document's starting style.
///
/// RFC 0004 also gives a preset author-origin base sheets, adopted before any
/// other sheet. They come with the components preset; until then a preset is
/// its user-agent sheet only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct StylePreset {
    /// The user-agent sheet.
    pub user_agent: UserAgentStyle,
}

impl StylePreset {
    /// No user-agent sheet. The default.
    pub const NONE: Self = Self {
        user_agent: UserAgentStyle::None,
    };
    /// The browser's defaults for HTML elements.
    pub const BROWSER: Self = Self {
        user_agent: UserAgentStyle::Browser,
    };
}

crate::stylesheet!(html_rules, "src/user_agent/html.css");

/// The browser preset's sheet, in the user-agent origin. `stylesheet!` emits
/// author sheets, so its rules are moved into a user-agent sheet once.
pub(crate) static BROWSER_SHEET: LazyLock<StyleSheet> = LazyLock::new(|| {
    let mut sheet = StyleSheet::new(CascadeOrigin::UserAgent);
    for rule in html_rules::SHEET.rules() {
        sheet.push(rule.clone());
    }
    sheet
});

impl UserAgentStyle {
    pub(crate) fn sheet(self) -> Option<&'static StyleSheet> {
        match self {
            Self::None => None,
            Self::Browser => Some(&BROWSER_SHEET),
        }
    }
}
