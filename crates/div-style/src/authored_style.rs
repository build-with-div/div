//! Lossless author inputs and canonical longhand ownership.
//!
//! These helpers store source values, never computed snapshots. The Engine
//! supplies the permanent native source baseline and resolves each surviving
//! input in its current font/viewport/color context before projection.
use crate::{
    apply_to_computed, ComputedStyle, CssWideKeyword, CustomDeclaration, PropertyTypeError,
    StyleProperty, StyleValue,
};
use std::collections::TryReserveError;
use std::sync::OnceLock;

include!("generated_authored_fields.rs");

/// Internal ownership set. Compatibility summaries are derived, not owners.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalStyleFields([u64; CANONICAL_STYLE_FIELD_WORDS]);

impl CanonicalStyleFields {
    pub const EMPTY: Self = Self([0; CANONICAL_STYLE_FIELD_WORDS]);

    pub fn for_property(property: StyleProperty) -> Self {
        let mut fields = Self::EMPTY;
        for field in property.canonical_field_groups() {
            let index = *field as usize;
            fields.0[index / 64] |= 1 << (index % 64);
        }
        fields
    }

    pub fn contains(self, field: CanonicalStyleField) -> bool {
        let index = field as usize;
        self.0[index / 64] & (1 << (index % 64)) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    pub fn union(self, other: Self) -> Self {
        let mut fields = self;
        for (word, other) in fields.0.iter_mut().zip(other.0) {
            *word |= other;
        }
        fields
    }

    pub fn intersection(self, other: Self) -> Self {
        let mut fields = self;
        for (word, other) in fields.0.iter_mut().zip(other.0) {
            *word &= other;
        }
        fields
    }

    /// The fields of inherited properties.
    fn inherited() -> Self {
        static INHERITED: OnceLock<CanonicalStyleFields> = OnceLock::new();
        *INHERITED.get_or_init(|| {
            crate::PROPERTY_METADATA
                .iter()
                .filter(|metadata| metadata.inherited)
                .fold(Self::EMPTY, |fields, metadata| {
                    fields.union(Self::for_property(metadata.property))
                })
        })
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0
            .iter()
            .zip(other.0)
            .any(|(word, other)| word & other != 0)
    }

    fn subtract(&mut self, removed: Self) -> bool {
        let before = *self;
        for (word, remove) in self.0.iter_mut().zip(removed.0) {
            *word &= !remove;
        }
        *self != before
    }

    fn can_override(self, other: Self) -> bool {
        if self.intersects(other) {
            return true;
        }
        CANONICAL_STYLE_FIELDS.iter().copied().any(|field| {
            self.contains(field) && field.dependent_mutation_groups().intersects(other)
        })
    }
}

/// One original typed value and the parts that remain effective.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredStyleContribution {
    property: StyleProperty,
    value: StyleValue,
    fields: CanonicalStyleFields,
}

impl AuthoredStyleContribution {
    pub fn property(&self) -> StyleProperty {
        self.property
    }

    pub fn value(&self) -> &StyleValue {
        &self.value
    }

    pub fn fields(&self) -> CanonicalStyleFields {
        self.fields
    }

    /// The same live fields of the same property, with another value: the
    /// typed result of substituting `var()`.
    pub fn with_value(&self, value: StyleValue) -> Self {
        Self {
            property: self.property,
            value,
            fields: self.fields,
        }
    }

    /// Apply a CSS-wide keyword to the live fields. `inherit` copies the
    /// parent's value, `initial` the initial one, and `unset` whichever the
    /// property's inheritance calls for. `revert` and `revert-layer` leave
    /// the native source the engine started from: the cascade has already
    /// replaced them with the earlier origin's or layer's declarations
    /// wherever there were any.
    pub fn project_wide(
        &self,
        style: &mut ComputedStyle,
        keyword: CssWideKeyword,
        parent: &ComputedStyle,
    ) {
        if matches!(
            keyword,
            CssWideKeyword::Revert | CssWideKeyword::RevertLayer
        ) {
            return;
        }
        let initial = ComputedStyle::initial();
        let inherited = CanonicalStyleFields::inherited();
        for field in CANONICAL_STYLE_FIELDS {
            if !self.fields.contains(*field) {
                continue;
            }
            let source = match keyword {
                CssWideKeyword::Inherit => parent,
                CssWideKeyword::Unset if inherited.contains(*field) => parent,
                _ => &initial,
            };
            field.copy_from(style, source);
        }
        style.fields.white_space = crate::property::normalized_white_space(
            style.fields.white_space_collapse,
            style.fields.text_wrap_mode,
        );
        style.fields.text_wrap = crate::property::normalized_text_wrap(
            style.fields.text_wrap_mode,
            style.fields.text_wrap_style,
        );
        style.resolve_current_color_dependencies();
    }

