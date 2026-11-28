//! CSS-wide keywords, custom properties and `var()` (RFC 0014 step 4).
//!
//! A value that contains `var()`, and every custom property's value, is
//! kept as text and substituted when the element's style is computed, as
//! Blink's `StyleCascade` does (Chromium 147, `style_cascade.cc`, BSD). The
//! text is checked here, once: it must be a `<declaration-value>` (CSS
//! Syntax 3: balanced blocks, no bad strings or URLs, no top-level `;` or
//! `!`), and every `var()` in it must name a custom property, with an
//! optional fallback. The check also splits the text into literal runs and
//! references, so substitution never tokenizes again. The engine resolves
//! references and parses a substituted standard value with the property's
//! grammar.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::css_tokens::{tokenize_spans, Token};
use crate::{ComputedStyle, LiteralError};

/// The CSS-wide keywords, valid for every property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum CssWideKeyword {
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWideKeyword {
    /// The keyword for `text`, ignoring ASCII case and surrounding
    /// whitespace.
    pub fn from_css(text: &str) -> Option<Self> {
        let text = text.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}'));
        [
            ("inherit", Self::Inherit),
            ("initial", Self::Initial),
            ("unset", Self::Unset),
            ("revert", Self::Revert),
            ("revert-layer", Self::RevertLayer),
        ]
        .into_iter()
        .find(|(name, _)| text.eq_ignore_ascii_case(name))
        .map(|(_, keyword)| keyword)
    }

    pub fn as_css(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Initial => "initial",
            Self::Unset => "unset",
            Self::Revert => "revert",
            Self::RevertLayer => "revert-layer",
        }
    }
}

impl fmt::Display for CssWideKeyword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_css())
    }
}

/// A custom property name: an identifier that starts with `--`. Names are
/// case-sensitive.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomPropertyName(Arc<str>);

impl CustomPropertyName {
    /// Check that `name` is exactly one identifier that starts with `--`.
    pub fn new(name: &str) -> Result<Self, LiteralError> {
        match tokenize_spans(name).as_slice() {
            [(Token::Ident(ident), range)] if range.start == 0 && range.end == name.len() => {
                if ident.starts_with("--") {
                    return Ok(Self(Arc::from(ident.as_str())));
                }
            }
            _ => {}
        }
        Err(LiteralError(format!(
            "`{name}` is not a custom property name (an identifier starting with `--`)"
        )))
    }

    /// A name `style!` or `stylesheet!` already checked, without checking
    /// it again.
    #[doc(hidden)]
    pub fn from_checked(name: &str) -> Self {
        Self(Arc::from(name))
    }

    /// The name, with its `--`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for CustomPropertyName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CustomPropertyName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The kind of token at one edge of a run of text. Substitution writes
/// `/**/` between two runs whose edge tokens would otherwise merge into one
/// token, as Blink's `StyleCascade::TokenSequence::Append` does. Every kind
/// that never merges is `Other`.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum TokenEdge {
    Ident,
    Function,
    Url,
    AtKeyword,
    Hash,
    Number,
    Percentage,
    Dimension,
    Cdc,
    OpenParen,
    /// `#`
    HashDelim,
    /// `-`
    Minus,
    /// `@`
    At,
    /// `.`
    Dot,
    /// `+`
    Plus,
    /// `/`
    Slash,
    /// `*`
    Star,
    /// `%`
    Percent,
    Other,
}

impl TokenEdge {
    fn of(token: &Token) -> Self {
        match token {
            Token::Ident(_) => Self::Ident,
            Token::Function(_) => Self::Function,
            Token::Url(_) | Token::BadUrl => Self::Url,
            Token::AtKeyword(_) => Self::AtKeyword,
            Token::Hash(_) => Self::Hash,
            Token::Number { .. } => Self::Number,
            Token::Percentage(_) => Self::Percentage,
            Token::Dimension { .. } => Self::Dimension,
            Token::Cdc => Self::Cdc,
            Token::OpenParen => Self::OpenParen,
            Token::Delim('#') => Self::HashDelim,
            Token::Delim('-') => Self::Minus,
            Token::Delim('@') => Self::At,
            Token::Delim('.') => Self::Dot,
            Token::Delim('+') => Self::Plus,
            Token::Delim('/') => Self::Slash,
            Token::Delim('*') => Self::Star,
            Token::Delim('%') => Self::Percent,
            _ => Self::Other,
        }
    }

