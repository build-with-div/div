//! Permanent declaration-free source, distinct from effect presentation/cache.
use crate::{Engine, EngineError};
use div_dom::NodeId;
use div_style::{
    AuthoredStyleDeclarations, CanonicalStyleFields, ComputedStyle, Display, Style, StyleProperty,
    StyleValue,
};
use std::collections::{HashMap, HashSet};

pub(crate) struct CascadeUpdate {
    node: NodeId,
    pub(crate) index: u32,
    pub(crate) old_style: ComputedStyle,
    pub(crate) base: ComputedStyle,
    pub(crate) style: ComputedStyle,
}

#[derive(Clone)]
pub(crate) enum NativeStyleSource {
    /// Constructor/engine inputs inherit through the ordinary native cascade.
    Defaults(ComputedStyle),
    /// An explicitly supplied computed input retains its snapshot policy.
    Snapshot(ComputedStyle),
}

impl NativeStyleSource {
    pub(crate) fn style(&self) -> &ComputedStyle {
        match self {
            Self::Defaults(style) | Self::Snapshot(style) => style,
        }
    }

    pub(crate) fn style_mut(&mut self) -> &mut ComputedStyle {
        match self {
            Self::Defaults(style) | Self::Snapshot(style) => style,
        }
    }

    pub(crate) fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot(_))
    }
}

impl Engine {
    /// Resolve the affected physical and origin-owned dependency closure before
    /// publishing any style. No
    /// callback runs here. A type/reservation error leaves document styles and
    /// effect caches unchanged so source mutators can roll back their input.
    pub(crate) fn publish_native_cascade(&mut self, root: NodeId) -> Result<(), EngineError> {
        let updates = self.prepare_native_cascade(root)?;
        self.publish_prepared_native_cascade(updates);
        Ok(())
    }

    /// Resolve the same cascade without publishing it. Timeline controls stage
    /// every affected root before committing an instance or any style/events.
    pub(crate) fn prepare_native_cascade(
        &self,
        root: NodeId,
    ) -> Result<Vec<CascadeUpdate>, EngineError> {
        self.prepare_native_cascade_roots(&[root])
    }

    pub(crate) fn prepare_native_cascade_roots(
        &self,
        roots: &[NodeId],
    ) -> Result<Vec<CascadeUpdate>, EngineError> {
        self.prepare_native_cascade_nodes(roots, true)
    }

    /// Resolve one element against its parent's published style, without
    /// its descendants. `None` for a node without a slot.
    pub(crate) fn prepare_native_cascade_element(
        &self,
        node: NodeId,
    ) -> Result<Option<CascadeUpdate>, EngineError> {
        Ok(self.prepare_native_cascade_nodes(&[node], false)?.pop())
    }

