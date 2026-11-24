//! Style sheet text, read at macro time (RFC 0001).
//!
//! [`parse_stylesheet`] reads the CSS of `stylesheet!` into rules and layer
//! statements, in source order. It follows CSS Syntax Level 3 for the shape
//! of rules and blocks and CSS Nesting for nested style rules, and accepts
//! the subset of CSS the cascade supports:
//!
//! - style rules, with nested style rules (`&` and implicit nesting);
//! - `@layer a, b;` and `@layer name { ... }`.
//!
//! Anything else is an error that names the construct, never a rule that
//! silently applies nothing: `@media`, `@supports` and other at-rules,
//! anonymous and nested layers, and at-rules inside style rules. Like the
//! declaration block parser, this module exists only with the internal
//! `constructor-tokens` feature.

use std::ops::Range;

use crate::declaration_block::{
    parse_declaration, strip_comments, trimmed_range, BlockDeclaration,
};
use crate::selector::{PseudoClass, RelativeSelector, Selector, SimpleSelector};
use crate::selector_parser::parse_selector_list;

/// One style rule of the sheet, with nesting already resolved: its selectors
/// are complete, and its declarations are the ones written directly in its
/// block before the next nested rule, or after one.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<BlockDeclaration>,
    pub layer: Option<String>,
}

/// A statement of the sheet, in source order.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetItem {
    /// `@layer a, b;`: declares layers in order.
    Layers(Vec<String>),
    Rule(SheetRule),
}

/// Something the sheet cannot accept, with its byte range in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetError {
    pub message: String,
    pub source: Range<usize>,
}

/// Parse a style sheet. Every error is reported, not only the first.
pub fn parse_stylesheet(text: &str) -> Result<Vec<SheetItem>, Vec<SheetError>> {
    let cleaned = match strip_comments(text) {
        Ok(cleaned) => cleaned,
        Err(error) => {
            return Err(vec![SheetError {
                message: error.message,
                source: error.source,
            }])
        }
    };
    let mut parser = SheetParser {
        text: &cleaned,
        items: Vec::new(),
        errors: Vec::new(),
    };
    parser.top_level(0..cleaned.len(), None);
    if parser.errors.is_empty() {
        Ok(parser.items)
    } else {
        Err(parser.errors)
    }
}

/// Every class name the rules' selectors use, in order of first appearance,
/// including those inside `:is()`, `:where()`, `:not()`, `:has()` and
/// `:nth-child(of ...)`.
pub fn class_names(items: &[SheetItem]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        if let SheetItem::Rule(rule) = item {
            for selector in &rule.selectors {
                selector_classes(selector, &mut names);
            }
        }
    }
    names
}

fn selector_classes(selector: &Selector, names: &mut Vec<String>) {
    let compounds = std::iter::once(&selector.subject)
        .chain(selector.ancestors.iter().map(|(_, compound)| compound));
    // Ancestors are stored right to left; report classes left to right.
    let mut compounds: Vec<_> = compounds.collect();
    compounds.reverse();
    for compound in compounds {
        for simple in &compound.simple {
            match simple {
                SimpleSelector::Class(name) => {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
                SimpleSelector::PseudoClass(pseudo) => pseudo_classes(pseudo, names),
                _ => {}
            }
        }
    }
}

fn pseudo_classes(pseudo: &PseudoClass, names: &mut Vec<String>) {
    match pseudo {
        PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) => {
            for selector in list {
                selector_classes(selector, names);
            }
        }
        PseudoClass::NthChild { of: Some(list), .. } => {
            for selector in list {
                selector_classes(selector, names);
            }
        }
        PseudoClass::Has(list) => {
            for RelativeSelector { selector, .. } in list {
                selector_classes(selector, names);
            }
        }
        _ => {}
    }
}

/// One item of a list of rules or of a block's contents.
enum Item {
    /// `@name prelude;` or `@name prelude { block }`.
    AtRule {
        name: String,
        whole: Range<usize>,
        prelude: Range<usize>,
        block: Option<Range<usize>>,
    },
    /// `prelude { block }`.
    Qualified {
        prelude: Range<usize>,
        block: Range<usize>,
    },
    /// Text up to the next top-level `;` that opens no block.
    Declaration(Range<usize>),
}

struct SheetParser<'a> {
    text: &'a str,
    items: Vec<SheetItem>,
    errors: Vec<SheetError>,
}