    /// Whether `self` followed directly by `next` needs a comment between
    /// them. Measured on Chromium 147 for every pair (the `token-merging`
    /// case of `custom-properties-v1`); it is CSS Syntax 3's serialization
    /// table.
    fn needs_comment(self, next: Self) -> bool {
        use TokenEdge as E;
        let ident_like = matches!(
            next,
            E::Ident
                | E::Function
                | E::Url
                | E::Minus
                | E::Number
                | E::Percentage
                | E::Dimension
                | E::Cdc
        );
        match self {
            E::Ident => ident_like || next == E::OpenParen,
            E::AtKeyword | E::Hash | E::Dimension | E::HashDelim | E::Minus => ident_like,
            E::Number => matches!(
                next,
                E::Ident
                    | E::Function
                    | E::Url
                    | E::Number
                    | E::Percentage
                    | E::Dimension
                    | E::Cdc
                    | E::Percent
            ),
            E::At => matches!(next, E::Ident | E::Function | E::Url | E::Minus | E::Cdc),
            E::Dot | E::Plus => matches!(next, E::Number | E::Percentage | E::Dimension),
            E::Slash => next == E::Star,
            _ => false,
        }
    }
}

/// One part of an [`UnparsedValue`]: a run of literal text, or a `var()`.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnparsedSegment {
    /// The value's text from byte `start` to `end`, and the kinds of its
    /// first and last tokens.
    Text {
        start: u32,
        end: u32,
        first: TokenEdge,
        last: TokenEdge,
    },
    /// `var(name)` or `var(name, fallback)`.
    Var {
        name: CustomPropertyName,
        fallback: Option<UnparsedValue>,
    },
}

/// A value kept as text until the element's style is computed: a standard
/// property's value that contains `var()`, or a custom property's value.
///
/// The text is the value as written, without its leading and trailing
/// whitespace and comments, which is how Chromium serializes a custom
/// property.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnparsedValue {
    text: Arc<str>,
    segments: Arc<[UnparsedSegment]>,
}

impl UnparsedValue {
    /// Check `text` as a `<declaration-value>` whose `var()` references are
    /// well formed. An empty value is valid (a custom property may be empty).
    pub fn parse(text: &str) -> Result<Self, LiteralError> {
        let text = preprocess(text);
        let tokens = tokenize_spans(&text);
        check(&text, &tokens)?;
        let start = tokens
            .iter()
            .find(|(token, _)| *token != Token::Whitespace)
            .map_or(text.len(), |(_, range)| range.start);
        let end = tokens
            .iter()
            .rev()
            .find(|(token, _)| *token != Token::Whitespace)
            .map_or(start, |(_, range)| range.end);
        let text = &text[start..end];
        let tokens = tokenize_spans(text);
        let segments = segments(text, &tokens)?;
        Ok(Self {
            text: Arc::from(text),
            segments: segments.into(),
        })
    }

    /// A value `style!` or `stylesheet!` already checked and split, without
    /// checking it again.
    #[doc(hidden)]
    pub fn from_checked(text: &str, segments: Vec<UnparsedSegment>) -> Self {
        Self {
            text: Arc::from(text),
            segments: segments.into(),
        }
    }

    /// The value's text, as Chromium serializes it.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[doc(hidden)]
    pub fn segments(&self) -> &[UnparsedSegment] {
        &self.segments
    }

    /// Whether the value contains `var()`.
    #[doc(hidden)]
    pub fn has_references(&self) -> bool {
        self.segments
            .iter()
            .any(|segment| matches!(segment, UnparsedSegment::Var { .. }))
    }

    /// The CSS-wide keyword this value is, if it is exactly one.
    #[doc(hidden)]
    pub fn wide_keyword(&self) -> Option<CssWideKeyword> {
        match *self.segments {
            [UnparsedSegment::Text { .. }] => CssWideKeyword::from_css(&self.text),
            _ => None,
        }
    }