    /// Apply an already context-resolved value without reviving masked fields.
    /// Errors leave the supplied style unchanged. The caller retains the raw
    /// `value()` for the next recomposition and publishes effects afterwards.
    pub fn project_resolved(
        &self,
        style: &mut ComputedStyle,
        resolved: &StyleValue,
        viewport: (f32, f32),
    ) -> Result<(), PropertyTypeError> {
        let mut candidate = style.clone();
        apply_to_computed(&mut candidate, self.property, resolved, viewport)?;
        // A surviving paired declaration retains its exact old source presence.
        // Only partial masks materialize one axis around the other live source.
        let corners = [
            [
                CanonicalStyleField::BorderTopLeftRadiusX,
                CanonicalStyleField::BorderTopLeftRadiusY,
            ],
            [
                CanonicalStyleField::BorderTopRightRadiusX,
                CanonicalStyleField::BorderTopRightRadiusY,
            ],
            [
                CanonicalStyleField::BorderBottomRightRadiusX,
                CanonicalStyleField::BorderBottomRightRadiusY,
            ],
            [
                CanonicalStyleField::BorderBottomLeftRadiusX,
                CanonicalStyleField::BorderBottomLeftRadiusY,
            ],
        ];
        let paired = corners.map(|axes| axes.into_iter().all(|axis| self.fields.contains(axis)));
        for (corner, whole) in paired.into_iter().enumerate() {
            if whole {
                style.copy_border_radius_corner_from(&candidate, corner);
            }
        }
        for field in CANONICAL_STYLE_FIELDS {
            if self.fields.contains(*field)
                && !corners
                    .iter()
                    .zip(paired)
                    .any(|(axes, whole)| whole && axes.contains(field))
            {
                field.copy_from(style, &candidate);
            }
        }
        if [
            CanonicalStyleField::WhiteSpaceCollapse,
            CanonicalStyleField::TextWrapMode,
            CanonicalStyleField::TextWrapStyle,
        ]
        .into_iter()
        .any(|field| self.fields.contains(field))
        {
            style.fields.white_space = crate::property::normalized_white_space(
                style.fields.white_space_collapse,
                style.fields.text_wrap_mode,
            );
            style.fields.text_wrap = crate::property::normalized_text_wrap(
                style.fields.text_wrap_mode,
                style.fields.text_wrap_style,
            );
        }
        // Own color is published before dependent ordinary colors, as in the
        // existing Engine cascade. This does not resolve text-fill keywords.
        style.resolve_current_color_dependencies();
        Ok(())
    }
}

/// Ordered, currently live canonical contributions.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuthoredStyleDeclarations {
    contributions: Vec<AuthoredStyleContribution>,
    custom: Vec<CustomDeclaration>,
}

impl AuthoredStyleDeclarations {
    pub fn contributions(&self) -> &[AuthoredStyleContribution] {
        &self.contributions
    }

    /// The winning custom property declaration for each name.
    pub fn custom(&self) -> &[CustomDeclaration] {
        &self.custom
    }

    pub fn is_empty(&self) -> bool {
        self.contributions.is_empty() && self.custom.is_empty()
    }

    /// Whether any declaration is `revert` or `revert-layer`.
    pub fn has_revert(&self) -> bool {
        self.contributions.iter().any(|contribution| {
            matches!(
                contribution.value,
                StyleValue::Wide(CssWideKeyword::Revert | CssWideKeyword::RevertLayer)
            )
        }) || self.custom.iter().any(|declaration| {
            matches!(
                declaration.value().wide_keyword(),
                Some(CssWideKeyword::Revert | CssWideKeyword::RevertLayer)
            )
        })
    }

