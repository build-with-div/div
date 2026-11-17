//! The browser-compatible declaration parser (R0b).
//!
//! A `style` attribute is read as Chromium 147 reads one. The text is split
//! into declarations as CSS Syntax 3 splits a declaration list, and each
//! declaration Div cannot use is dropped and reported while the others
//! apply. Values go through the same [`parse_literal`] as everywhere else, so
//! the accepted values are exactly the ones `style!` accepts.
//!
//! Compared with `style!`, which rejects a whole block for one bad
//! declaration, this parser never fails: a browser keeps what it can.

use std::ops::Range;

use crate::declaration_block::{collapse_whitespace, split_important};
use crate::{parse_literal, StyleProperty, StyleValue};

/// One declaration of a `style` attribute that applies.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineDeclaration {
    pub property: StyleProperty,
    pub value: StyleValue,
    pub important: bool,
}

/// A declaration the parser dropped, with the text it dropped and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedDeclaration {
    pub text: String,
    pub reason: String,
}

/// A parsed `style` attribute: the declarations that apply, in source
/// order, and the ones that were dropped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedStyleAttribute {
    pub declarations: Vec<InlineDeclaration>,
    pub dropped: Vec<DroppedDeclaration>,
}

/// Read a `style` attribute.
///
/// Declarations are kept in source order. For one property, a later
/// declaration overrides an earlier one of the same importance, and an
/// `!important` declaration overrides normal ones wherever it appears, as
/// in Chromium. The cascade applies them in that sense: normal declarations
/// in order at the style attribute's level, important ones in order at its
/// important level.
pub fn parse_style_attribute(text: &str) -> ParsedStyleAttribute {
    let (cleaned, segments) = scan(text);
    let mut parsed = ParsedStyleAttribute::default();
    for segment in segments {
        parse_segment(&cleaned, segment, &mut parsed);
    }
    parsed
}

/// Parse one value of `property` as a `style` attribute reads it.
pub fn parse_value(property: StyleProperty, text: &str) -> Result<StyleValue, String> {
    let error = match parse_literal(property, text) {
        Ok(value) => return Ok(value),
        Err(error) => error.0,
    };
    // CSS keywords and units are ASCII case-insensitive; Div's literals are
    // written in lowercase. Strings keep their case.
    let lowercase = lowercase_outside_strings(text);
    if lowercase != text {
        if let Ok(value) = parse_literal(property, &lowercase) {
            return Ok(value);
        }
    }
    Err(error)
}

fn lowercase_outside_strings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quote = None;
    let mut escaped = false;
    for character in text.chars() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
                out.push(character);
            }
            None => {
                if character == '"' || character == '\'' {
                    quote = Some(character);
                }
                out.push(character.to_ascii_lowercase());
            }
        }
    }
    out
}

