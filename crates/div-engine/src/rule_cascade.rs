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
//! 4. the `style` attribute's normal declarations, then the element's own
//!    declarations (`view!`'s `style:` and the typed setters), which rank
//!    with the `style` attribute and override it;
//! 5. author `!important` rules;
//! 6. the `style` attribute's `!important` declarations;
//! 7. user-agent `!important` rules.
//!
//! Within an origin and importance, rules are ordered by layer, then by the
//! specificity of the most specific selector of the rule that matches, then
//! by order of appearance (sheet adoption order, then rule order). Normal
//! layered rules rank below unlayered ones, in layer order; `!important`
//! reverses the layer order. Animations apply above normal declarations and
//! below `!important` ones.
//!
//! Invalidation follows Blink's rule invalidation data
//! (`core/css/invalidation/`) at its coarsest. An [`InvalidationIndex`]
//! records, for each class, id, attribute name and state pseudo-class the
//! adopted selectors read, where it occurs: in a subject compound, in a
//! compound an ancestor matches, in a compound a preceding sibling (or one
//! of its ancestors) matches, or inside `:has()`. A change to one element
//! looks its features up and restyles, in that order of reach, the element
//! (and its subtree only if its own style changed), its subtree, its
//! parent's subtree or the document. A feature no selector reads restyles
//! nothing.
//!
//! Attribute changes made through the public API restyle at once. State
//! changes (interaction and form state) and the attributes the engine
//! writes itself restyle at the next `update()`, as Blink recalculates style
//! lazily: each element's state bits are compared with the bits its style
//! was last resolved with, so only elements whose state changed are
//! candidates. A restyle compares each element's old and new computed style
//! and marks only the invalidation classes of the properties that changed,
//! so a hover color change repaints without layout. Inserting or removing
//! children restyles the parent's subtree, or the document when a rule uses
//! `:has()`. Nothing here runs until a sheet is adopted.
//!
//! A rule inside `@media` applies while its query lists match (Media
//! Queries 4, as Blink's `MediaQueryEvaluator` reads them). The rule set
//! keeps each distinct list stack once, with its result. When the viewport,
//! scale factor, color scheme or reduced-motion preference changes, each is
//! evaluated again, and if any result flipped the document restyles at the
//! next `update()`. Nothing is evaluated while no rule is inside `@media`.

use std::borrow::Cow;
use std::collections::HashMap;

use div_dom::NodeId;
use div_style::media_query::{ColorScheme, MediaQueryList, MediaValues};
use div_style::selector::{
    Combinator, Compound, PseudoClass, Selector, SimpleSelector, Specificity,
};
use div_style::stylesheet::{CascadeOrigin, StyleSheet};
use div_style::{
    AuthoredStyleDeclarations, CanonicalStyleFields, ComputedStyle, DroppedDeclaration,
    InvalidationClass, Style, StyleProperty, StyleValue,
};

use crate::selector_match::{is_html_space, SelectorMatchError};
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

impl std::fmt::Display for StyleSheetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PropertyType { property } => write!(
                formatter,
                "a `{}` declaration holds a value of another property's type",
                property.metadata().css_name
            ),
            Self::UnsupportedPseudoClass(name) => {
                write!(formatter, "the cascade does not match `:{name}` yet")
            }
            Self::UnknownSheet => formatter.write_str("no adopted style sheet has this id"),
            Self::Engine(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for StyleSheetError {}

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
    /// The rule's media query lists, outermost first.
    media: Vec<MediaQueryList>,
    /// Index into the rule set's media table; `None` without `@media`.
    media_slot: Option<u32>,
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
}

/// The state pseudo-classes, each one bit of an element's selector state.
const STATE_PSEUDO_CLASSES: [PseudoClass; 15] = [
    PseudoClass::Hover,
    PseudoClass::Active,
    PseudoClass::Focus,
    PseudoClass::FocusVisible,
    PseudoClass::FocusWithin,
    PseudoClass::Disabled,
    PseudoClass::Enabled,
    PseudoClass::Checked,
    PseudoClass::Indeterminate,
    PseudoClass::PlaceholderShown,
    PseudoClass::ReadOnly,
    PseudoClass::ReadWrite,
    PseudoClass::Required,
    PseudoClass::Open,
    PseudoClass::Modal,
];

