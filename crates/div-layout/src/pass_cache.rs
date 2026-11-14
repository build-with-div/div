//! Per-pass memo of flex item layouts and intrinsic block sizes.
//!
//! A column flex container lays each item out once to find its
//! content-based flex basis and again at its final size, and its intrinsic
//! sizing asks for each item's intrinsic block sizes more than once. Chromium
//! keeps layout results on the box (`LayoutBox::CachedLayoutResult`), and it
//! measures block sizes by layout, so a nested item whose constraints are
//! unchanged is laid out once. Div kept no such results, so each level of
//! nested column containers doubled the work below it: six nested columns
//! laid their innermost items out 32 times.
//!
//! The memo lives while the outermost flex container on this thread is being
//! laid out. The document is borrowed immutably for that whole time, and both
//! computations are pure functions of the document, the box and the inputs
//! in their keys, so a hit returns exactly what the computation would return
//! again. Spaces that carry shared mutable state (line-clamp and first-line
//! contexts), float exclusions or fragmentation, and documents seen through
//! a style-overriding context, are never cached.
use crate::constraint_space::ConstraintSpace;
use crate::fragment::Fragment;
use crate::intrinsic_sizing::IntrinsicSizes;
use crate::layout_context::LayoutDocument;
use div_dom::{Document, NodeId};
use div_geometry::{BfcOffset, BoxStrut, LayoutUnit, WritingDirectionMode};
use std::cell::RefCell;
use std::collections::HashMap;

/// The parts of a [`ConstraintSpace`] a layout reads, for spaces with no
/// shared state.
#[derive(Clone, PartialEq)]
struct SpaceKey {
    element_scrollbars: Option<(NodeId, crate::ElementScrollbars)>,
    element_used_border: Option<(NodeId, BoxStrut)>,
    writing_direction: WritingDirectionMode,
    available_inline_size: LayoutUnit,
    available_block_size: LayoutUnit,
    percentage_resolution_inline_size: LayoutUnit,
    percentage_resolution_block_size: LayoutUnit,
    bfc_offset: BfcOffset,
    floats_bfc_block_offset: Option<LayoutUnit>,
    is_new_formatting_context: bool,
    is_fixed_inline_size: bool,
    is_fixed_block_size: bool,
    stretch_inline_size: bool,
    stretch_block_size: bool,
    is_initial_block_size_indefinite: bool,
    needs_first_baseline: bool,
    needs_last_baseline: bool,
}

impl SpaceKey {
    fn new(space: &ConstraintSpace) -> Option<Self> {
        // Every field of `ConstraintSpace` is either part of the key or
        // excluded here; destructuring keeps a new field from being missed.
        let ConstraintSpace {
            element_scrollbars,
            element_used_border,
            writing_direction,
            available_inline_size,
            available_block_size,
            percentage_resolution_inline_size,
            percentage_resolution_block_size,
            bfc_offset,
            floats_bfc_block_offset,
            exclusion_space,
            is_new_formatting_context,
            is_fixed_inline_size,
            is_fixed_block_size,
            stretch_inline_size,
            stretch_block_size,
            is_initial_block_size_indefinite,
            fragmentainer_block_size,
            outer_fragmentainer_block_size,
            block_offset_in_fragmentainer,
            is_resuming,
            needs_first_baseline,
            needs_last_baseline,
            line_clamp_context,
            first_line_context,
        } = space;
        if exclusion_space.is_some()
            || line_clamp_context.is_some()
            || first_line_context.is_some()
            || *fragmentainer_block_size != LayoutUnit::zero()
            || *outer_fragmentainer_block_size != LayoutUnit::zero()
            || *block_offset_in_fragmentainer != LayoutUnit::zero()
            || *is_resuming
        {
            return None;
        }
        Some(Self {
            element_scrollbars: *element_scrollbars,
            element_used_border: *element_used_border,
            writing_direction: *writing_direction,
            available_inline_size: *available_inline_size,
            available_block_size: *available_block_size,
            percentage_resolution_inline_size: *percentage_resolution_inline_size,
            percentage_resolution_block_size: *percentage_resolution_block_size,
            bfc_offset: *bfc_offset,
            floats_bfc_block_offset: *floats_bfc_block_offset,
            is_new_formatting_context: *is_new_formatting_context,
            is_fixed_inline_size: *is_fixed_inline_size,
            is_fixed_block_size: *is_fixed_block_size,
            stretch_inline_size: *stretch_inline_size,
            stretch_block_size: *stretch_block_size,
            is_initial_block_size_indefinite: *is_initial_block_size_indefinite,
            needs_first_baseline: *needs_first_baseline,
            needs_last_baseline: *needs_last_baseline,
        })
    }
}

type BoxId = (*const Document, NodeId);

#[derive(Default)]
struct Memo {
    flex_items: HashMap<BoxId, Vec<(SpaceKey, Fragment)>>,
    intrinsic_block_sizes: HashMap<BoxId, Vec<(Option<BoxStrut>, IntrinsicSizes)>>,
}

thread_local! {
    static MEMO: RefCell<Option<Memo>> = const { RefCell::new(None) };
    #[cfg(test)]
    static COMPUTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Keeps the memo alive while the outermost flex container is laid out.
pub(crate) struct Scope {
    owner: bool,
}

pub(crate) fn enter() -> Scope {
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        let owner = memo.is_none();
        if owner {
            *memo = Some(Memo::default());
        }
        Scope { owner }
    })
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.owner {
            MEMO.with(|memo| *memo.borrow_mut() = None);
        }
    }
}