impl SheetParser<'_> {
    fn error(&mut self, message: impl Into<String>, source: Range<usize>) {
        self.errors.push(SheetError {
            message: message.into(),
            source,
        });
    }

    fn snippet(&self, range: &Range<usize>) -> String {
        let text = self.text[range.clone()].trim();
        let first_line = text.lines().next().unwrap_or_default();
        if first_line.len() > 60 {
            format!("{}...", &first_line[..floor_char_boundary(first_line, 60)])
        } else {
            first_line.to_owned()
        }
    }

    /// The rules of a sheet, or of an `@layer` block when `layer` is set.
    fn top_level(&mut self, range: Range<usize>, layer: Option<&str>) {
        for item in self.items_in(range) {
            match item {
                Item::AtRule {
                    name,
                    whole,
                    prelude,
                    block,
                } => self.at_rule(&name, whole, prelude, block, layer),
                Item::Qualified { prelude, block } => {
                    let Some(selectors) = self.selectors(prelude, None) else {
                        continue;
                    };
                    self.style_block(block, &selectors, layer);
                }
                Item::Declaration(range) => {
                    let snippet = self.snippet(&range);
                    self.error(
                        format!("`{snippet}`: a declaration outside a style rule"),
                        range,
                    );
                }
            }
        }
    }

    fn at_rule(
        &mut self,
        name: &str,
        whole: Range<usize>,
        prelude: Range<usize>,
        block: Option<Range<usize>>,
        layer: Option<&str>,
    ) {
        let lowercase = name.to_ascii_lowercase();
        match lowercase.as_str() {
            "layer" => {
                if layer.is_some() {
                    self.error(
                        "a layer inside a layer (a sublayer) is not supported yet",
                        whole,
                    );
                    return;
                }
                let names = self.text[prelude.clone()].trim();
                match block {
                    None => {
                        let mut layers = Vec::new();
                        for name in names.split(',') {
                            let name = name.trim();
                            if let Err(message) = layer_name(name) {
                                self.error(message, whole.clone());
                                return;
                            }
                            layers.push(name.to_owned());
                        }
                        self.items.push(SheetItem::Layers(layers));
                    }
                    Some(block) => {
                        if names.is_empty() {
                            self.error(
                                "an anonymous `@layer { }` is not supported yet; name the layer",
                                whole,
                            );
                            return;
                        }
                        if let Err(message) = layer_name(names) {
                            self.error(message, whole);
                            return;
                        }
                        let names = names.to_owned();
                        self.top_level(block, Some(&names));
                    }
                }
            }
            "media" => self.error("`@media`: media queries land with Tier 0 item 0.5", whole),
            "supports" => self.error("`@supports` is not supported yet", whole),
            _ => self.error(
                format!("`@{name}` is not supported in a style sheet"),
                whole,
            ),
        }
    }

    /// A style rule's block: its declarations, and its nested rules, which
    /// follow it in order of appearance. Declarations after a nested rule
    /// form a rule of their own with the same selectors, after that nested
    /// rule, as CSS Nesting's nested declarations rules do.
    fn style_block(&mut self, range: Range<usize>, selectors: &[Selector], layer: Option<&str>) {
        let mut declarations = Vec::new();
        for item in self.items_in(range) {
            match item {
                Item::Declaration(range) => {
                    match parse_declaration(self.text, trimmed_range(self.text, range)) {
                        Ok(declaration) => declarations.push(declaration),
                        Err(error) => self.error(error.message, error.source),
                    }
                }
                Item::Qualified { prelude, block } => {
                    self.push_rule(selectors, std::mem::take(&mut declarations), layer);
                    let Some(nested) = self.selectors(prelude, Some(selectors)) else {
                        continue;
                    };
                    self.style_block(block, &nested, layer);
                }
                Item::AtRule { name, whole, .. } => self.error(
                    format!("`@{name}` inside a style rule is not supported yet"),
                    whole,
                ),
            }
        }
        self.push_rule(selectors, declarations, layer);
    }

    fn push_rule(
        &mut self,
        selectors: &[Selector],
        declarations: Vec<BlockDeclaration>,
        layer: Option<&str>,
    ) {
        if declarations.is_empty() {
            return;
        }
        self.items.push(SheetItem::Rule(SheetRule {
            selectors: selectors.to_vec(),
            declarations,
            layer: layer.map(str::to_owned),
        }));
    }

    fn selectors(
        &mut self,
        prelude: Range<usize>,
        parent: Option<&[Selector]>,
    ) -> Option<Vec<Selector>> {
        let text = &self.text[prelude.clone()];
        match parse_selector_list(text, parent) {
            Ok(selectors) => Some(selectors),
            Err(error) => {
                let start = prelude.start + error.source.start.min(text.len());
                let end = prelude.start + error.source.end.min(text.len());
                self.error(error.message, start..end);
                None
            }
        }
    }

    /// Split `range` into items. An unbalanced closing bracket, or a block
    /// left open, is an error.
    fn items_in(&mut self, range: Range<usize>) -> Vec<Item> {
        let bytes = self.text.as_bytes();
        let mut items = Vec::new();
        let mut index = range.start;
        while index < range.end {
            let byte = bytes[index];
            if byte.is_ascii_whitespace() || byte == b';' {
                index += 1;
                continue;
            }
            let start = index;
            let at_name = (byte == b'@').then(|| {
                let name_end = self.text[start + 1..range.end]
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                    .map_or(range.end, |offset| start + 1 + offset);
                (self.text[start + 1..name_end].to_owned(), name_end)
            });
            let scan_from = at_name.as_ref().map_or(start, |(_, end)| *end);
            match self.next_stop(scan_from, range.end) {
                Err(position) => {
                    self.error(
                        format!("`{}` has no opening bracket", byte_char(bytes[position])),
                        position..position + 1,
                    );
                    return items;
                }
                Ok((stop, b'{')) => {
                    let Some(close) = self.matching_brace(stop, range.end) else {
                        self.error("a block is never closed", stop..range.end);
                        return items;
                    };
                    let prelude = scan_from..stop;
                    let block = stop + 1..close;
                    items.push(match at_name {
                        Some((name, _)) => Item::AtRule {
                            name,
                            whole: start..close + 1,
                            prelude,
                            block: Some(block),
                        },
                        None => Item::Qualified {
                            prelude: start..stop,
                            block,
                        },
                    });
                    index = close + 1;
                }
                Ok((stop, _)) => {
                    items.push(match at_name {
                        Some((name, _)) => Item::AtRule {
                            name,
                            whole: start..stop,
                            prelude: scan_from..stop,
                            block: None,
                        },
                        None => Item::Declaration(start..stop),
                    });
                    index = stop + 1;
                }
            }
        }
        items
    }

    /// The first top-level `;` or `{` at or after `from`, outside strings and
    /// brackets, or `end` (reported as `;`). `Err` holds the position of a
    /// closing bracket with no opening one.
    fn next_stop(&self, from: usize, end: usize) -> Result<(usize, u8), usize> {
        let bytes = self.text.as_bytes();
        let mut depth: Vec<u8> = Vec::new();
        let mut quote = None;
        let mut index = from;
        while index < end {
            let byte = bytes[index];
            if let Some(open) = quote {
                if byte == b'\\' {
                    index += 1;
                } else if byte == open {
                    quote = None;
                }
            } else {
                match byte {
                    b'"' | b'\'' => quote = Some(byte),
                    b'(' => depth.push(b')'),
                    b'[' => depth.push(b']'),
                    b')' | b']' => {
                        if depth.pop() != Some(byte) {
                            return Err(index);
                        }
                    }
                    b'}' if depth.is_empty() => return Err(index),
                    b'{' | b';' if depth.is_empty() => return Ok((index, byte)),
                    _ => {}
                }
            }
            index += 1;
        }
        Ok((end, b';'))
    }

    /// The `}` that closes the `{` at `open`, before `end`.
    fn matching_brace(&self, open: usize, end: usize) -> Option<usize> {
        let bytes = self.text.as_bytes();
        let mut depth = 0usize;
        let mut quote = None;
        let mut index = open;
        while index < end {
            let byte = bytes[index];
            if let Some(quote_byte) = quote {
                if byte == b'\\' {
                    index += 1;
                } else if byte == quote_byte {
                    quote = None;
                }
            } else {
                match byte {
                    b'"' | b'\'' => quote = Some(byte),
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(index);
                        }
                    }
                    _ => {}
                }
            }
            index += 1;
        }
        None
    }
}