fn state_bit(pseudo: &PseudoClass) -> Option<usize> {
    STATE_PSEUDO_CLASSES
        .iter()
        .position(|state| state == pseudo)
}

/// Where a feature occurs in a selector, as Blink's rule invalidation data
/// tells a subject feature from a descendant, sibling or `:has()` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    /// The subject compound: the element itself matches it.
    Subject,
    /// A compound an ancestor matches.
    Ancestor,
    /// A compound a preceding sibling, or an ancestor of one, matches.
    Sibling,
    /// Inside `:has()`: an ancestor or a preceding sibling anchors it.
    Relational,
}

impl Position {
    /// The position of the compound left of `combinator`.
    fn left_of(self, combinator: Combinator) -> Self {
        match (self, combinator) {
            (Self::Relational, _) => Self::Relational,
            (_, Combinator::NextSibling | Combinator::SubsequentSibling) | (Self::Sibling, _) => {
                Self::Sibling
            }
            _ => Self::Ancestor,
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The positions a feature occurs at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Positions(u8);

impl Positions {
    fn insert(&mut self, position: Position) {
        self.0 |= position.bit();
    }

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn contains(self, position: Position) -> bool {
        self.0 & position.bit() != 0
    }

    /// What a change to a feature at these positions restyles.
    fn scope(self) -> Option<RestyleScope> {
        if self.contains(Position::Relational) {
            Some(RestyleScope::Document)
        } else if self.contains(Position::Sibling) {
            Some(RestyleScope::Parent)
        } else if self.contains(Position::Ancestor) {
            Some(RestyleScope::Subtree)
        } else if self.contains(Position::Subject) {
            Some(RestyleScope::Element)
        } else {
            None
        }
    }
}

/// What a change to one element restyles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestyleScope {
    /// The element, and its subtree only if the element's style changes.
    Element,
    /// The element and its descendants.
    Subtree,
    /// The element's parent and the parent's descendants, which hold the
    /// element's later siblings.
    Parent,
    /// The document: a `:has()` anchor can be any ancestor or preceding
    /// sibling.
    Document,
}

/// Each class, id, attribute name and state pseudo-class the adopted
/// selectors read, with the positions it occurs at. Built when a sheet is
/// adopted or removed, from the same selectors the matcher reads.
#[derive(Default)]
pub(crate) struct InvalidationIndex {
    classes: HashMap<String, Positions>,
    ids: HashMap<String, Positions>,
    /// Attribute names, lowercased as `set_attribute` stores them.
    attributes: HashMap<String, Positions>,
    states: [Positions; STATE_PSEUDO_CLASSES.len()],
}

/// The properties no other property's fields are a strict subset of. A
/// shorthand (`border-top`, say) is left out: its longhands account for
/// every change it would, with their own invalidation classes.
fn longhand_properties() -> &'static [StyleProperty] {
    static LONGHANDS: std::sync::OnceLock<Vec<StyleProperty>> = std::sync::OnceLock::new();
    LONGHANDS.get_or_init(|| {
        let all: Vec<(StyleProperty, CanonicalStyleFields)> = (1u16..)
            .map_while(StyleProperty::from_u16)
            .map(|property| (property, CanonicalStyleFields::for_property(property)))
            .collect();
        all.iter()
            .filter(|(_, fields)| {
                !all.iter().any(|(_, other)| {
                    !other.is_empty() && *other != *fields && fields.union(*other) == *fields
                })
            })
            .map(|(property, _)| *property)
            .collect()
    })
}

fn class_tokens(value: Option<&str>) -> Vec<&str> {
    value
        .unwrap_or_default()
        .split(is_html_space)
        .filter(|token| !token.is_empty())
        .collect()
}

impl InvalidationIndex {
    fn add_selector(&mut self, selector: &Selector, subject: Position) {
        self.add_compound(&selector.subject, subject);
        let mut position = subject;
        for (combinator, compound) in &selector.ancestors {
            position = position.left_of(*combinator);
            self.add_compound(compound, position);
        }
    }

