//! Keyboard-focusable scrollers, a press's focus and the sequential focus
//! navigation starting point, as Chromium 147.0.7727.50 behaves.
//!
//! Sources: `third_party/blink/renderer/core/page/focus_controller.cc`
//! (Apple/Nuanti 2-clause BSD) and `core/input/event_handler.cc`,
//! `mouse_event_manager.cc` (Apple 2-clause BSD, Chromium BSD). The rules
//! in `element.cc` and `document.cc` (LGPL, not read) were measured instead,
//! by the oracles `tests/fidelity/fixtures/focus-navigation-v1` and
//! `keyboard-scroll-v1`:
//!
//! - A scroller is focusable when it can scroll on an axis that overflows:
//!   `overflow: auto` or `scroll` on that axis, and scrollable overflow
//!   beyond the client rectangle there, by any amount (60.4px of content in
//!   60px counts, though its snapped scroll range is 0). A scroller whose
//!   content fits, one with `overflow: hidden`, and one that overflows only
//!   on a hidden axis are not.
//! - Such a scroller without `tabindex` is in the sequential order only when
//!   no descendant is (`focus_controller.cc`: "a scroller with focusable
//!   children" supports focus but is not keyboard focusable). A descendant
//!   with `tabindex="-1"`, a disabled one and a `display: none` one do not
//!   count. Script focus reaches it either way; a click never does.
//! - A press focuses the nearest mouse-focusable element from its target
//!   outwards, or clears focus when there is none, unless it hit a
//!   scrollbar (`MouseEventManager::HandleMouseFocus`).
//! - Every press sets the starting point to its target before `mousedown`
//!   (`EventHandler::HandleMousePressEvent`), and every focus sets it to the
//!   focused element; `blur()` keeps it. Navigation starts from the focused
//!   element, else from the starting point
//!   (`FocusController::AdvanceFocusInDocumentOrder`). When the starting
//!   node or an ancestor is removed, the start is where it was: Tab goes to
//!   the next element after that position in tree order and Shift+Tab to the
//!   one before it.

use crate::{Engine, EngineError, NodeHandle};
use div_dom::{ElementTag, NodeId};

/// The sequential focus navigation starting point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusStart {
    Node(NodeHandle),
    /// Where a removed starting node was: among `parent`'s children, before
    /// `next`, or at their end.
    Position {
        parent: NodeHandle,
        next: Option<NodeHandle>,
    },
}

/// What a primary press does to focus, as Blink's
/// `MouseEventManager::HandleMouseFocus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressFocus {
    /// Focus this element, the nearest mouse-focusable one from the target.
    Focus(NodeHandle),
    /// No element can take the focus: clear it.
    Clear,
    /// The press hit a scrollbar and no element can take the focus: keep it.
    Keep,
}

impl Engine {
    /// A scroller that can scroll on an axis whose scrollable overflow
    /// extends past its client rectangle, unsnapped. The viewport is not
    /// one. Reads the latest layout; callers that need it fresh update
    /// first.
    pub(crate) fn is_focusable_scroller(&self, handle: NodeHandle) -> bool {
        let Ok(node) = self.resolve(handle) else {
            return false;
        };
        if node == self.document.root() || Some(node) == self.document.document_element() {
            return false;
        }
        let Some(area) = self.layout_scroll_area(node) else {
            return false;
        };
        if !area.is_scroll_container() {
            return false;
        }
        let (content, client) = (area.content_rect, area.client_rect);
        let overflows_x =
            content.x() < client.x() || content.x() + content.width() > client.x() + client.width();
        let overflows_y = content.y() < client.y()
            || content.y() + content.height() > client.y() + client.height();
        (area.overflow_x.is_scrollable() && overflows_x)
            || (area.overflow_y.is_scrollable() && overflows_y)
    }

    pub(crate) fn tabindex(&self, handle: NodeHandle) -> Option<i32> {
        self.attribute(handle, "tabindex")
            .ok()
            .flatten()
            .and_then(|value| value.parse::<i32>().ok())
    }

    /// Focusable for a reason other than scrolling: `tabindex`, a form
    /// control or `summary`.
    pub fn has_focus_semantics(&self, handle: NodeHandle) -> bool {
        self.tabindex(handle).is_some()
            || self.controls.contains_key(&handle.index)
            || self.element_tag(handle) == Ok(ElementTag::Summary)
    }

    /// In the sequential focus order.
    pub(crate) fn is_keyboard_focusable(&self, handle: NodeHandle) -> bool {
        if !self.is_focusable(handle) {
            return false;
        }
        if let Some(tabindex) = self.tabindex(handle) {
            return tabindex >= 0;
        }
        self.has_focus_semantics(handle) || !self.has_keyboard_focusable_descendant(handle)
    }

    /// One walk, linear in the subtree: a focusable descendant without a
    /// negative `tabindex` is either keyboard focusable itself or a scroller
    /// with a keyboard-focusable descendant, so either way the answer is yes.
    fn has_keyboard_focusable_descendant(&self, handle: NodeHandle) -> bool {
        let mut stack = self.presentation_children(handle).unwrap_or_default();
        while let Some(node) = stack.pop() {
            if self.is_focusable(node) && !self.tabindex(node).is_some_and(|value| value < 0) {
                return true;
            }
            stack.extend(self.presentation_children(node).unwrap_or_default());
        }
        false
    }