    /// Set a custom property; a later declaration of a name replaces an
    /// earlier one.
    pub fn try_set_custom(
        &mut self,
        declaration: CustomDeclaration,
    ) -> Result<bool, TryReserveError> {
        if let Some(index) = self
            .custom
            .iter()
            .position(|existing| existing.name() == declaration.name())
        {
            if self.custom[index].value() == declaration.value() {
                return Ok(false);
            }
            self.custom[index] = declaration;
            return Ok(true);
        }
        self.custom.try_reserve(1)?;
        self.custom.push(declaration);
        Ok(true)
    }

    /// Remove a custom property. Returns false if it was not declared.
    pub fn remove_custom(&mut self, name: &str) -> bool {
        let before = self.custom.len();
        self.custom
            .retain(|declaration| declaration.name().as_str() != name);
        self.custom.len() != before
    }

    /// Roll `mask` back to `earlier`, the declarations of the earlier
    /// origins or layers: the masked fields first take the native source
    /// (`revert`), then `earlier`'s declarations for them on top.
    pub fn try_revert_to(
        &mut self,
        property: StyleProperty,
        mask: CanonicalStyleFields,
        earlier: &Self,
    ) -> Result<(), TryReserveError> {
        self.try_set_masked(property, StyleValue::Wide(CssWideKeyword::Revert), mask)?;
        for contribution in &earlier.contributions {
            let fields = contribution.fields.intersection(mask);
            if !fields.is_empty() {
                self.try_set_masked(contribution.property, contribution.value.clone(), fields)?;
            }
        }
        Ok(())
    }

    /// Roll a custom property back to `earlier`'s declaration of it, or to
    /// none.
    pub fn try_revert_custom_to(
        &mut self,
        name: &str,
        earlier: &Self,
    ) -> Result<(), TryReserveError> {
        match earlier
            .custom
            .iter()
            .find(|declaration| declaration.name().as_str() == name)
        {
            Some(declaration) => self.try_set_custom(declaration.clone()).map(|_| ()),
            None => {
                self.remove_custom(name);
                Ok(())
            }
        }
    }

    fn try_set_masked(
        &mut self,
        property: StyleProperty,
        value: StyleValue,
        fields: CanonicalStyleFields,
    ) -> Result<(), TryReserveError> {
        self.contributions.try_reserve(1)?;
        for contribution in &mut self.contributions {
            contribution.fields.subtract(fields);
        }
        self.contributions
            .retain(|contribution| !contribution.fields.is_empty());
        self.contributions.push(AuthoredStyleContribution {
            property,
            value,
            fields,
        });
        Ok(())
    }

    /// Commit an owned input that the Engine has already type-validated.
    /// Reserve before changing ownership. A missing allocation leaves every
    /// older declaration intact; source values keep their original units.
    pub fn try_set_validated(
        &mut self,
        property: StyleProperty,
        value: StyleValue,
    ) -> Result<bool, TryReserveError> {
        let fields = CanonicalStyleFields::for_property(property);
        if let Some(index) = self.contributions.iter().position(|contribution| {
            contribution.property == property
                && contribution.value == value
                && contribution.fields == fields
        }) {
            if !self.contributions[index + 1..]
                .iter()
                .any(|contribution| contribution.fields.can_override(fields))
            {
                return Ok(false);
            }
        }
        self.contributions.try_reserve(1)?;
        for contribution in &mut self.contributions {
            contribution.fields.subtract(fields);
        }
        self.contributions
            .retain(|contribution| !contribution.fields.is_empty());
        self.contributions.push(AuthoredStyleContribution {
            property,
            value,
            fields,
        });
        Ok(true)
    }

    /// Remove the requested canonical set from every older declaration.
    /// A removed longhand never revives an earlier shorthand contribution.
    /// Native computed overrides retain their explicit, separate identities.
    pub fn remove(&mut self, property: StyleProperty) -> bool {
        let fields = CanonicalStyleFields::for_property(property);
        let mut changed = false;
        for contribution in &mut self.contributions {
            changed |= contribution.fields.subtract(fields);
        }
        if changed {
            self.contributions
                .retain(|contribution| !contribution.fields.is_empty());
        }
        changed
    }
}

#[cfg(test)]
#[path = "authored_style_tests.rs"]
mod tests;