    fn add_compound(&mut self, compound: &Compound, position: Position) {
        for simple in &compound.simple {
            match simple {
                SimpleSelector::Id(id) => self.ids.entry(id.clone()).or_default().insert(position),
                SimpleSelector::Class(class) => self
                    .classes
                    .entry(class.clone())
                    .or_default()
                    .insert(position),
                SimpleSelector::Attribute(attribute) => self
                    .attributes
                    .entry(attribute.name.to_ascii_lowercase())
                    .or_default()
                    .insert(position),
                SimpleSelector::PseudoClass(pseudo) => match pseudo {
                    PseudoClass::Is(list) | PseudoClass::Where(list) | PseudoClass::Not(list) => {
                        for selector in list {
                            self.add_selector(selector, position);
                        }
                    }
                    // Whether an element matches the `of` list moves its
                    // later siblings' indices.
                    PseudoClass::NthChild { of, .. } => {
                        for selector in of.iter().flatten() {
                            self.add_selector(
                                selector,
                                position.left_of(Combinator::SubsequentSibling),
                            );
                        }
                    }
                    PseudoClass::Has(list) => {
                        for relative in list {
                            self.add_selector(&relative.selector, Position::Relational);
                        }
                    }
                    other => {
                        if let Some(bit) = state_bit(other) {
                            self.states[bit].insert(position);
                        }
                    }
                },
                SimpleSelector::Universal | SimpleSelector::Tag(_) => {}
            }
        }
    }

    /// The state bits some selector reads.
    fn state_mask(&self) -> u16 {
        self.states
            .iter()
            .enumerate()
            .filter(|(_, positions)| **positions != Positions::default())
            .fold(0, |mask, (bit, _)| mask | 1 << bit)
    }

    fn state_positions(&self, changed: u16) -> Positions {
        self.states
            .iter()
            .enumerate()
            .filter(|(bit, _)| changed & 1 << bit != 0)
            .fold(Positions::default(), |all, (_, positions)| {
                all.union(*positions)
            })
    }

    /// The positions of the features an attribute change touches: the
    /// attribute itself, and for `class` and `id` each token added or
    /// removed.
    fn attribute_positions(&self, name: &str, old: Option<&str>, new: Option<&str>) -> Positions {
        let mut positions = self.attributes.get(name).copied().unwrap_or_default();
        let lookup =
            |map: &HashMap<String, Positions>, key: &str| map.get(key).copied().unwrap_or_default();
        match name {
            "class" => {
                let (old, new) = (class_tokens(old), class_tokens(new));
                for token in old
                    .iter()
                    .filter(|token| !new.contains(token))
                    .chain(new.iter().filter(|token| !old.contains(token)))
                {
                    positions = positions.union(lookup(&self.classes, token));
                }
            }
            "id" if old != new => {
                for id in [old, new].into_iter().flatten() {
                    positions = positions.union(lookup(&self.ids, id));
                }
            }
            _ => {}
        }
        positions
    }
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
    index: InvalidationIndex,
    /// The state bits some selector reads.
    state_mask: u16,
    /// Per slot, the state bits the element's style was last resolved with.
    state_cache: HashMap<u32, u16>,
    /// Restyles queued for the next `update()`.
    pending: Vec<(NodeId, RestyleScope)>,
    /// The slots selector restyles resolved, while a test records them.
    restyle_log: Option<Vec<u32>>,
    /// Each distinct stack of media query lists the rules use, and whether
    /// it matched when last evaluated.
    media_table: Vec<(Vec<MediaQueryList>, bool)>,
}

/// Where a change can alter which rules match.
pub(crate) enum SelectorChange<'a> {
    /// An attribute of an element, with its old and new values.
    Attribute {
        node: NodeId,
        name: &'a str,
        old: Option<&'a str>,
        new: Option<&'a str>,
    },
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
            _ => {}
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
                media: rule.media().to_vec(),
                media_slot: None,
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

