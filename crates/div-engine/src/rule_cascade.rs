//! The rule cascade (CSS Cascade 5, section 6).
//!
//! Adopted [`StyleSheet`]s become a [`RuleSet`]. Rules are bucketed by the
//! rightmost compound of each selector (id, then class, then local name,
//! then everything else), as Blink's `RuleSet` buckets them, so an element
//! tests only the rules its id, classes and name can select, as in Blink's
//! `ElementRuleCollector`.
//!
//! The declarations that match an element are applied in ascending
//! precedence:
//!
//! 1. user-agent normal rules;
//! 2. presentational hints (author origin, before every author rule);
//! 3. author normal rules;
//! 4. the element's own declarations, which rank as the `style` attribute;
//! 5. author `!important` rules;
//! 6. user-agent `!important` rules.
//!
//! Within an origin and importance, rules are ordered by layer, then by the
//! specificity of the most specific selector of the rule that matches, then
//! by order of appearance (sheet adoption order, then rule order). Normal
//! layered rules rank below unlayered ones, in layer order; `!important`
//! reverses the layer order. Animations apply above normal declarations and
//! below `!important` ones.
//!
//! Invalidation is conservative. Changing an attribute restyles the
//! element's subtree, or its parent's when a rule depends on siblings, or the
//! document when a rule uses `:has()`. Inserting or removing children
//! restyles the parent's subtree. Interaction and form state restyle the
//! whole document at the next `update()`, as Blink recalculates style
//! lazily. Nothing here runs until a sheet is adopted.

use std::borrow::Cow;
use std::collections::HashMap;

use div_dom::NodeId;
use div_style::selector::{PseudoClass, Selector, SimpleSelector, Specificity};
use div_style::stylesheet::{CascadeOrigin, StyleSheet};
use div_style::{
    AuthoredStyleDeclarations, CanonicalStyleFields, InvalidationClass, Style, StyleProperty,
    StyleValue,
};

use crate::selector_match::{is_html_space, local_name, SelectorMatchError};
use crate::{Engine, EngineError, NodeHandle};

/// An adopted style sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleSheetId(u64);

/// Why a sheet could not be adopted or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleSheetError {
    /// A declaration's value does not fit its property.
    PropertyType {
        property: StyleProperty,
    },
    /// A selector uses a pseudo-class the matcher does not read yet.
    UnsupportedPseudoClass(&'static str),
    /// No adopted sheet has this id.
    UnknownSheet,
    Engine(EngineError),
}

impl From<EngineError> for StyleSheetError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

struct CompiledRule {
    selectors: Vec<(Selector, Specificity)>,
    normal: AuthoredStyleDeclarations,
    important: AuthoredStyleDeclarations,
    /// Index into the sheet's layer names.
    layer: Option<usize>,
}