    /// Replace every `var()` with `resolve(name)`, or its fallback when that
    /// is `None`. `None` when a reference has neither: the value is then
    /// invalid at computed-value time.
    #[doc(hidden)]
    pub fn substitute(
        &self,
        resolve: &mut dyn FnMut(&str) -> Option<CustomValue>,
    ) -> Option<CustomValue> {
        if let [UnparsedSegment::Text { first, last, .. }] = *self.segments {
            return Some(CustomValue {
                text: self.text.clone(),
                first,
                last,
            });
        }
        let mut out = Substitution::default();
        self.substitute_into(&mut out, resolve)?;
        Some(out.finish())
    }

    fn substitute_into(
        &self,
        out: &mut Substitution,
        resolve: &mut dyn FnMut(&str) -> Option<CustomValue>,
    ) -> Option<()> {
        for segment in self.segments.iter() {
            match segment {
                UnparsedSegment::Text {
                    start,
                    end,
                    first,
                    last,
                } => out.push(&self.text[*start as usize..*end as usize], *first, *last),
                UnparsedSegment::Var { name, fallback } => match resolve(name.as_str()) {
                    Some(value) => out.push(&value.text, value.first, value.last),
                    None => fallback.as_ref()?.substitute_into(out, resolve)?,
                },
            }
        }
        Some(())
    }
}

impl fmt::Display for UnparsedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// A custom property's computed value, with the kinds of its first and
/// last tokens for later substitutions.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomValue {
    pub text: Arc<str>,
    pub first: TokenEdge,
    pub last: TokenEdge,
}