    fn prepare_native_cascade_nodes(
        &self,
        roots: &[NodeId],
        descend: bool,
    ) -> Result<Vec<CascadeUpdate>, EngineError> {
        let initial = ComputedStyle::initial();
        let viewport = (
            self.viewport.logical_width() as f32,
            self.viewport.logical_height() as f32,
        );
        let mut pending: Vec<_> = roots.iter().rev().copied().collect();
        let mut updates: Vec<CascadeUpdate> = Vec::new();
        let mut staged: HashMap<NodeId, usize> = HashMap::new();
        let mut visited = HashSet::new();
        while let Some(node) = pending.pop() {
            if visited.contains(&node) {
                continue;
            }
            visited
                .try_reserve(1)
                .map_err(|_| EngineError::AllocationFailure)?;
            visited.insert(node);
            if let Some(index) = self.node_slots.get(&node).copied() {
                let slot = &self.slots[index as usize];
                let parent_node = self
                    .scroll_buttons
                    .bindings
                    .get(&self.handle_for_slot(index))
                    .and_then(|binding| self.resolve(binding.origin).ok())
                    .unwrap_or(self.document.node(node).parent);
                let parent = if parent_node.is_none() {
                    initial.clone()
                } else if let Some(position) = staged.get(&parent_node) {
                    updates[*position].style.clone()
                } else {
                    self.document.node(parent_node).style.clone()
                };
                let root_font = if node == self.document.root() {
                    initial.font_size
                } else if let Some(position) = staged.get(&self.document.root()) {
                    updates[*position].style.font_size
                } else {
                    self.document.node(self.document.root()).style.font_size
                };
                let mut source = slot.native_source.style().clone();
                source.set_raster_context(
                    self.raster_configuration,
                    self.viewport.device_scale_factor(),
                );
                if !slot.native_source.is_snapshot() {
                    source.inherit_properties_from(&parent);
                }
                let cascaded = self.cascaded_declarations(index)?;
                let base = self.resolve_native_declarations(
                    source,
                    cascaded.declarations.contributions(),
                    &parent,
                    root_font,
                    viewport,
                    node == self.document.root(),
                )?;
                // Animations rank below `!important` declarations.
                let font_effects: Vec<_> = self
                    .animation_values_for_node(index, &base, &parent, root_font, viewport, true)
                    .into_iter()
                    .filter(|(property, _)| {
                        !cascaded
                            .important
                            .intersects(CanonicalStyleFields::for_property(*property))
                    })
                    .collect();
                let mut style = if font_effects.is_empty() {
                    base.clone()
                } else {
                    let mut declarations = cascaded.declarations.clone().into_owned();
                    for (property, value) in font_effects {
                        declarations
                            .try_set_validated(property, value)
                            .map_err(|_| EngineError::AllocationFailure)?;
                    }
                    self.resolve_native_declarations(
                        base.clone(),
                        declarations.contributions(),
                        &parent,
                        root_font,
                        viewport,
                        node == self.document.root(),
                    )?
                };
                for (property, value) in self
                    .animation_values_for_node(index, &style, &parent, root_font, viewport, false)
                {
                    if cascaded
                        .important
                        .intersects(CanonicalStyleFields::for_property(property))
                    {
                        continue;
                    }
                    div_style::apply_to_computed(&mut style, property, &value, viewport)
                        .map_err(|_| EngineError::PropertyType { property })?;
                }
                style.resolve_current_color_dependencies();
                self.adjust_native_control_style(index, node, &mut style);
                let pseudo_root_font = if node == self.document.root() {
                    style.font_size
                } else {
                    root_font
                };
                let style =
                    self.resolve_authored_pseudo_styles(index, style, pseudo_root_font, viewport)?;
                updates
                    .try_reserve(1)
                    .map_err(|_| EngineError::AllocationFailure)?;
                staged
                    .try_reserve(1)
                    .map_err(|_| EngineError::AllocationFailure)?;
                staged.insert(node, updates.len());
                updates.push(CascadeUpdate {
                    node,
                    index,
                    old_style: self.document.node(node).style.clone(),
                    base,
                    style,
                });
            }
            if !descend {
                continue;
            }
            let children: Vec<_> = self.document.children(node).collect();
            let owned_buttons = self.owned_buttons(node);
            pending
                .try_reserve(children.len() + owned_buttons.len())
                .map_err(|_| EngineError::AllocationFailure)?;
            pending.extend(children.into_iter().rev());
            // Buttons inherit from their origin even when their retained boxes
            // are siblings or detached. Visit them after staging that origin;
            // the visited set removes their duplicate physical-tree entries.
            pending.extend(owned_buttons.into_iter().rev());
        }
        Ok(updates)
    }

    pub(crate) fn publish_prepared_native_cascade(&mut self, updates: Vec<CascadeUpdate>) {
        for update in updates {
            self.note_resolved_selector_state(update.index);
            self.invalidate_transform_containing_block_change(
                update.node,
                update.old_style.has_transform_containing_block(),
                &update.style,
            );
            let changed_properties = self.slots[update.index as usize]
                .animation_properties
                .clone();
            for id in changed_properties {
                let property = StyleProperty::from_u16(id).expect("animated property");
                if div_style::value_from_computed(&update.old_style, property)
                    != div_style::value_from_computed(&update.style, property)
                {
                    self.dirty.hit_test = true;
                    self.mark_dirty(property.metadata().invalidation);
                }
            }
            let has_effect_owner = self
                .animations
                .values()
                .any(|effect| effect.target == self.handle_for_slot(update.index));
            let slot = &mut self.slots[update.index as usize];
            if slot.animation_base_style.is_some() {
                slot.animation_base_style = has_effect_owner.then_some(update.base);
                if !has_effect_owner {
                    slot.animation_properties.clear();
                }
            }
            self.document
                .install_resolved_style(update.node, update.style);
        }
    }

