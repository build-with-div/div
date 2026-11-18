//! Keyboard scrolling, as Chromium 147.0.7727.50's
//! `ScrollManager::LogicalScroll` (`third_party/blink/renderer/core/input/
//! scroll_manager.cc`) performs the default action of an arrow, page, Home,
//! End or space key.
//!
//! - The chain starts at the focused element, else the node the latest press
//!   hit (`MouseEventManager::MousePressNode`), else the document. A start
//!   node without a layout box starts at the document.
//! - Each scroller in the chain, from the start outwards, converts the
//!   direction to a physical one in its own writing mode and scrolls if its
//!   overflow lets the user scroll on that axis. The first that moves ends
//!   the scroll.
//! - The step is `ScrollableArea::ScrollStep`: a line is
//!   `kPixelsPerLineStep` (40) pixels, a page is `ScrollableArea::PageStep`
//!   (`cc::ScrollUtils::CalculatePageStep` of the pixel-snapped visible
//!   scroll snapport), and a document is the whole scroll range.
//!
//! Chromium animates the scroll; Div applies it at once. Neither
//! `overscroll-behavior` nor scroll snapping applies yet.

use crate::scroll_button::{
    page_step, physical_direction, scroll_snapport, visible_snapport_length,
};
use crate::{Engine, EngineError, NodeHandle, ScrollButtonDirection};

/// How far a keyboard scroll moves, as Chromium's `ui::ScrollGranularity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollGranularity {
    /// 40 CSS pixels: an arrow key.
    Line,
    /// 87.5% of the scroller's visible snapport, at least one pixel: Page Up,
    /// Page Down and the space bar.
    Page,
    /// The whole scroll range: Home and End.
    Document,
}

impl Engine {
    /// The node the latest pointer press hit. It survives the release and a
    /// cancelled `mousedown`; when it is removed, its former parent takes
    /// its place.
    pub fn mouse_press_node(&self) -> Option<NodeHandle> {
        self.mouse_press_node
    }

    /// Scroll by one `granularity` step in `direction` as a key's default
    /// action, from the focused element or the pressed node outwards.
    /// Returns whether a scroller moved.
    pub fn scroll_logical(
        &mut self,
        direction: ScrollButtonDirection,
        granularity: ScrollGranularity,
    ) -> Result<bool, EngineError> {
        self.update()?;
        let root = self.root();
        let start = self
            .focused
            .or(self.mouse_press_node)
            .filter(|handle| {
                self.resolve(*handle).is_ok_and(|node| {
                    self.node_is_connected(node)
                        && self
                            .latest_fragment
                            .as_ref()
                            .is_some_and(|fragment| fragment.contains_element_box(node))
                })
            })
            .unwrap_or(root);
        let start_node = self.resolve(start)?;
        let mut current = Some(start);
        while let Some(handle) = current {
            let owner = self.resolve(handle)?;
            let in_scroll_box = handle == root
                || self.latest_fragment.as_ref().is_some_and(|fragment| {
                    fragment.scroll_box_contains_element(owner, start_node)
                });
            if in_scroll_box && self.scroll_one_logical(handle, direction, granularity)? {
                return Ok(true);
            }
            current = self.parent(handle)?;
        }
        Ok(false)
    }

    fn scroll_one_logical(
        &mut self,
        handle: NodeHandle,
        direction: ScrollButtonDirection,
        granularity: ScrollGranularity,
    ) -> Result<bool, EngineError> {
        let node = self.resolve(handle)?;
        let Some(area) = self.layout_scroll_area(node) else {
            return Ok(false);
        };
        if !area.is_scroll_container() {
            return Ok(false);
        }
        let style = self.retained_scroll_style(node);
        let direction = physical_direction(
            direction,
            style.direction.writing_direction(style.writing_mode),
        );
        let axis = direction.axis();
        let user_scrollable = if axis == 0 {
            area.overflow_x.is_scrollable()
        } else {
            area.overflow_y.is_scrollable()
        };
        if !user_scrollable {
            return Ok(false);
        }
        let before = self.scroll_offset(handle)?;
        let offset = [before.0, before.1];
        let step = match granularity {
            ScrollGranularity::Line => 40.0,
            ScrollGranularity::Page => {
                let port = scroll_snapport(style, area);
                f64::from(page_step(visible_snapport_length(
                    area,
                    port,
                    offset[axis] as f32,
                    axis,
                )))
            }
            ScrollGranularity::Document => {
                let limits = area.limits();
                let (minimum, maximum) = if axis == 0 { limits.0 } else { limits.1 };
                maximum - minimum
            }
        };
        let mut target = offset;
        target[axis] += f64::from(direction.sign()) * step;
        self.scroll_to(handle, target[0], target[1])?;
        Ok(self.scroll_offset(handle)? != before)
    }
}