impl CustomValue {
    /// `text`'s value, with its edges read from its tokens.
    #[doc(hidden)]
    pub fn new(text: &str) -> Self {
        let tokens = tokenize_spans(text);
        let (first, last) = edges(text, &tokens, 0..text.len());
        Self {
            text: Arc::from(text),
            first,
            last,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// This value without leading and trailing whitespace. A substitution
    /// can leave some, where a reference at an end was empty.
    #[doc(hidden)]
    pub fn trimmed(self) -> Self {
        let trimmed = self.text.trim_matches(|c| matches!(c, ' ' | '\t' | '\n'));
        if trimmed.len() == self.text.len() {
            self
        } else {
            Self::new(trimmed)
        }
    }
}

/// The text a substitution builds, as Blink's `TokenSequence`.
#[derive(Default)]
struct Substitution {
    text: String,
    first: Option<TokenEdge>,
    last: Option<TokenEdge>,
}

impl Substitution {
    fn push(&mut self, text: &str, first: TokenEdge, last: TokenEdge) {
        if text.is_empty() {
            return;
        }
        if self
            .last
            .is_some_and(|previous| previous.needs_comment(first))
        {
            self.text.push_str("/**/");
        }
        self.first.get_or_insert(first);
        self.last = Some(last);
        self.text.push_str(text);
    }

    fn finish(self) -> CustomValue {
        CustomValue {
            text: Arc::from(self.text),
            first: self.first.unwrap_or(TokenEdge::Other),
            last: self.last.unwrap_or(TokenEdge::Other),
        }
    }
}

#[cfg(feature = "constructor-tokens")]
mod tokens {
    use super::*;
    use crate::constructor_tokens::{path, ConstructorTokens, TokenStream, Unsupported};
    use quote::quote;

    impl ConstructorTokens for CustomPropertyName {
        fn constructor_tokens(&self, root: &TokenStream) -> Result<TokenStream, Unsupported> {
            let name = proc_macro2::Literal::string(&self.0);
            let path = path(root, &["CustomPropertyName", "from_checked"]);
            Ok(quote!(#path(#name)))
        }
    }

    impl ConstructorTokens for UnparsedSegment {
        fn constructor_tokens(&self, root: &TokenStream) -> Result<TokenStream, Unsupported> {
            Ok(match self {
                Self::Text {
                    start,
                    end,
                    first,
                    last,
                } => {
                    let path = path(root, &["UnparsedSegment", "Text"]);
                    let first = first.constructor_tokens(root)?;
                    let last = last.constructor_tokens(root)?;
                    quote!(#path { start: #start, end: #end, first: #first, last: #last })
                }
                Self::Var { name, fallback } => {
                    let path = path(root, &["UnparsedSegment", "Var"]);
                    let name = name.constructor_tokens(root)?;
                    let fallback = match fallback {
                        Some(fallback) => {
                            let fallback = fallback.constructor_tokens(root)?;
                            quote!(::core::option::Option::Some(#fallback))
                        }
                        None => quote!(::core::option::Option::None),
                    };
                    quote!(#path { name: #name, fallback: #fallback })
                }
            })
        }
    }

    impl ConstructorTokens for UnparsedValue {
        fn constructor_tokens(&self, root: &TokenStream) -> Result<TokenStream, Unsupported> {
            let text = proc_macro2::Literal::string(&self.text);
            let segments = self
                .segments
                .iter()
                .map(|segment| segment.constructor_tokens(root))
                .collect::<Result<Vec<_>, _>>()?;
            let path = path(root, &["UnparsedValue", "from_checked"]);
            Ok(quote!(#path(#text, ::std::vec![#(#segments),*])))
        }
    }
}

/// One custom property declaration, `--name: value`.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomDeclaration {
    name: CustomPropertyName,
    value: UnparsedValue,
    /// How many standard declarations come before this one in its block,
    /// for CSSOM's interleaved order.
    position: usize,
}

impl CustomDeclaration {
    pub(crate) fn new(name: CustomPropertyName, value: UnparsedValue, position: usize) -> Self {
        Self {
            name,
            value,
            position,
        }
    }

    pub fn name(&self) -> &CustomPropertyName {
        &self.name
    }

    pub fn value(&self) -> &UnparsedValue {
        &self.value
    }
}

/// An element's computed custom properties. A child shares its parent's
/// map until it declares one of its own, like Blink's `StyleVariables`.
/// A guaranteed-invalid property has no entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomProperties(Option<Arc<BTreeMap<CustomPropertyName, CustomValue>>>);

impl CustomProperties {
    pub const NONE: Self = Self(None);

    /// The value of `name` (with its `--`), or `None` when it has none.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.as_ref()?.get(name).map(CustomValue::as_str)
    }

    #[doc(hidden)]
    pub fn get_shared(&self, name: &str) -> Option<CustomValue> {
        self.0.as_ref()?.get(name).cloned()
    }

    /// The properties in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&CustomPropertyName, &str)> {
        self.0
            .iter()
            .flat_map(|map| map.iter().map(|(name, value)| (name, value.as_str())))
    }

    /// These properties with `changes` applied: `Some` sets, `None` makes
    /// the property guaranteed-invalid. Unchanged maps stay shared.
    #[doc(hidden)]
    pub fn with_changes(
        &self,
        changes: impl IntoIterator<Item = (CustomPropertyName, Option<CustomValue>)>,
    ) -> Self {
        let mut map: Option<BTreeMap<CustomPropertyName, CustomValue>> = None;
        for (name, value) in changes {
            let current = map.as_ref().map_or_else(
                || self.0.as_ref().and_then(|map| map.get(name.as_str())),
                |map| map.get(name.as_str()),
            );
            if current == value.as_ref() {
                continue;
            }
            let map = map.get_or_insert_with(|| self.0.as_deref().cloned().unwrap_or_default());
            match value {
                Some(value) => {
                    map.insert(name, value);
                }
                None => {
                    map.remove(name.as_str());
                }
            }
        }
        match map {
            None => self.clone(),
            Some(map) if map.is_empty() => Self(None),
            Some(map) => Self(Some(Arc::new(map))),
        }
    }
}

impl ComputedStyle {
    /// A custom property's computed value, as Chromium's
    /// `getPropertyValue(name)` serializes it.
    ///
    /// `name` includes the `--`, and custom property names are
    /// case-sensitive; a malformed name gives `None`. `None` means the
    /// element has no value for it: the property is unset, or it is
    /// guaranteed-invalid, as a member of a reference cycle or a value
    /// that is exactly `initial` is. `Some("")` is an empty value, as
    /// `--x:;` declares.
    pub fn custom_property(&self, name: &str) -> Option<&str> {
        self.fields.custom_properties.get(name)
    }
}

/// Why `value` cannot be written at run time before RFC 0014 step 5, or
/// `None` when it can.
///
/// This exists only for the step-4 guards in `Engine::set_property` and
/// [`runtime_style_limit`], and goes away in step 5, when run-time writes
/// take CSS-wide keywords and `var()`. It is not part of the public API.
#[doc(hidden)]
pub fn runtime_value_limit(value: &crate::StyleValue) -> Option<&'static str> {
    match value {
        crate::StyleValue::Wide(_) => {
            Some("CSS-wide keywords cannot be applied at run time until RFC 0014 step 5")
        }
        crate::StyleValue::Unparsed(_) => {
            Some("var() cannot be applied at run time until RFC 0014 step 5")
        }
        _ => None,
    }
}

/// Why `style` cannot be applied at run time before RFC 0014 step 5,
/// for its first such declaration in source order: a custom property, a
/// CSS-wide keyword or a value with `var()`. `None` when it can.
///
/// This exists only for divui's step-4 guard in `Element::apply_style`,
/// which checks a whole `Style` before applying any of it, and goes away
/// in step 5, when run-time writes take custom properties, CSS-wide
/// keywords and `var()`. It is not part of the public API.
#[doc(hidden)]
pub fn runtime_style_limit(style: &crate::Style) -> Option<&'static str> {
    const CUSTOM: &str = "custom properties cannot be applied at run time until RFC 0014 step 5";
    let standard = style
        .declarations()
        .iter()
        .position(|declaration| runtime_value_limit(&declaration.value).is_some());
    let custom = style
        .custom_declarations()
        .first()
        .map(|first| first.position);
    match (standard, custom) {
        (Some(index), Some(position)) if position <= index => Some(CUSTOM),
        (Some(index), _) => runtime_value_limit(&style.declarations()[index].value),
        (None, Some(_)) => Some(CUSTOM),
        (None, None) => None,
    }
}

/// Parse a substituted value with `property`'s grammar, as the `style`
/// attribute reads values. A result that is exactly a CSS-wide keyword acts
/// as that keyword, as in Blink.
#[doc(hidden)]
pub fn parse_substituted(
    property: crate::StyleProperty,
    text: &str,
) -> Result<crate::StyleValue, String> {
    // A comment, including one substitution wrote, separates tokens.
    let text = crate::declaration_block::strip_comments(text).map_err(|error| error.message)?;
    let text = crate::declaration_block::collapse_whitespace(&text);
    if text.is_empty() {
        return Err("the value is empty after substitution".into());
    }
    if let Some(keyword) = CssWideKeyword::from_css(&text) {
        return Ok(crate::StyleValue::Wide(keyword));
    }
    crate::parse_value(property, &text)
}

/// CSS Syntax 3.3 preprocessing, so token ranges index the stored text.
fn preprocess(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{c}' => out.push('\n'),
            '\0' => out.push('\u{fffd}'),
            other => out.push(other),
        }
    }
    out
}

fn invalid(message: impl Into<String>) -> LiteralError {
    LiteralError(message.into())
}

fn closer(token: &Token) -> Option<Token> {
    match token {
        Token::OpenParen | Token::Function(_) => Some(Token::CloseParen),
        Token::OpenBracket => Some(Token::CloseBracket),
        Token::OpenBrace => Some(Token::CloseBrace),
        _ => None,
    }
}

fn is_var(token: &Token) -> bool {
    matches!(token, Token::Function(name) if name.eq_ignore_ascii_case("var"))
}

/// Check a `<declaration-value>` and its `var()` references.
fn check(text: &str, tokens: &[(Token, Range<usize>)]) -> Result<(), LiteralError> {
    let mut stack: Vec<Token> = Vec::new();
    for (index, (token, _)) in tokens.iter().enumerate() {
        match token {
            Token::BadString => return Err(invalid("a value holds an unterminated string")),
            Token::BadUrl => return Err(invalid("a value holds a malformed `url()`")),
            Token::Semicolon if stack.is_empty() => {
                return Err(invalid("a value cannot hold a top-level `;`"))
            }
            Token::Delim('!') if stack.is_empty() => {
                return Err(invalid("a value cannot hold a top-level `!`"))
            }
            Token::CloseParen | Token::CloseBracket | Token::CloseBrace => {
                if stack.pop().as_ref() != Some(token) {
                    return Err(invalid("a value has an unmatched closing bracket"));
                }
            }
            _ => {}
        }
        if let Some(close) = closer(token) {
            stack.push(close);
        }
        if is_var(token) {
            let close = matching_close(tokens, index);
            check_var_arguments(text, &tokens[index + 1..close.min(tokens.len())])?;
        }
    }
    if !stack.is_empty() {
        return Err(invalid("a value has an unclosed bracket"));
    }
    Ok(())
}

/// The index of the token that closes the block or function opened at
/// `open`, or `tokens.len()` when it does not close.
fn matching_close(tokens: &[(Token, Range<usize>)], open: usize) -> usize {
    let mut depth = 0usize;
    for (index, (token, _)) in tokens.iter().enumerate().skip(open) {
        if closer(token).is_some() {
            depth += 1;
        } else if matches!(
            token,
            Token::CloseParen | Token::CloseBracket | Token::CloseBrace
        ) {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    tokens.len()
}

/// `var( <custom-property-name> , <declaration-value>? )`: whitespace, the
/// name, whitespace, and an optional comma with the fallback after it.
fn check_var_arguments(
    text: &str,
    arguments: &[(Token, Range<usize>)],
) -> Result<(), LiteralError> {
    let mut rest = arguments
        .iter()
        .skip_while(|(token, _)| *token == Token::Whitespace);
    match rest.next() {
        Some((Token::Ident(name), _)) if name.starts_with("--") => {}
        _ => {
            return Err(invalid(format!(
                "`var({})` does not start with a custom property name",
                arguments
                    .first()
                    .zip(arguments.last())
                    .map_or("", |(first, last)| &text[first.1.start..last.1.end])
            )))
        }
    }
    match rest
        .skip_while(|(token, _)| *token == Token::Whitespace)
        .next()
    {
        None | Some((Token::Comma, _)) => Ok(()),
        Some(_) => Err(invalid(
            "`var()` takes a custom property name and an optional fallback after a comma",
        )),
    }
}

/// The kinds of the first and last tokens in `range` of `text`. A comment
/// at either end separates, so that edge is `Other`.
fn edges(
    text: &str,
    tokens: &[(Token, Range<usize>)],
    range: Range<usize>,
) -> (TokenEdge, TokenEdge) {
    let mut inside = tokens
        .iter()
        .filter(|(_, span)| span.start >= range.start && span.end <= range.end);
    let first = inside.next();
    let last = inside.last().or(first);
    let first = match first {
        Some((token, span)) if span.start == range.start => TokenEdge::of(token),
        _ => TokenEdge::Other,
    };
    let last = match last {
        Some((token, span)) if span.end == range.end => TokenEdge::of(token),
        _ => TokenEdge::Other,
    };
    debug_assert!(range.end <= text.len());
    (first, last)
}

/// Split checked, trimmed `text` into literal runs and `var()` references.
fn segments(
    text: &str,
    tokens: &[(Token, Range<usize>)],
) -> Result<Vec<UnparsedSegment>, LiteralError> {
    let offset = |position: usize| {
        u32::try_from(position).map_err(|_| invalid("a value is longer than 4 GiB"))
    };
    let mut segments = Vec::new();
    let mut run_start = 0;
    let mut index = 0;
    while index < tokens.len() {
        let (token, span) = &tokens[index];
        if !is_var(token) {
            index += 1;
            continue;
        }
        if run_start < span.start {
            let (first, last) = edges(text, tokens, run_start..span.start);
            segments.push(UnparsedSegment::Text {
                start: offset(run_start)?,
                end: offset(span.start)?,
                first,
                last,
            });
        }
        let close = matching_close(tokens, index);
        let arguments = &tokens[index + 1..close];
        let mut rest = arguments
            .iter()
            .filter(|(token, _)| *token != Token::Whitespace);
        let Some((Token::Ident(name), _)) = rest.next() else {
            unreachable!("checked by `check_var_arguments`");
        };
        let fallback = match rest.next() {
            Some((Token::Comma, comma)) => Some(UnparsedValue::parse(
                &text[comma.end..tokens[close].1.start],
            )?),
            _ => None,
        };
        segments.push(UnparsedSegment::Var {
            name: CustomPropertyName(Arc::from(name.as_str())),
            fallback,
        });
        run_start = tokens[close].1.end;
        index = close + 1;
    }
    if run_start < text.len() {
        let (first, last) = edges(text, tokens, run_start..text.len());
        segments.push(UnparsedSegment::Text {
            start: offset(run_start)?,
            end: offset(text.len())?,
            first,
            last,
        });
    }
    Ok(segments)
}

#[cfg(test)]
#[path = "custom_property_tests.rs"]
mod tests;
