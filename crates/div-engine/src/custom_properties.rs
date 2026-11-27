//! Custom properties and `var()` at computed-value time (RFC 0014 step 4).
//!
//! This follows Blink's `StyleCascade` (Chromium 147, `style_cascade.cc`,
//! BSD):
//! - every unregistered custom property inherits, and the element's winning
//!   declarations apply on top;
//! - a reference is resolved on demand, and a lock stack finds cycles:
//!   every property in a cycle is guaranteed-invalid, while one that only
//!   references a cycle falls back like any other invalid reference;
//! - a custom property whose value is exactly a CSS-wide keyword acts as
//!   that keyword (`initial` is guaranteed-invalid, the others inherit);
//! - a standard declaration with `var()` is substituted against the
//!   element's computed custom properties and parsed with the property's
//!   grammar. A failure at either step makes it `unset`
//!   (`ResolveVariableReference` returns `CSSUnsetValue`).
//!
//! Substitution writes `/**/` between tokens that would otherwise merge, as
//! Blink's `TokenSequence::Append` does (see `div_style::UnparsedValue`).

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use div_style::{
    parse_substituted, AuthoredStyleContribution, AuthoredStyleDeclarations, CssWideKeyword,
    CustomProperties, CustomValue, StyleValue, UnparsedValue,
};

/// Whether `value` is resolved only when the style is computed: a CSS-wide
/// keyword or a value with `var()`. The cascade takes these; animations,
/// presentational hints and run-time writes do not.
pub(crate) fn is_cascade_value(value: &StyleValue) -> bool {
    matches!(value, StyleValue::Wide(_) | StyleValue::Unparsed(_))
}

/// The element's computed custom properties, and its declarations with
/// every `var()` replaced by a typed value or `unset`.
pub(crate) fn substitute<'a>(
    declarations: &'a AuthoredStyleDeclarations,
    parent: &CustomProperties,
) -> (CustomProperties, Cow<'a, [AuthoredStyleContribution]>) {
    let custom = if declarations.custom().is_empty() {
        parent.clone()
    } else {
        let mut resolver = Resolver {
            declared: declarations
                .custom()
                .iter()
                .map(|declaration| (declaration.name().as_str(), declaration.value()))
                .collect(),
            parent,
            done: HashMap::new(),
            stack: Vec::new(),
            cyclic: HashSet::new(),
        };
        let changes: Vec<_> = declarations
            .custom()
            .iter()
            .map(|declaration| {
                (
                    declaration.name().clone(),
                    resolver.resolve(declaration.name().as_str()),
                )
            })
            .collect();
        parent.with_changes(changes)
    };
    let contributions = declarations.contributions();
    if !contributions
        .iter()
        .any(|contribution| matches!(contribution.value(), StyleValue::Unparsed(_)))
    {
        return (custom, Cow::Borrowed(contributions));
    }
    let substituted = contributions
        .iter()
        .map(|contribution| match contribution.value() {
            StyleValue::Unparsed(value) => {
                contribution.with_value(typed(contribution, value, &custom))
            }
            _ => contribution.clone(),
        })
        .collect();
    (custom, Cow::Owned(substituted))
}

fn typed(
    contribution: &AuthoredStyleContribution,
    value: &UnparsedValue,
    custom: &CustomProperties,
) -> StyleValue {
    value
        .substitute(&mut |name| custom.get_shared(name))
        .and_then(|text| parse_substituted(contribution.property(), text.as_str()).ok())
        .unwrap_or(StyleValue::Wide(CssWideKeyword::Unset))
}

struct Resolver<'a> {
    declared: HashMap<&'a str, &'a UnparsedValue>,
    parent: &'a CustomProperties,
    done: HashMap<String, Option<CustomValue>>,
    stack: Vec<String>,
    cyclic: HashSet<String>,
}

impl Resolver<'_> {
    /// `name`'s computed value, or `None` when it is guaranteed-invalid.
    fn resolve(&mut self, name: &str) -> Option<CustomValue> {
        if let Some(value) = self.done.get(name) {
            return value.clone();
        }
        let Some(value) = self.declared.get(name).copied() else {
            return self.parent.get_shared(name);
        };
        if let Some(position) = self.stack.iter().position(|entry| entry == name) {
            self.cyclic.extend(self.stack[position..].iter().cloned());
            return None;
        }
        let result = match value.wide_keyword() {
            Some(CssWideKeyword::Initial) => None,
            Some(_) => self.parent.get_shared(name),
            None => {
                self.stack.push(name.to_owned());
                let value = value.substitute(&mut |reference| self.resolve(reference));
                self.stack.pop();
                value.map(CustomValue::trimmed)
            }
        };
        let result = if self.cyclic.contains(name) {
            None
        } else {
            result
        };
        self.done.insert(name.to_owned(), result.clone());
        result
    }
}