struct AdoptedSheet {
    id: StyleSheetId,
    origin: CascadeOrigin,
    layers: Vec<String>,
    rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuleRef {
    sheet: u32,
    rule: u32,
    selector: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RuleFlags {
    /// Some selector uses `:has()`.
    pub(crate) relational: bool,
    /// Some selector depends on siblings: a sibling combinator or a
    /// structural pseudo-class.
    pub(crate) sibling_dependent: bool,
    /// Some selector reads interaction or form state.
    pub(crate) state_dependent: bool,
}

/// The adopted sheets and their derived buckets.
#[derive(Default)]
pub(crate) struct RuleSet {
    sheets: Vec<AdoptedSheet>,
    next_id: u64,
    /// Per sheet, the origin-wide rank of each of its layers.
    layer_ranks: Vec<Vec<u32>>,
    /// Distinct layers per origin: user agent, author.
    layer_counts: [u32; 2],
    by_id: HashMap<String, Vec<RuleRef>>,
    by_class: HashMap<String, Vec<RuleRef>>,
    by_tag: HashMap<String, Vec<RuleRef>>,
    universal: Vec<RuleRef>,
    pub(crate) flags: RuleFlags,
}

/// Where a change can alter which rules match.
pub(crate) enum SelectorChange {
    /// An element's own id, class or other attribute.
    Element(NodeId),
    /// The children of a node.
    Children(NodeId),
}

fn origin_index(origin: CascadeOrigin) -> usize {
    match origin {
        CascadeOrigin::UserAgent => 0,
        CascadeOrigin::Author => 1,
    }
}

fn unsupported_pseudo_class(selector: &Selector) -> Option<&'static str> {
    let compounds =
        std::iter::once(&selector.subject).chain(selector.ancestors.iter().map(|(_, c)| c));
    for simple in compounds.flat_map(|compound| compound.simple.iter()) {
        if let SimpleSelector::PseudoClass(pseudo) = simple {
            let found = match pseudo {
                PseudoClass::Invalid => Some("invalid"),
                PseudoClass::Open => Some("open"),
                PseudoClass::Modal => Some("modal"),
                PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) => {
                    list.iter().find_map(unsupported_pseudo_class)
                }
                PseudoClass::NthChild { of: Some(list), .. } => {
                    list.iter().find_map(unsupported_pseudo_class)
                }
                PseudoClass::Has(list) => list
                    .iter()
                    .find_map(|relative| unsupported_pseudo_class(&relative.selector)),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

fn add_flags(selector: &Selector, flags: &mut RuleFlags) {
    use div_style::selector::Combinator;
    if selector.ancestors.iter().any(|(combinator, _)| {
        matches!(
            combinator,
            Combinator::NextSibling | Combinator::SubsequentSibling
        )
    }) {
        flags.sibling_dependent = true;
    }
    let compounds =
        std::iter::once(&selector.subject).chain(selector.ancestors.iter().map(|(_, c)| c));
    for simple in compounds.flat_map(|compound| compound.simple.iter()) {
        let SimpleSelector::PseudoClass(pseudo) = simple else {
            continue;
        };
        match pseudo {
            PseudoClass::Has(list) => {
                flags.relational = true;
                for relative in list {
                    add_flags(&relative.selector, flags);
                }
            }
            PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) => {
                for selector in list {
                    add_flags(selector, flags);
                }
            }
            PseudoClass::NthChild { of, .. } => {
                flags.sibling_dependent = true;
                for selector in of.iter().flatten() {
                    add_flags(selector, flags);
                }
            }
            PseudoClass::FirstChild | PseudoClass::LastChild | PseudoClass::OnlyChild => {
                flags.sibling_dependent = true;
            }
            PseudoClass::Empty | PseudoClass::Root => {}
            _ => flags.state_dependent = true,
        }
    }
}

/// The bucket of a selector's rightmost compound: id, then class, then
/// local name; `None` for the universal bucket.
fn bucket_key(selector: &Selector) -> Option<(u8, &str)> {
    let simple = &selector.subject.simple;
    simple
        .iter()
        .find_map(|s| match s {
            SimpleSelector::Id(id) => Some((0, id.as_str())),
            _ => None,
        })
        .or_else(|| {
            simple.iter().find_map(|s| match s {
                SimpleSelector::Class(class) => Some((1, class.as_str())),
                _ => None,
            })
        })
        .or_else(|| {
            simple.iter().find_map(|s| match s {
                SimpleSelector::Tag(name) => Some((2, name.as_str())),
                _ => None,
            })
        })
}

fn compile_declarations(style: &Style) -> Result<AuthoredStyleDeclarations, StyleSheetError> {
    Engine::canonical_declarations(style).map_err(|error| match error {
        EngineError::PropertyType { property } => StyleSheetError::PropertyType { property },
        other => StyleSheetError::Engine(other),
    })
}

impl RuleSet {
    pub(crate) fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    fn compile(&mut self, sheet: &StyleSheet) -> Result<AdoptedSheet, StyleSheetError> {
        let mut rules = Vec::new();
        for rule in sheet.rules() {
            for selector in rule.selectors() {
                if let Some(name) = unsupported_pseudo_class(selector) {
                    return Err(StyleSheetError::UnsupportedPseudoClass(name));
                }
            }
            rules.push(CompiledRule {
                selectors: rule
                    .selectors()
                    .iter()
                    .map(|selector| (selector.clone(), selector.specificity()))
                    .collect(),
                normal: compile_declarations(rule.declarations())?,
                important: compile_declarations(rule.important())?,
                layer: rule
                    .layer()
                    .map(|name| sheet.layers().iter().position(|layer| layer == name))
                    .map(|index| index.expect("a pushed rule declares its layer")),
            });
        }
        self.next_id += 1;
        Ok(AdoptedSheet {
            id: StyleSheetId(self.next_id),
            origin: sheet.origin(),
            layers: sheet.layers().to_vec(),
            rules,
        })
    }

    /// Recompute layer ranks, buckets and flags from the adopted sheets.
    fn rebuild(&mut self) {
        let mut names: [Vec<&str>; 2] = [Vec::new(), Vec::new()];
        self.layer_ranks = self
            .sheets
            .iter()
            .map(|sheet| {
                let names = &mut names[origin_index(sheet.origin)];
                sheet
                    .layers
                    .iter()
                    .map(|layer| match names.iter().position(|name| name == layer) {
                        Some(rank) => rank as u32,
                        None => {
                            names.push(layer);
                            (names.len() - 1) as u32
                        }
                    })
                    .collect()
            })
            .collect();
        self.layer_counts = [names[0].len() as u32, names[1].len() as u32];
        self.by_id.clear();
        self.by_class.clear();
        self.by_tag.clear();
        self.universal.clear();
        self.flags = RuleFlags::default();
        for (sheet_index, sheet) in self.sheets.iter().enumerate() {
            for (rule_index, rule) in sheet.rules.iter().enumerate() {
                for (selector_index, (selector, _)) in rule.selectors.iter().enumerate() {
                    add_flags(selector, &mut self.flags);
                    let reference = RuleRef {
                        sheet: sheet_index as u32,
                        rule: rule_index as u32,
                        selector: selector_index as u32,
                    };
                    match bucket_key(selector) {
                        Some((0, key)) => self.by_id.entry(key.to_owned()).or_default(),
                        Some((1, key)) => self.by_class.entry(key.to_owned()).or_default(),
                        Some((_, key)) => self.by_tag.entry(key.to_owned()).or_default(),
                        None => &mut self.universal,
                    }
                    .push(reference);
                }
            }
        }
    }

    /// Normal layered rules rank below unlayered ones, in layer order;
    /// `!important` reverses that order.
    fn layer_rank(&self, sheet: u32, rule: &CompiledRule, important: bool) -> u32 {
        let count = self.layer_counts[origin_index(self.sheets[sheet as usize].origin)];
        match (rule.layer, important) {
            (Some(layer), false) => self.layer_ranks[sheet as usize][layer],
            (None, false) => count,
            (Some(layer), true) => count - self.layer_ranks[sheet as usize][layer],
            (None, true) => 0,
        }
    }
}

/// A matched rule and the specificity of its most specific matching
/// selector.
#[derive(Clone, Copy)]
struct Matched {
    sheet: u32,
    rule: u32,
    specificity: Specificity,
}

/// An element's cascaded declarations, and the fields `!important`
/// declarations own, which animations must not override.
pub(crate) struct Cascaded<'a> {
    pub(crate) declarations: Cow<'a, AuthoredStyleDeclarations>,
    pub(crate) important: CanonicalStyleFields,
}

impl Engine {
    /// Adopt a sheet. Its rules apply after the sheets adopted before it of
    /// the same origin, and the document is restyled. On error nothing
    /// changes.
    pub fn adopt_style_sheet(
        &mut self,
        sheet: &StyleSheet,
    ) -> Result<StyleSheetId, StyleSheetError> {
        let adopted = self.rule_set.compile(sheet)?;
        let id = adopted.id;
        self.rule_set
            .sheets
            .try_reserve(1)
            .map_err(|_| EngineError::AllocationFailure)?;
        self.rule_set.sheets.push(adopted);
        self.rule_set.rebuild();
        if let Err(error) = self.restyle_document_for_rules() {
            self.rule_set.sheets.pop();
            self.rule_set.rebuild();
            return Err(error.into());
        }
        Ok(id)
    }

    /// Remove an adopted sheet and restyle the document.
    pub fn remove_style_sheet(&mut self, id: StyleSheetId) -> Result<(), StyleSheetError> {
        let position = self
            .rule_set
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or(StyleSheetError::UnknownSheet)?;
        let removed = self.rule_set.sheets.remove(position);
        self.rule_set.rebuild();
        if let Err(error) = self.restyle_document_for_rules() {
            self.rule_set.sheets.insert(position, removed);
            self.rule_set.rebuild();
            return Err(error.into());
        }
        Ok(())
    }

    /// Set a presentational hint, a declaration that ranks in the author
    /// origin below every author rule, as an SVG presentation attribute
    /// does.
    #[doc(hidden)]
    pub fn set_presentational_hint(
        &mut self,
        handle: NodeHandle,
        property: StyleProperty,
        value: StyleValue,
    ) -> Result<(), EngineError> {
        let node = self.resolve(handle)?;
        if !div_style::value_matches_property(property, &value) {
            return Err(EngineError::PropertyType { property });
        }
        let mut candidate = self.slots[handle.index as usize]
            .presentational_hints
            .clone();
        if !candidate
            .try_set_validated(property, value)
            .map_err(|_| EngineError::AllocationFailure)?
        {
            return Ok(());
        }
        self.replace_presentational_hints(handle, node, candidate)
    }

    /// Remove a presentational hint. Returns false if none was set.
    #[doc(hidden)]
    pub fn remove_presentational_hint(
        &mut self,
        handle: NodeHandle,
        property: StyleProperty,
    ) -> Result<bool, EngineError> {
        let node = self.resolve(handle)?;
        let mut candidate = self.slots[handle.index as usize]
            .presentational_hints
            .clone();
        if !candidate.remove(property) {
            return Ok(false);
        }
        self.replace_presentational_hints(handle, node, candidate)?;
        Ok(true)
    }

    fn replace_presentational_hints(
        &mut self,
        handle: NodeHandle,
        node: NodeId,
        candidate: AuthoredStyleDeclarations,
    ) -> Result<(), EngineError> {
        let previous = std::mem::replace(
            &mut self.slots[handle.index as usize].presentational_hints,
            candidate,
        );
        if let Err(error) = self.publish_native_cascade(node) {
            self.slots[handle.index as usize].presentational_hints = previous;
            return Err(error);
        }
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Subtree);
        Ok(())
    }

    fn restyle_document_for_rules(&mut self) -> Result<(), EngineError> {
        self.selector_restyle_pending = false;
        self.publish_native_cascade(self.document.root())?;
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Subtree);
        Ok(())
    }