    /// This is a creation input, never an authored declaration. Alias/native
    /// constructors call it before exposing a handle or starting any effects.
    pub(crate) fn set_native_display_default(
        &mut self,
        handle: crate::NodeHandle,
        display: Display,
    ) -> Result<(), EngineError> {
        let node = self.resolve(handle)?;
        let slot = &mut self.slots[handle.index as usize];
        debug_assert!(slot.authored.contributions().is_empty());
        debug_assert!(slot.animation_base_style.is_none());
        slot.native_source
            .style_mut()
            .update_derived(|style| style.display = display);
        // Native control adjustments remain derived, but must be applied
        // before a native constructor exposes the handle (for example textarea).
        self.publish_native_cascade(node)
    }

    pub(crate) fn canonical_declarations(
        declarations: &Style,
    ) -> Result<AuthoredStyleDeclarations, EngineError> {
        let mut canonical = AuthoredStyleDeclarations::default();
        for declaration in declarations.declarations() {
            if !div_style::value_matches_property(declaration.property, &declaration.value) {
                return Err(EngineError::PropertyType {
                    property: declaration.property,
                });
            }
            canonical
                .try_set_validated(declaration.property, declaration.value.clone())
                .map_err(|_| EngineError::AllocationFailure)?;
        }
        Ok(canonical)
    }

    /// Build the unanimated native cascade without using current presentation
    /// as the source. Explicit snapshot inputs keep their inheritance policy.
    pub(crate) fn unanimated_style(
        &self,
        node: NodeId,
        index: u32,
    ) -> Result<ComputedStyle, EngineError> {
        let cascaded = self.cascaded_declarations(index)?;
        self.resolve_native_source(node, index, cascaded.declarations.contributions())
    }

    /// The style a renderer snapshot freezes: the native source and the
    /// slot's own declarations (`set_property`), resolved as the cascade
    /// resolves them. Declarations from matched stylesheet rules,
    /// presentational hints and the `style` attribute are left out: they stay
    /// live and cascade above the snapshot, so one whose rule stops matching
    /// (a closed dialog's `display: none`, for example) leaves nothing behind.
    pub(crate) fn snapshot_base_style(
        &self,
        node: NodeId,
        index: u32,
    ) -> Result<ComputedStyle, EngineError> {
        let slot = &self.slots[index as usize];
        self.resolve_native_source(node, index, slot.authored.contributions())
    }

    /// The native source with `declarations` resolved over it, against the
    /// node's style parent, the root font and the viewport.
    fn resolve_native_source(
        &self,
        node: NodeId,
        index: u32,
        declarations: &[div_style::AuthoredStyleContribution],
    ) -> Result<ComputedStyle, EngineError> {
        let initial = ComputedStyle::initial();
        let slot = &self.slots[index as usize];
        let parent_node = self
            .scroll_buttons
            .bindings
            .get(&self.handle_for_slot(index))
            .and_then(|binding| self.resolve(binding.origin).ok())
            .unwrap_or(self.document.node(node).parent);
        let parent = if parent_node.is_none() {
            initial.clone()
        } else {
            self.document.node(parent_node).style.clone()
        };
        let root_font = if node == self.document.root() {
            initial.font_size
        } else {
            self.document.node(self.document.root()).style.font_size
        };
        let mut style = slot.native_source.style().clone();
        style.set_raster_context(
            self.raster_configuration,
            self.viewport.device_scale_factor(),
        );
        if !slot.native_source.is_snapshot() {
            style.inherit_properties_from(&parent);
        }
        self.resolve_native_declarations(
            style,
            declarations,
            &parent,
            root_font,
            (
                self.viewport.logical_width() as f32,
                self.viewport.logical_height() as f32,
            ),
            node == self.document.root(),
        )
    }

    /// Validate before reserving/committing ownership. The returned candidate
    /// is unpublished; an error leaves the Slot and document untouched.
    pub(crate) fn staged_authored_property(
        &self,
        handle: crate::NodeHandle,
        property: StyleProperty,
        value: StyleValue,
    ) -> Result<Option<AuthoredStyleDeclarations>, EngineError> {
        self.resolve(handle)?;
        if !div_style::value_matches_property(property, &value) {
            return Err(EngineError::PropertyType { property });
        }
        let mut candidate = self.slots[handle.index as usize].authored.clone();
        if !candidate
            .try_set_validated(property, value)
            .map_err(|_| EngineError::AllocationFailure)?
        {
            return Ok(None);
        }
        Ok(Some(candidate))
    }
}