/// A CSS `<number>` token: an optional sign, digits with an optional
/// fraction (or a fraction alone), and an optional exponent.
pub(crate) fn is_css_number(token: &str) -> bool {
    let bytes = token.as_bytes();
    let mut index = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        index += 1;
    }
    let integer = bytes[index..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    index += integer;
    let mut fraction = 0;
    if bytes.get(index) == Some(&b'.') {
        fraction = bytes[index + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if fraction == 0 {
            return false;
        }
        index += 1 + fraction;
    }
    if integer == 0 && fraction == 0 {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut exponent = index + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let digits = bytes[exponent..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits > 0 {
            index = exponent + digits;
        }
    }
    index == bytes.len()
}

/// A top-level stretch of the text between `;` separators.
#[derive(Debug)]
struct Segment {
    range: Range<usize>,
    /// Why every declaration in the segment is invalid, if one is.
    bad: Option<&'static str>,
    /// Brackets still open at the end of the text, which the end closes.
    closers: Vec<u8>,
}

/// Split the text into top-level segments as CSS Syntax 3 tokenizes it.
/// Comments become spaces in the returned text, so byte offsets stay valid.
///
/// - A comment that never closes runs to the end.
/// - A string that never closes ends at the end of the text. A string that
///   meets a newline is a bad string, which invalidates its declaration.
/// - A bracket that never closes is closed by the end of the text.
/// - A closing bracket that matches no open one is a token no Div value
///   accepts, which invalidates its declaration.
fn scan(text: &str) -> (String, Vec<Segment>) {
    let mut bytes = text.as_bytes().to_vec();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut stack: Vec<u8> = Vec::new();
    let mut quote: Option<u8> = None;
    let mut bad: Option<&'static str> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            match byte {
                b'\\' => index += 1,
                b'\n' | b'\r' | 0x0c => {
                    quote = None;
                    bad.get_or_insert("a string is not closed on its line");
                }
                _ if byte == open => quote = None,
                _ => {}
            }
            index += 1;
            continue;
        }
        match byte {
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let end = text[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |length| index + 2 + length + 2);
                bytes[index..end].fill(b' ');
                index = end;
                continue;
            }
            b'\\' => index += 1,
            b'"' | b'\'' => quote = Some(byte),
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => stack.push(b'}'),
            b')' | b']' | b'}' => {
                if stack.last() == Some(&byte) {
                    stack.pop();
                } else {
                    bad.get_or_insert("a closing bracket matches no open one");
                }
            }
            b';' if stack.is_empty() => {
                segments.push(Segment {
                    range: start..index,
                    bad: bad.take(),
                    closers: Vec::new(),
                });
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    let end = bytes.len();
    // The end closes the innermost first: an open string, then brackets.
    let mut closers: Vec<u8> = quote.into_iter().collect();
    closers.extend(stack.iter().rev());
    segments.push(Segment {
        range: start..end,
        bad,
        closers,
    });
    // Only ASCII bytes of comments were replaced, whole comments at a time.
    let cleaned = String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned());
    (cleaned, segments)
}

fn parse_segment(cleaned: &str, segment: Segment, parsed: &mut ParsedStyleAttribute) {
    let range = trimmed(cleaned, segment.range.clone());
    if range.is_empty() {
        return;
    }
    let text = &cleaned[range.clone()];
    let drop = |parsed: &mut ParsedStyleAttribute, reason: String| {
        parsed.dropped.push(DroppedDeclaration {
            text: collapse_whitespace(text),
            reason,
        });
    };
    if text.starts_with('@') {
        // An at-rule ends at a `;` or after its `{}` block; what follows
        // the block starts a new declaration.
        if let Some(block_end) = first_block_end(text) {
            drop(
                parsed,
                "an at-rule has no meaning in a declaration list".into(),
            );
            if block_end < text.len() {
                let rest = Segment {
                    range: range.start + block_end..range.end,
                    bad: segment.bad,
                    closers: segment.closers,
                };
                parse_segment(cleaned, rest, parsed);
            }
        } else {
            drop(
                parsed,
                "an at-rule has no meaning in a declaration list".into(),
            );
        }
        return;
    }
    if let Some(reason) = segment.bad {
        return drop(parsed, reason.into());
    }
    if !starts_ident(text) {
        return drop(parsed, "a declaration starts with a property name".into());
    }
    let Some(colon) = text.find(':') else {
        return drop(parsed, "expected `property: value`".into());
    };
    let name = text[..colon].trim_end();
    if name
        .bytes()
        .any(|byte| is_css_whitespace(byte as char) || b"([{\"'".contains(&byte))
    {
        return drop(parsed, "expected `property: value`".into());
    }
    if name.starts_with("--") {
        return drop(parsed, "custom properties are not supported yet".into());
    }
    let Some(property) = StyleProperty::from_css_name(&name.to_ascii_lowercase()) else {
        return drop(parsed, format!("Div has no property `{name}`"));
    };
    let (value, important) = split_important(text[colon + 1..].trim());
    if value.is_empty() {
        return drop(parsed, format!("`{name}` has no value"));
    }
    let mut value = collapse_whitespace(value);
    value.extend(segment.closers.iter().map(|&byte| byte as char));
    let lowercase = value.to_ascii_lowercase();
    if matches!(
        lowercase.as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
    ) {
        return drop(
            parsed,
            format!("`{value}`: CSS-wide keywords are not supported yet"),
        );
    }
    if lowercase.contains("var(") || lowercase.contains("env(") || lowercase.contains("attr(") {
        return drop(
            parsed,
            format!("`{value}`: substitution functions are not supported yet"),
        );
    }
    match parse_value(property, &value) {
        Ok(value) => parsed.declarations.push(InlineDeclaration {
            property,
            value,
            important,
        }),
        Err(reason) => drop(parsed, reason),
    }
}