/// The box in the plain document, when `doc` is one. Wrapping contexts
/// override styles or children, so their results are not the document's.
fn plain_box(doc: &dyn LayoutDocument, node: NodeId) -> Option<BoxId> {
    let source: *const Document = doc.source_document();
    std::ptr::eq(
        doc as *const dyn LayoutDocument as *const u8,
        source as *const u8,
    )
    .then_some((source, node))
}

/// Lays a flex item out through the memo when one is active.
pub(crate) fn flex_item(
    doc: &dyn LayoutDocument,
    node: NodeId,
    space: &ConstraintSpace,
    layout: impl FnOnce() -> Fragment,
) -> Fragment {
    let Some((id, space_key)) =
        plain_box(doc, node).and_then(|id| Some((id, SpaceKey::new(space)?)))
    else {
        return computed(layout);
    };
    let hit = MEMO.with(|memo| {
        let memo = memo.borrow();
        let entries = memo.as_ref()?.flex_items.get(&id)?;
        entries
            .iter()
            .find(|(key, _)| *key == space_key)
            .map(|(_, fragment)| fragment.clone())
    });
    if let Some(fragment) = hit {
        return fragment;
    }
    let fragment = computed(layout);
    MEMO.with(|memo| {
        if let Some(memo) = memo.borrow_mut().as_mut() {
            memo.flex_items
                .entry(id)
                .or_default()
                .push((space_key, fragment.clone()));
        }
    });
    fragment
}

/// Computes a box's intrinsic block sizes through the memo when one is
/// active.
pub(crate) fn intrinsic_block_sizes(
    doc: &dyn LayoutDocument,
    node: NodeId,
    used_border: Option<BoxStrut>,
    compute: impl FnOnce() -> IntrinsicSizes,
) -> IntrinsicSizes {
    let Some(id) = plain_box(doc, node) else {
        return computed(compute);
    };
    let hit = MEMO.with(|memo| {
        let memo = memo.borrow();
        let entries = memo.as_ref()?.intrinsic_block_sizes.get(&id)?;
        entries
            .iter()
            .find(|(border, _)| *border == used_border)
            .map(|(_, sizes)| *sizes)
    });
    if let Some(sizes) = hit {
        return sizes;
    }
    let sizes = computed(compute);
    MEMO.with(|memo| {
        if let Some(memo) = memo.borrow_mut().as_mut() {
            memo.intrinsic_block_sizes
                .entry(id)
                .or_default()
                .push((used_border, sizes));
        }
    });
    sizes
}

fn computed<T>(compute: impl FnOnce() -> T) -> T {
    #[cfg(test)]
    COMPUTED.with(|count| count.set(count.get() + 1));
    compute()
}

#[cfg(test)]
mod tests {
    use super::COMPUTED;
    use crate::ConstraintSpace;
    use div_dom::{Document, ElementTag, NodeId};
    use div_geometry::{LayoutUnit, Length};
    use div_style::{Display, FlexDirection};

    /// `depth` nested `display: flex; flex-direction: column` containers, each
    /// `flex-grow: 1; min-height: 0`, around `rows` 18px block rows: the
    /// sample app's list shape.
    fn nested_columns(depth: usize, rows: usize) -> (Document, NodeId) {
        let mut doc = Document::new();
        let outer = doc.create_node(ElementTag::Div);
        doc.update_resolved_style(outer, |style| {
            style.update_derived(|computed| {
                computed.display = Display::Flex;
                computed.flex_direction = FlexDirection::Column;
                computed.width = Length::px(800.0);
                computed.height = Length::px(600.0);
            })
        });
        doc.append_child(doc.root(), outer);
        let mut parent = outer;
        for _ in 0..depth {
            let column = doc.create_node(ElementTag::Div);
            doc.update_resolved_style(column, |style| {
                style.update_derived(|computed| {
                    computed.display = Display::Flex;
                    computed.flex_direction = FlexDirection::Column;
                    computed.flex_grow = 1.0;
                    computed.min_height = Length::px(0.0);
                })
            });
            doc.append_child(parent, column);
            parent = column;
        }
        for _ in 0..rows {
            let row = doc.create_node(ElementTag::Div);
            doc.update_resolved_style(row, |style| {
                style.update_derived(|computed| {
                    computed.display = Display::Block;
                    computed.height = Length::px(18.0);
                })
            });
            doc.append_child(parent, row);
        }
        (doc, outer)
    }

    /// Item layouts and intrinsic block sizings computed for one layout.
    fn computations(depth: usize) -> usize {
        let (doc, outer) = nested_columns(depth, 4);
        let space = ConstraintSpace::for_root(LayoutUnit::from_i32(800), LayoutUnit::from_i32(600));
        let before = COMPUTED.with(std::cell::Cell::get);
        let fragment = crate::flex::flex_layout(&doc, outer, &space);
        assert_eq!(fragment.height(), LayoutUnit::from_i32(600));
        COMPUTED.with(std::cell::Cell::get) - before
    }

    /// Past the first level, whose container is the definite-height outer
    /// one, each nested column adds the same number of computations. Before
    /// the memo the count roughly doubled with every level.
    #[test]
    fn nested_column_containers_cost_linear_work() {
        let counts: Vec<usize> = (1..=8).map(computations).collect();
        let steps: Vec<usize> = counts.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            steps[1..].windows(2).all(|pair| pair[0] == pair[1]),
            "computations by depth 1 to 8: {counts:?}"
        );
    }
}
