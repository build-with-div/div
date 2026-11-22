//! Style sheets for the rule cascade (CSS Cascade 5).
//!
//! A [`StyleSheet`] holds [`StyleRule`]s for one cascade origin. A rule pairs
//! a selector list with its normal and `!important` declarations, and may
//! belong to a named cascade layer. The engine adopts sheets and orders the
//! declarations that match an element as CSS Cascade 5 orders them: origin
//! and importance, then layer, then specificity, then order of appearance.

use crate::media_query::{MediaQueryList, MediaValues};
use crate::selector::Selector;
use crate::Style;

/// The origin of a style sheet. Div has no user origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CascadeOrigin {
    /// The engine's own defaults, such as the user-agent stylesheet.
    UserAgent,
    /// The application's styles.
    Author,
}

/// A class name that a `stylesheet!` sheet uses, as a typed constant.
///
/// `stylesheet!` makes one for each class its selectors name, under
/// `<sheet>::classes`, so a misspelt class is a compile error rather than
/// a rule that never matches. It reads as the class name: `ClassId` derefs
/// to `str`, so `element.add_class(&sheet::classes::card)` works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(&'static str);

impl ClassId {
    /// For `stylesheet!` only: the name must be one the sheet uses.
    #[doc(hidden)]
    pub const fn __from_sheet(name: &'static str) -> Self {
        Self(name)
    }

    /// The class name, as the sheet writes it.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::ops::Deref for ClassId {
    type Target = str;

    fn deref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for ClassId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for ClassId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

/// One style rule: a selector list and the declarations it applies.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    selectors: Vec<Selector>,
    declarations: Style,
    important: Style,
    layer: Option<String>,
    media: Vec<MediaQueryList>,
}

impl StyleRule {
    /// A rule applying `declarations` to the elements `selectors` match.
    pub fn new(selectors: Vec<Selector>, declarations: Style) -> Self {
        Self {
            selectors,
            declarations,
            important: Style::default(),
            layer: None,
            media: Vec::new(),
        }
    }

    /// The rule's `!important` declarations.
    pub fn with_important(mut self, important: Style) -> Self {
        self.important = important;
        self
    }

    /// Put the rule in the named cascade layer. Layers are ordered by their
    /// first appearance among the adopted sheets of the same origin. Rules
    /// outside every layer rank above all layered rules, and below them for
    /// `!important`. Dotted names are not split into sublayers yet.
    pub fn in_layer(mut self, layer: impl Into<String>) -> Self {
        self.layer = Some(layer.into());
        self
    }

    /// Apply the rule only while `media` matches, as `@media` does. Each
    /// call adds a list, as nested `@media` rules do: the rule applies while
    /// every list matches.
    pub fn with_media(mut self, media: MediaQueryList) -> Self {
        self.media.push(media);
        self
    }

    pub fn selectors(&self) -> &[Selector] {
        &self.selectors
    }

    pub fn declarations(&self) -> &Style {
        &self.declarations
    }

    pub fn important(&self) -> &Style {
        &self.important
    }

    pub fn layer(&self) -> Option<&str> {
        self.layer.as_deref()
    }

    /// The media query lists the rule applies under, outermost first.
    pub fn media(&self) -> &[MediaQueryList] {
        &self.media
    }

    /// Whether every media query list of the rule matches `values`.
    pub fn media_matches(&self, values: &MediaValues) -> bool {
        self.media.iter().all(|list| list.matches(values))
    }
}

/// An ordered list of rules from one origin.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSheet {
    origin: CascadeOrigin,
    layers: Vec<String>,
    rules: Vec<StyleRule>,
}

impl StyleSheet {
    pub fn new(origin: CascadeOrigin) -> Self {
        Self {
            origin,
            layers: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// Declare layers in order without adding rules, as `@layer a, b;` does.
    pub fn declare_layers<I, S>(&mut self, names: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for name in names {
            let name = name.into();
            if !self.layers.contains(&name) {
                self.layers.push(name);
            }
        }
    }

    /// Append a rule. Its layer, if new, is declared at this point.
    pub fn push(&mut self, rule: StyleRule) {
        if let Some(layer) = rule.layer() {
            if !self.layers.iter().any(|name| name == layer) {
                self.layers.push(layer.to_owned());
            }
        }
        self.rules.push(rule);
    }

    pub fn with_rule(mut self, rule: StyleRule) -> Self {
        self.push(rule);
        self
    }

    pub fn origin(&self) -> CascadeOrigin {
        self.origin
    }

    /// The sheet's layers, in order of first appearance.
    pub fn layers(&self) -> &[String] {
        &self.layers
    }

    pub fn rules(&self) -> &[StyleRule] {
        &self.rules
    }
}