/// The byte offset just past the first top-level `{}` block, if it closes.
fn first_block_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let open = bytes.iter().position(|&byte| byte == b'{')?;
    let mut depth = 0usize;
    let mut quote = None;
    let mut index = open;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == b'\\' {
                index += 1;
            } else if byte == q {
                quote = None;
            }
        } else {
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(index + 1);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// Whether the text starts with a CSS identifier (`-`, `_`, a letter, a
/// non-ASCII character or an escape, or `--`).
fn starts_ident(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some('-') => {
            matches!(chars.next(), Some(c) if c == '-' || c == '_' || c == '\\' || c.is_ascii_alphabetic() || !c.is_ascii())
        }
        Some(c) => c == '_' || c == '\\' || c.is_ascii_alphabetic() || !c.is_ascii(),
        None => false,
    }
}

fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let slice = &text[range.clone()];
    let start = range.start + (slice.len() - slice.trim_start_matches(is_css_whitespace).len());
    let end = range.end - (slice.len() - slice.trim_end_matches(is_css_whitespace).len());
    start..end.max(start)
}

fn is_css_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Display;

    fn kept(text: &str) -> Vec<(StyleProperty, bool)> {
        parse_style_attribute(text)
            .declarations
            .iter()
            .map(|declaration| (declaration.property, declaration.important))
            .collect()
    }

    #[test]
    fn invalid_declarations_are_dropped_and_the_rest_kept() {
        let parsed = parse_style_attribute("colr: red; display: flex; opacity: lots; width");
        assert_eq!(
            parsed.declarations,
            [InlineDeclaration {
                property: StyleProperty::Display,
                value: StyleValue::from(Display::Flex),
                important: false,
            }]
        );
        assert_eq!(parsed.dropped.len(), 3, "{:?}", parsed.dropped);
        assert_eq!(parsed.dropped[0].text, "colr: red");
        assert!(parsed.dropped[0].reason.contains("no property `colr`"));
    }

    #[test]
    fn the_end_of_the_text_closes_comments_strings_and_brackets() {
        assert_eq!(
            kept("display: flex /* open"),
            [(StyleProperty::Display, false)]
        );
        assert_eq!(
            kept("color: rgb(255, 0, 0"),
            [(StyleProperty::Color, false)]
        );
        assert_eq!(
            kept("font-family: \"open"),
            [(StyleProperty::FontFamily, false)]
        );
    }

    #[test]
    fn a_stray_closing_bracket_drops_only_its_declaration() {
        assert_eq!(
            kept("color: red); display: flex"),
            [(StyleProperty::Display, false)]
        );
        assert_eq!(
            kept("{color: red}; display: flex"),
            [(StyleProperty::Display, false)]
        );
    }

    #[test]
    fn importance_is_kept_per_declaration() {
        assert_eq!(
            kept("color: red !important; color: blue"),
            [(StyleProperty::Color, true), (StyleProperty::Color, false)]
        );
        assert!(kept("color: red !ie").is_empty());
    }

    #[test]
    fn css_numbers_are_recognised() {
        assert!(is_css_number("-1.5e2") && is_css_number(".5") && !is_css_number("1."));
        assert!(!is_css_number("10 ") && !is_css_number("inf") && !is_css_number(""));
    }
}