fn byte_char(byte: u8) -> char {
    byte as char
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    (0..=index.min(text.len()))
        .rev()
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(0)
}

/// A layer name is one CSS identifier. Dotted names would be sublayers.
fn layer_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("`@layer` needs a layer name".to_owned());
    }
    if name.contains('.') {
        return Err(format!(
            "`{name}`: dotted layer names (sublayers) are not supported yet"
        ));
    }
    let mut chars = name.chars();
    let first = chars.next().expect("not empty");
    let starts = first.is_ascii_alphabetic()
        || first == '_'
        || !first.is_ascii()
        || (first == '-' && name.len() > 1);
    if starts && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()) {
        Ok(())
    } else {
        Err(format!("`{name}` is not a layer name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StyleProperty;

    fn rules(text: &str) -> Vec<SheetItem> {
        parse_stylesheet(text).unwrap_or_else(|errors| panic!("{errors:?}"))
    }

    #[test]
    fn a_name_with_no_css_lookup_is_an_error() {
        for name in crate::INTERNAL_CSS_NAMES {
            let errors = parse_stylesheet(&format!(".a {{ {name}: 1; }}")).unwrap_err();
            assert_eq!(errors.len(), 1, "{name}");
            assert!(errors[0].message.contains(name), "{name}: {errors:?}");
        }
    }

    fn rule(item: &SheetItem) -> &SheetRule {
        match item {
            SheetItem::Rule(rule) => rule,
            SheetItem::Layers(_) => panic!("expected a rule"),
        }
    }

    fn message(text: &str) -> String {
        let errors = parse_stylesheet(text).expect_err(text);
        errors[0].message.clone()
    }

    #[test]
    fn rules_keep_source_order_and_importance() {
        let items = rules(".a { color: red; opacity: 0.5 !important } #b, p { display: flex }");
        assert_eq!(items.len(), 2);
        let first = rule(&items[0]);
        assert_eq!(first.selectors, parse_selector_list(".a", None).unwrap());
        assert_eq!(first.declarations.len(), 2);
        assert_eq!(first.declarations[0].property, StyleProperty::Color);
        assert!(!first.declarations[0].important);
        assert!(first.declarations[1].important);
        assert_eq!(rule(&items[1]).selectors.len(), 2);
    }

    #[test]
    fn nested_rules_follow_their_parent_and_split_its_declarations() {
        let items = rules(
            ".card { color: red; &:hover { color: blue } .title { opacity: 0.5 } display: flex }",
        );
        assert_eq!(items.len(), 4);
        let parent = parse_selector_list(".card", None).unwrap();
        assert_eq!(rule(&items[0]).selectors, parent);
        assert_eq!(
            rule(&items[1]).selectors,
            parse_selector_list("&:hover", Some(&parent)).unwrap()
        );
        assert_eq!(
            rule(&items[2]).selectors,
            parse_selector_list(".title", Some(&parent)).unwrap()
        );
        // Declarations after a nested rule form their own rule, after it.
        assert_eq!(rule(&items[3]).selectors, parent);
        assert_eq!(
            rule(&items[3]).declarations[0].property,
            StyleProperty::Display
        );
    }

    #[test]
    fn layers_are_declared_and_assigned() {
        let items =
            rules("@layer base, theme; @layer theme { .a { color: red } } .b { color: blue }");
        assert_eq!(
            items[0],
            SheetItem::Layers(vec!["base".to_owned(), "theme".to_owned()])
        );
        assert_eq!(rule(&items[1]).layer.as_deref(), Some("theme"));
        assert_eq!(rule(&items[2]).layer, None);
    }

    #[test]
    fn comments_and_strings_do_not_split_rules() {
        let items = rules(
            "/* a } comment */ [data-x=\"}{;\"] { font-family: \"a;b\" } /* ; */ .c { color: red }",
        );
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn unsupported_constructs_are_errors() {
        assert!(message("@media (width > 1px) { .a { color: red } }").contains("0.5"));
        assert!(message("@supports (display: grid) { }").contains("@supports"));
        assert!(message("@font-face { font-family: x }").contains("@font-face"));
        assert!(message("@layer { .a { color: red } }").contains("anonymous"));
        assert!(message("@layer a { @layer b { } }").contains("sublayer"));
        assert!(message("@layer a.b;").contains("dotted"));
        assert!(message(".a { @media print { } }").contains("inside a style rule"));
        assert!(message("color: red;").contains("outside a style rule"));
        assert!(message(".a { color: red").contains("never closed"));
        assert!(message(".a { color: red } }").contains("no opening bracket"));
        assert!(message(".a { colr: red }").contains("colr"));
    }

    #[test]
    fn every_error_is_reported() {
        let errors = parse_stylesheet(".a { colr: red } @media print { } .b { opacity: x }")
            .expect_err("three errors");
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn class_names_are_listed_once_in_order() {
        let items = rules(
            ".b .a { color: red } .a:is(.c, :not(.d)):has(> .e) { color: blue } \
             :nth-child(2 of .f) { color: red }",
        );
        assert_eq!(class_names(&items), ["b", "a", "c", "d", "e", "f"]);
    }
}