    /// Restyle what a change to ids, classes, attributes or children can
    /// affect. Nothing happens without adopted sheets.
    pub(crate) fn restyle_for_selectors(
        &mut self,
        change: SelectorChange,
    ) -> Result<(), EngineError> {
        if self.rule_set.is_empty() {
            return Ok(());
        }
        let flags = self.rule_set.flags;
        let root = if flags.relational {
            self.document.root()
        } else {
            match change {
                SelectorChange::Element(node) => {
                    let parent = self.document.node(node).parent;
                    if flags.sibling_dependent && !parent.is_none() {
                        parent
                    } else {
                        node
                    }
                }
                SelectorChange::Children(node) => node,
            }
        };
        if root == self.document.root() {
            return self.restyle_document_for_rules();
        }
        self.publish_native_cascade(root)?;
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Subtree);
        Ok(())
    }

    /// Interaction or form state changed. Rules that read it restyle the
    /// document at the next `update()`.
    pub(crate) fn note_selector_state_change(&mut self) {
        if !self.rule_set.is_empty() && self.rule_set.flags.state_dependent {
            self.selector_restyle_pending = true;
            self.dirty.paint = true;
        }
    }

    pub(crate) fn apply_pending_selector_restyle(&mut self) -> Result<(), EngineError> {
        if self.selector_restyle_pending {
            self.restyle_document_for_rules()?;
        }
        Ok(())
    }

    fn matched_rules(&self, handle: NodeHandle) -> Result<Vec<Matched>, EngineError> {
        let rules = &self.rule_set;
        let node = self.resolve(handle)?;
        let data = self.document.node(node);
        let mut candidates: Vec<RuleRef> = rules.universal.clone();
        if let Some(id) = data.attributes.get("id") {
            candidates.extend(rules.by_id.get(id.as_str()).into_iter().flatten());
        }
        if let Some(classes) = data.attributes.get("class") {
            for class in classes
                .split(is_html_space)
                .filter(|token| !token.is_empty())
            {
                candidates.extend(rules.by_class.get(class).into_iter().flatten());
            }
        }
        if let Some(name) = local_name(data.tag) {
            candidates.extend(rules.by_tag.get(name).into_iter().flatten());
        }
        let mut matched: Vec<Matched> = Vec::new();
        for reference in candidates {
            let (selector, specificity) = &rules.sheets[reference.sheet as usize].rules
                [reference.rule as usize]
                .selectors[reference.selector as usize];
            let matches = match self.matches_selector(handle, selector) {
                Ok(matches) => matches,
                Err(SelectorMatchError::Engine(error)) => return Err(error),
                // Adoption rejects selectors the matcher cannot read.
                Err(SelectorMatchError::UnsupportedPseudoClass(_)) => false,
            };
            if !matches {
                continue;
            }
            match matched
                .iter_mut()
                .find(|m| m.sheet == reference.sheet && m.rule == reference.rule)
            {
                Some(existing) => existing.specificity = existing.specificity.max(*specificity),
                None => matched.push(Matched {
                    sheet: reference.sheet,
                    rule: reference.rule,
                    specificity: *specificity,
                }),
            }
        }
        Ok(matched)
    }

    /// The declarations the cascade gives the element in `index`'s slot,
    /// before animations.
    pub(crate) fn cascaded_declarations(&self, index: u32) -> Result<Cascaded<'_>, EngineError> {
        let slot = &self.slots[index as usize];
        if self.rule_set.is_empty() && slot.presentational_hints.contributions().is_empty() {
            return Ok(Cascaded {
                declarations: Cow::Borrowed(&slot.authored),
                important: CanonicalStyleFields::EMPTY,
            });
        }
        let matched = if self.rule_set.is_empty() {
            Vec::new()
        } else {
            self.matched_rules(self.handle_for_slot(index))?
        };
        let rules = &self.rule_set;
        let ordered = |origin: CascadeOrigin, important: bool| {
            let mut list: Vec<&Matched> = matched
                .iter()
                .filter(|m| rules.sheets[m.sheet as usize].origin == origin)
                .collect();
            list.sort_by_key(|m| {
                let rule = &rules.sheets[m.sheet as usize].rules[m.rule as usize];
                (
                    rules.layer_rank(m.sheet, rule, important),
                    m.specificity,
                    m.sheet,
                    m.rule,
                )
            });
            list
        };
        let mut out = AuthoredStyleDeclarations::default();
        let mut important = CanonicalStyleFields::EMPTY;
        let apply = |declarations: &AuthoredStyleDeclarations,
                     out: &mut AuthoredStyleDeclarations,
                     important_fields: Option<&mut CanonicalStyleFields>|
         -> Result<(), EngineError> {
            let mut fields = CanonicalStyleFields::EMPTY;
            for contribution in declarations.contributions() {
                out.try_set_validated(contribution.property(), contribution.value().clone())
                    .map_err(|_| EngineError::AllocationFailure)?;
                fields = fields.union(contribution.fields());
            }
            if let Some(important_fields) = important_fields {
                *important_fields = important_fields.union(fields);
            }
            Ok(())
        };
        let rule = |m: &Matched| &rules.sheets[m.sheet as usize].rules[m.rule as usize];
        for m in ordered(CascadeOrigin::UserAgent, false) {
            apply(&rule(m).normal, &mut out, None)?;
        }
        apply(&slot.presentational_hints, &mut out, None)?;
        for m in ordered(CascadeOrigin::Author, false) {
            apply(&rule(m).normal, &mut out, None)?;
        }
        apply(&slot.authored, &mut out, None)?;
        for m in ordered(CascadeOrigin::Author, true) {
            apply(&rule(m).important, &mut out, Some(&mut important))?;
        }
        for m in ordered(CascadeOrigin::UserAgent, true) {
            apply(&rule(m).important, &mut out, Some(&mut important))?;
        }
        Ok(Cascaded {
            declarations: Cow::Owned(out),
            important,
        })
    }
}