    /// Recompute layer ranks, buckets, flags and media results from the
    /// adopted sheets.
    fn rebuild(&mut self, media: &MediaValues) {
        let mut table: Vec<(Vec<MediaQueryList>, bool)> = Vec::new();
        for sheet in &mut self.sheets {
            for rule in &mut sheet.rules {
                rule.media_slot = (!rule.media.is_empty()).then(|| {
                    let slot = match table.iter().position(|(lists, _)| *lists == rule.media) {
                        Some(slot) => slot,
                        None => {
                            let matches = rule.media.iter().all(|list| list.matches(media));
                            table.push((rule.media.clone(), matches));
                            table.len() - 1
                        }
                    };
                    slot as u32
                });
            }
        }
        self.media_table = table;
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
        self.index = InvalidationIndex::default();
        self.state_cache.clear();
        self.pending.clear();
        for (sheet_index, sheet) in self.sheets.iter().enumerate() {
            for (rule_index, rule) in sheet.rules.iter().enumerate() {
                for (selector_index, (selector, _)) in rule.selectors.iter().enumerate() {
                    add_flags(selector, &mut self.flags);
                    self.index.add_selector(selector, Position::Subject);
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
        self.state_mask = self.index.state_mask();
    }

    /// Evaluate every media query list stack again. Whether any result
    /// changed.
    fn evaluate_media(&mut self, media: &MediaValues) -> bool {
        let mut flipped = false;
        for (lists, matches) in &mut self.media_table {
            let now = lists.iter().all(|list| list.matches(media));
            flipped |= now != *matches;
            *matches = now;
        }
        flipped
    }

    /// Whether the rule's media queries match, or it has none.
    fn media_applies(&self, rule: &CompiledRule) -> bool {
        rule.media_slot
            .is_none_or(|slot| self.media_table[slot as usize].1)
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

/// An element's parsed `style` attribute.
#[derive(Debug, Clone, Default)]
pub(crate) struct StyleAttribute {
    normal: AuthoredStyleDeclarations,
    important: AuthoredStyleDeclarations,
    dropped: Vec<DroppedDeclaration>,
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
        let media = self.media_values();
        self.rule_set.rebuild(&media);
        if let Err(error) = self.restyle_document_for_rules() {
            self.rule_set.sheets.pop();
            self.rule_set.rebuild(&media);
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
        let media = self.media_values();
        self.rule_set.rebuild(&media);
        if let Err(error) = self.restyle_document_for_rules() {
            self.rule_set.sheets.insert(position, removed);
            self.rule_set.rebuild(&media);
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

    /// Parse and apply the element's `style` attribute, or clear it with
    /// `None`. Declarations Div cannot use are dropped, as Chromium drops
    /// them, and kept for [`Engine::dropped_style_declarations`].
    pub(crate) fn sync_style_attribute(
        &mut self,
        handle: NodeHandle,
        node: NodeId,
        value: Option<&str>,
    ) -> Result<(), EngineError> {
        let candidate = match value {
            None => None,
            Some(text) => {
                let parsed = div_style::parse_style_attribute(text);
                let mut attribute = StyleAttribute {
                    dropped: parsed.dropped,
                    ..StyleAttribute::default()
                };
                for declaration in parsed.declarations {
                    if !div_style::value_matches_property(declaration.property, &declaration.value)
                    {
                        return Err(EngineError::PropertyType {
                            property: declaration.property,
                        });
                    }
                    let target = if declaration.important {
                        &mut attribute.important
                    } else {
                        &mut attribute.normal
                    };
                    target
                        .try_set_validated(declaration.property, declaration.value)
                        .map_err(|_| EngineError::AllocationFailure)?;
                }
                Some(Box::new(attribute))
            }
        };
        let previous = std::mem::replace(
            &mut self.slots[handle.index as usize].style_attribute,
            candidate,
        );
        if previous.is_none() && self.slots[handle.index as usize].style_attribute.is_none() {
            return Ok(());
        }
        if let Err(error) = self.publish_native_cascade(node) {
            self.slots[handle.index as usize].style_attribute = previous;
            return Err(error);
        }
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Subtree);
        Ok(())
    }

    /// The declarations the element's `style` attribute dropped, in source
    /// order, each with the reason. Chromium drops the same declarations
    /// silently, except where Div lacks a property or value Chromium has.
    pub fn dropped_style_declarations(
        &self,
        handle: NodeHandle,
    ) -> Result<&[DroppedDeclaration], EngineError> {
        self.resolve(handle)?;
        Ok(self.slots[handle.index as usize]
            .style_attribute
            .as_ref()
            .map_or(&[], |attribute| &attribute.dropped))
    }

    fn restyle_document_for_rules(&mut self) -> Result<(), EngineError> {
        self.selector_restyle_pending = false;
        self.publish_native_cascade(self.document.root())?;
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Subtree);
        Ok(())
    }

    /// Restyle what a change to an attribute or to children can affect.
    /// Nothing happens without adopted sheets.
    pub(crate) fn restyle_for_selectors(
        &mut self,
        change: SelectorChange,
    ) -> Result<(), EngineError> {
        if self.rule_set.is_empty() {
            return Ok(());
        }
        match change {
            SelectorChange::Attribute {
                node,
                name,
                old,
                new,
            } => match self
                .rule_set
                .index
                .attribute_positions(name, old, new)
                .scope()
            {
                Some(scope) => self.restyle_scoped(vec![(node, scope)]),
                None => Ok(()),
            },
            SelectorChange::Children(node) => {
                let flags = self.rule_set.flags;
                let root = if flags.relational {
                    self.document.root()
                } else {
                    node
                };
                if root == self.document.root() {
                    return self.restyle_document_for_rules();
                }
                self.publish_native_cascade(root)?;
                self.dirty.hit_test = true;
                self.mark_dirty(InvalidationClass::Subtree);
                Ok(())
            }
        }
    }

    /// Write an attribute the engine keeps itself: a control's `value`, the
    /// `open` and `selected` it mirrors, and its `data-divui-` markers.
    /// `None` removes it. Rules that read the attribute restyle at the next
    /// `update()`.
    pub(crate) fn write_engine_attribute(&mut self, node: NodeId, name: &str, value: Option<&str>) {
        let old = match value {
            Some(value) => {
                let old = self.document.attribute(node, name).map(str::to_owned);
                if old.as_deref() == Some(value) {
                    return;
                }
                self.document.set_attribute(node, name, value);
                old
            }
            None => match self.document.node_mut(node).attributes.remove(name) {
                Some(old) => Some(old),
                None => return,
            },
        };
        if self.rule_set.is_empty() {
            return;
        }
        let scope = self
            .rule_set
            .index
            .attribute_positions(name, old.as_deref(), value)
            .scope();
        if let Some(scope) = scope {
            self.rule_set.pending.push((node, scope));
            self.selector_restyle_pending = true;
            self.dirty.paint = true;
        }
    }

    /// What media queries are evaluated against: the viewport in CSS pixels,
    /// the device scale factor, the color scheme and the reduced-motion
    /// preference, with a desktop mouse until the platform reports other
    /// input devices (RFC 0014).
    pub(crate) fn media_values(&self) -> MediaValues {
        let (width, height) = self.viewport.logical_size();
        MediaValues {
            color_scheme: self.color_scheme,
            reduced_motion: self.reduced_motion,
            ..MediaValues::screen(width, height, self.viewport.device_scale_factor())
        }
    }

    /// An input of media queries changed. If any rule's result flipped, the
    /// document restyles at the next `update()`.
    pub(crate) fn note_media_change(&mut self) {
        if self.rule_set.media_table.is_empty() {
            return;
        }
        let media = self.media_values();
        if self.rule_set.evaluate_media(&media) {
            let root = self.document.root();
            self.rule_set.pending.push((root, RestyleScope::Document));
            self.selector_restyle_pending = true;
            self.dirty.paint = true;
        }
    }

    /// The color scheme media queries see as `prefers-color-scheme`.
    pub fn color_scheme(&self) -> ColorScheme {
        self.color_scheme
    }

    /// Set the color scheme media queries see. Rules whose
    /// `prefers-color-scheme` result changes apply at the next `update()`.
    pub fn set_color_scheme(&mut self, scheme: ColorScheme) {
        if self.color_scheme != scheme {
            self.color_scheme = scheme;
            self.note_media_change();
        }
    }

    /// Interaction or form state changed. Elements whose state bits changed
    /// restyle at the next `update()`.
    pub(crate) fn note_selector_state_change(&mut self) {
        if self.rule_set.state_mask != 0 {
            self.selector_restyle_pending = true;
            self.dirty.paint = true;
        }
    }

    /// The element's state bits that some adopted selector reads.
    pub(crate) fn selector_state_bits(&self, handle: NodeHandle) -> u16 {
        let mask = self.rule_set.state_mask;
        let mut bits = 0;
        for (bit, pseudo) in STATE_PSEUDO_CLASSES.iter().enumerate() {
            if mask & 1 << bit != 0 && self.matches_state_pseudo_class(handle, pseudo) {
                bits |= 1 << bit;
            }
        }
        bits
    }

    /// Record the state bits a slot's style is being resolved with.
    pub(crate) fn note_resolved_selector_state(&mut self, index: u32) {
        if self.rule_set.state_mask != 0 {
            let bits = self.selector_state_bits(self.handle_for_slot(index));
            self.rule_set.state_cache.insert(index, bits);
        }
        if let Some(log) = &mut self.rule_set.restyle_log {
            log.push(index);
        }
    }

    pub(crate) fn apply_pending_selector_restyle(&mut self) -> Result<(), EngineError> {
        if !self.selector_restyle_pending {
            return Ok(());
        }
        let mut candidates = std::mem::take(&mut self.rule_set.pending);
        let mut current = Vec::new();
        let mask = self.rule_set.state_mask;
        if mask != 0 {
            let mut stack = vec![self.document.root()];
            while let Some(node) = stack.pop() {
                if let Some(index) = self.node_slots.get(&node).copied() {
                    let bits = self.selector_state_bits(self.handle_for_slot(index));
                    let changed = match self.rule_set.state_cache.get(&index) {
                        Some(old) => old ^ bits,
                        // Not resolved since the sheets changed.
                        None => mask,
                    };
                    if changed != 0 {
                        let positions = self.rule_set.index.state_positions(changed);
                        if let Some(scope) = positions.scope() {
                            candidates.push((node, scope));
                        }
                    }
                    current.push((index, bits));
                }
                stack.extend(self.document.children(node));
            }
        }
        self.restyle_scoped(candidates)?;
        for (index, bits) in current {
            self.rule_set.state_cache.insert(index, bits);
        }
        self.selector_restyle_pending = false;
        Ok(())
    }

    /// Restyle each candidate element to its scope. An element-scoped
    /// candidate is resolved alone first and takes its subtree along only if
    /// its own style changed. The invalidation classes marked are those of
    /// the properties whose computed values changed.
    fn restyle_scoped(
        &mut self,
        candidates: Vec<(NodeId, RestyleScope)>,
    ) -> Result<(), EngineError> {
        if candidates.is_empty() {
            return Ok(());
        }
        let document_root = self.document.root();
        let mut roots: Vec<NodeId> = Vec::new();
        let mut elements: Vec<NodeId> = Vec::new();
        for (node, scope) in candidates {
            match scope {
                RestyleScope::Document => {
                    roots = vec![document_root];
                    elements.clear();
                    break;
                }
                RestyleScope::Parent => {
                    let parent = self.document.node(node).parent;
                    roots.push(if parent.is_none() { node } else { parent });
                }
                RestyleScope::Subtree => roots.push(node),
                RestyleScope::Element => elements.push(node),
            }
        }
        for node in elements {
            if self.within_any(node, &roots) {
                continue;
            }
            let Some(update) = self.prepare_native_cascade_element(node)? else {
                continue;
            };
            let animated_base_changed = self.slots[update.index as usize]
                .animation_base_style
                .as_ref()
                .is_some_and(|base| *base != update.base);
            if update.style != update.old_style || animated_base_changed {
                roots.push(node);
            } else {
                self.note_resolved_selector_state(update.index);
            }
        }
        let roots: Vec<NodeId> = roots
            .iter()
            .copied()
            .enumerate()
            .filter(|(position, root)| {
                !roots[..*position].contains(root) && !self.strictly_within_any(*root, &roots)
            })
            .map(|(_, root)| root)
            .collect();
        if roots.is_empty() {
            return Ok(());
        }
        let updates = self.prepare_native_cascade_roots(&roots)?;
        let mut classes = Vec::new();
        for update in &updates {
            for class in self.style_change_classes(&update.old_style, &update.style) {
                if !classes.contains(&class) {
                    classes.push(class);
                }
            }
        }
        self.publish_prepared_native_cascade(updates);
        if !classes.is_empty() {
            self.dirty.hit_test = true;
        }
        for class in classes {
            self.mark_dirty(class);
        }
        Ok(())
    }

    /// Whether `node` is one of `roots` or a descendant of one.
    fn within_any(&self, node: NodeId, roots: &[NodeId]) -> bool {
        roots.contains(&node) || self.strictly_within_any(node, roots)
    }

    /// Whether `node` is a descendant of one of `roots`.
    fn strictly_within_any(&self, node: NodeId, roots: &[NodeId]) -> bool {
        let mut current = self.document.node(node).parent;
        while !current.is_none() {
            if roots.contains(&current) {
                return true;
            }
            current = self.document.node(current).parent;
        }
        false
    }

    /// The invalidation classes of the properties whose computed values
    /// differ, none when the styles are equal. To find a difference the
    /// properties do not account for (a pseudo-element style, say), the
    /// changed properties' new values are written into copies of both
    /// styles, which must then be equal; otherwise the change is `Subtree`.
    fn style_change_classes(
        &self,
        old: &ComputedStyle,
        new: &ComputedStyle,
    ) -> Vec<InvalidationClass> {
        if old == new {
            return Vec::new();
        }
        let viewport = (
            self.viewport.logical_width() as f32,
            self.viewport.logical_height() as f32,
        );
        let mut classes = Vec::new();
        let (mut from_old, mut from_new) = (old.clone(), new.clone());
        for &property in longhand_properties() {
            let value = div_style::value_from_computed(new, property);
            if div_style::value_from_computed(old, property) == value {
                continue;
            }
            let class = property.metadata().invalidation;
            if !classes.contains(&class) {
                classes.push(class);
            }
            for style in [&mut from_old, &mut from_new] {
                if div_style::apply_to_computed(style, property, &value, viewport).is_err() {
                    return vec![InvalidationClass::Subtree];
                }
            }
        }
        from_old.resolve_current_color_dependencies();
        from_new.resolve_current_color_dependencies();
        if from_old != from_new || classes.is_empty() {
            return vec![InvalidationClass::Subtree];
        }
        classes
    }

    /// Start recording the elements selector restyles resolve, for restyle
    /// scope tests. [`Engine::take_selector_restyles`] returns them.
    #[doc(hidden)]
    pub fn record_selector_restyles(&mut self) {
        self.rule_set.restyle_log = Some(Vec::new());
    }

    /// The elements selector restyles resolved since recording started or
    /// since the last call, in the order they were resolved.
    #[doc(hidden)]
    pub fn take_selector_restyles(&mut self) -> Vec<NodeHandle> {
        let log = match &mut self.rule_set.restyle_log {
            Some(log) => std::mem::take(log),
            None => Vec::new(),
        };
        log.into_iter()
            .map(|index| self.handle_for_slot(index))
            .collect()
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
        if let Some(name) = data.local_name() {
            candidates.extend(rules.by_tag.get(name).into_iter().flatten());
        }
        let mut matched: Vec<Matched> = Vec::new();
        for reference in candidates {
            let rule = &rules.sheets[reference.sheet as usize].rules[reference.rule as usize];
            if !rules.media_applies(rule) {
                continue;
            }
            let (selector, specificity) = &rule.selectors[reference.selector as usize];
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
        if self.rule_set.is_empty()
            && slot.presentational_hints.contributions().is_empty()
            && slot.style_attribute.is_none()
        {
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
        if let Some(attribute) = &slot.style_attribute {
            apply(&attribute.normal, &mut out, None)?;
        }
        apply(&slot.authored, &mut out, None)?;
        for m in ordered(CascadeOrigin::Author, true) {
            apply(&rule(m).important, &mut out, Some(&mut important))?;
        }
        if let Some(attribute) = &slot.style_attribute {
            apply(&attribute.important, &mut out, Some(&mut important))?;
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