    /// A click can focus it: a scroller focusable only by scrolling cannot.
    pub(crate) fn is_mouse_focusable(&self, handle: NodeHandle) -> bool {
        self.is_focusable(handle) && self.has_focus_semantics(handle)
    }

    /// The focus change of a primary press at (`x`, `y`), in CSS pixels of
    /// the viewport, on `target`, for a `mousedown` that was not cancelled.
    pub fn press_focus(
        &mut self,
        target: NodeHandle,
        x: f32,
        y: f32,
    ) -> Result<PressFocus, EngineError> {
        self.update()?;
        let mut current = Some(target);
        while let Some(handle) = current {
            if self.is_mouse_focusable(handle) {
                return Ok(PressFocus::Focus(handle));
            }
            current = self.parent(handle)?;
        }
        Ok(if self.hits_scrollbar(target, x, y)? {
            PressFocus::Keep
        } else {
            PressFocus::Clear
        })
    }

    /// Whether (`x`, `y`) is on `target`'s scrollbars or scroll corner: in
    /// its padding box, outside its client rectangle. Transforms are not
    /// mapped.
    fn hits_scrollbar(&self, target: NodeHandle, x: f32, y: f32) -> Result<bool, EngineError> {
        let node = self.resolve(target)?;
        let Some(fragment) = self
            .latest_fragment
            .as_ref()
            .and_then(|root| root.find_scroll_box(node))
        else {
            return Ok(false);
        };
        let Some(area) = fragment
            .scroll_area
            .filter(|area| area.is_scroll_container())
        else {
            return Ok(false);
        };
        let Some(bounds) = self.node_bounds(node) else {
            return Ok(false);
        };
        let (local_x, local_y) = (x - bounds.x, y - bounds.y);
        let border = fragment.border;
        let in_padding_box = local_x >= border.left.to_f32()
            && local_y >= border.top.to_f32()
            && local_x < fragment.size.width.to_f32() - border.right.to_f32()
            && local_y < fragment.size.height.to_f32() - border.bottom.to_f32();
        let client = area.client_rect;
        let in_client = local_x >= client.x().to_f32()
            && local_y >= client.y().to_f32()
            && local_x < (client.x() + client.width()).to_f32()
            && local_y < (client.y() + client.height()).to_f32();
        Ok(in_padding_box && !in_client)
    }

    /// Keep the starting point where a removed subtree was, when it held it.
    pub(crate) fn note_focus_start_removal(
        &mut self,
        handles: &[NodeHandle],
        former_parent: NodeId,
        former_next: NodeId,
    ) {
        let removed = match self.focus_start {
            Some(FocusStart::Node(node)) => handles.contains(&node),
            Some(FocusStart::Position { parent, .. }) => handles.contains(&parent),
            None => false,
        };
        let handle_of = |engine: &Self, node: NodeId| {
            engine
                .node_slots
                .get(&node)
                .map(|index| engine.handle_for_slot(*index))
        };
        if removed {
            self.focus_start = handle_of(self, former_parent).map(|parent| FocusStart::Position {
                parent,
                next: self.next_handled_sibling(former_next),
            });
        } else if let Some(FocusStart::Position {
            next: Some(next), ..
        }) = self.focus_start
        {
            // The node the position was before left; it is now before that
            // node's next sibling.
            if handles.first() == Some(&next) {
                let moved = self.next_handled_sibling(former_next);
                if let Some(FocusStart::Position { next, .. }) = &mut self.focus_start {
                    *next = moved;
                }
            }
        }
    }

    fn next_handled_sibling(&self, mut node: NodeId) -> Option<NodeHandle> {
        while !node.is_none() {
            if let Some(index) = self.node_slots.get(&node) {
                return Some(self.handle_for_slot(*index));
            }
            node = self.document.node(node).next_sibling;
        }
        None
    }

    /// The index in tree order that navigation starts from when nothing
    /// is focused, and whether the start itself is excluded from the search:
    /// forward, the search begins after a node and at a position; backward,
    /// it ends before either.
    pub(crate) fn focus_start_anchor(
        &self,
        document_order: &[NodeHandle],
    ) -> Option<(usize, bool)> {
        match self.focus_start? {
            FocusStart::Node(node) => {
                let node_id = self.resolve(node).ok()?;
                if !self.node_is_connected(node_id) {
                    return None;
                }
                document_order
                    .iter()
                    .position(|candidate| *candidate == node)
                    .map(|index| (index, true))
            }
            FocusStart::Position { parent, next } => {
                if let Some(next) = next {
                    if let Some(index) = document_order.iter().position(|node| *node == next) {
                        return Some((index, false));
                    }
                }
                // At the end of `parent`'s children: just after its subtree.
                let start = document_order.iter().position(|node| *node == parent)?;
                let end = document_order[start + 1..]
                    .iter()
                    .position(|node| !self.is_inclusive_ancestor(parent, *node))
                    .map_or(document_order.len(), |offset| start + 1 + offset);
                Some((end, false))
            }
        }
    }

    fn is_inclusive_ancestor(&self, ancestor: NodeHandle, node: NodeHandle) -> bool {
        let mut current = Some(node);
        while let Some(handle) = current {
            if handle == ancestor {
                return true;
            }
            current = self.parent(handle).ok().flatten();
        }
        false
    }
}
