//! Box sizes for `ResizeObserver` (RFC 0007, section 3).
//!
//! The engine answers what Chromium 147's `ResizeObservation` and
//! `ResizeObserverEntry` read from a `LayoutBox`
//! (`third_party/blink/renderer/core/resize_observer/`
//! `resize_observer_utilities.cc` and `resize_observer_entry.cc`): the border
//! and content box sizes in the element's writing mode, the content box in
//! device pixels, the physical content rectangle and the flat-tree depth. It
//! stores no observers; the framework keeps them and compares sizes.

use crate::element_dimensions::{collect_boxes, source_parts, stitched_size};
use crate::{Engine, EngineError, NodeHandle};
use div_compositor::SceneRect;
use div_dom::NodeId;
use div_geometry::{BoxStrut, LayoutUnit, PhysicalOffset, PhysicalSize};
use div_layout::{Fragment, ScrollArea};
use div_style::WritingMode;

/// A size in an element's own writing mode, in CSS pixels at `LayoutUnit`
/// precision (or whole device pixels for the device-pixel content box).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogicalBoxSize {
    pub inline_size: f64,
    pub block_size: f64,
}

/// What a resize observation reads from an element's layout box.
///
/// An element without a layout box (`display: none` or `contents`, text, a
/// non-atomic inline, or a disconnected element) reports zero sizes and an
/// empty `content_rect`, as Chromium's `ComputeTargetSize` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeObservation {
    /// `LayoutBox::LogicalWidth` and `LogicalHeight`.
    pub border_box: LogicalBoxSize,
    /// `ContentLogicalWidth` and `ContentLogicalHeight`: the border box less
    /// used borders, padding and scrollbar gutters.
    pub content_box: LogicalBoxSize,
    /// `ComputeSnappedDevicePixelContentBox`: the content box scaled by the
    /// device scale factor and snapped with `SnapSizeToPixel` at the first
    /// fragment's paint offset.
    pub device_pixel_content_box: LogicalBoxSize,
    /// Physical: used padding-left and padding-top, and the content size.
    pub content_rect: SceneRect,
    /// `ResizeObservation::TargetDepth`: the number of elements from the
    /// target up to the root element, inclusive (the root element is 1).
    pub depth: u32,
}

impl Default for ResizeObservation {
    fn default() -> Self {
        Self {
            border_box: LogicalBoxSize::default(),
            content_box: LogicalBoxSize::default(),
            device_pixel_content_box: LogicalBoxSize::default(),
            content_rect: SceneRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            depth: 0,
        }
    }
}
/// Blink 147 `SnapSizeToPixel` (`layout_unit.h`).
fn snap_size_to_pixel(size: LayoutUnit, location: LayoutUnit) -> i32 {
    let fraction = location.fraction();
    let result = (fraction + size).round_i32() - fraction.round_i32();
    if result == 0 && size.raw().unsigned_abs() > 4 {
        if size.raw() > 0 {
            1
        } else {
            -1
        }
    } else {
        result
    }
}

/// The border-box origin of `node`'s first box, accumulated from the root
/// fragment in untransformed layout coordinates.
///
/// Chromium uses `FirstFragment().PaintOffset()`, which is relative to the
/// local transform space; a paint offset translation keeps only the integer
/// part, so the fraction that `SnapSizeToPixel` reads is the accumulated
/// layout fraction. Transforms that scale are not applied here.
fn first_box_origin(
    fragment: &Fragment,
    node: NodeId,
    origin: PhysicalOffset,
) -> Option<PhysicalOffset> {
    let here = PhysicalOffset::new(
        origin.left + fragment.offset.left,
        origin.top + fragment.offset.top,
    );
    if fragment.node_id == node && fragment.owns_element_box() && !fragment.is_inline_box_fragment {
        let principal = fragment.border_box_rect().offset;
        return Some(PhysicalOffset::new(
            here.left + principal.left,
            here.top + principal.top,
        ));
    }
    fragment
        .children
        .iter()
        .find_map(|child| first_box_origin(child, node, here))
}

fn logical(size: PhysicalSize, horizontal: bool) -> LogicalBoxSize {
    let (inline, block) = if horizontal {
        (size.width, size.height)
    } else {
        (size.height, size.width)
    };
    LogicalBoxSize {
        inline_size: inline.to_f64(),
        block_size: block.to_f64(),
    }
}

impl Engine {
    /// Resolve pending style and layout, then measure `handle`'s layout box
    /// for a resize observation. The engine keeps no observer state.
    pub fn resize_observation(
        &mut self,
        handle: NodeHandle,
    ) -> Result<ResizeObservation, EngineError> {
        let node = self.resolve(handle)?;
        let depth = self.resize_observation_depth(node);
        if !self.node_is_connected(node) {
            return Ok(ResizeObservation {
                depth,
                ..ResizeObservation::default()
            });
        }
        self.update()?;
        Ok(ResizeObservation {
            depth,
            ..self.retained_resize_observation(node)
        })
    }

    fn resize_observation_depth(&self, node: NodeId) -> u32 {
        // A native document's root is its root element. A browser-shaped
        // document's root is the document node above `<html>`.
        let root = self.document.root();
        let root_is_element = self.document.document_element().is_none();
        let mut depth = 0;
        let mut current = node;
        let mut remaining = self.node_slots.len() + 1;
        while !current.is_none() && remaining > 0 {
            remaining -= 1;
            if current != root || root_is_element {
                depth += 1;
            }
            current = self.document.node(current).parent;
        }
        depth
    }

    fn retained_resize_observation(&self, node: NodeId) -> ResizeObservation {
        let Some(root) = self.latest_fragment.as_ref() else {
            return ResizeObservation::default();
        };
        let mut boxes = Vec::new();
        collect_boxes(root, node, &mut boxes);
        let Some(&first) = boxes.first() else {
            return ResizeObservation::default();
        };
        let style = &self.document.node(node).style;
        let mode = style.writing_mode;
        let horizontal = mode == WritingMode::HorizontalTb;
        let parts = source_parts(&boxes, horizontal);
        let border_box = stitched_size(&parts, mode);
        // A stitched box keeps each physical used edge of its continuations.
        let mut border = BoxStrut::zero();
        let mut padding = BoxStrut::zero();
        for (fragment, _) in &parts {
            let next_border = ScrollArea::used_border(fragment);
            let next_padding = ScrollArea::used_padding(fragment);
            border.top = border.top.max_of(next_border.top);
            border.right = border.right.max_of(next_border.right);
            border.bottom = border.bottom.max_of(next_border.bottom);
            border.left = border.left.max_of(next_border.left);
            padding.top = padding.top.max_of(next_padding.top);
            padding.right = padding.right.max_of(next_padding.right);
            padding.bottom = padding.bottom.max_of(next_padding.bottom);
            padding.left = padding.left.max_of(next_padding.left);
        }
        let gutters = first
            .element_scrollbars
            .map_or(BoxStrut::zero(), |bars| bars.insets);
        let content = PhysicalSize::new(
            (border_box.width - border.inline_sum() - gutters.inline_sum() - padding.inline_sum())
                .clamp_negative_to_zero(),
            (border_box.height - border.block_sum() - gutters.block_sum() - padding.block_sum())
                .clamp_negative_to_zero(),
        );
        let scale = self.viewport.device_scale_factor();
        let device = |value: LayoutUnit| LayoutUnit::from_f64(value.to_f64() * scale);
        let origin = first_box_origin(root, node, PhysicalOffset::zero())
            .unwrap_or_else(PhysicalOffset::zero);
        let device_size = PhysicalSize::new(
            LayoutUnit::from_f64(f64::from(snap_size_to_pixel(
                device(content.width),
                device(origin.left),
            ))),
            LayoutUnit::from_f64(f64::from(snap_size_to_pixel(
                device(content.height),
                device(origin.top),
            ))),
        );
        ResizeObservation {
            border_box: logical(border_box, horizontal),
            content_box: logical(content, horizontal),
            device_pixel_content_box: logical(device_size, horizontal),
            content_rect: SceneRect {
                x: padding.left.to_f64() as f32,
                y: padding.top.to_f64() as f32,
                width: content.width.to_f64() as f32,
                height: content.height.to_f64() as f32,
            },
            depth: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FontCollection;
    use div_dom::ElementTag;
    use div_geometry::ViewportMetrics;
    use div_style::{parse_literal, StyleProperty};

    fn engine(scale: f64) -> Engine {
        Engine::new_with_font_collection(
            ViewportMetrics::from_logical_size(320.0, 240.0, scale).unwrap(),
            FontCollection::deterministic_test(),
        )
        .unwrap()
    }

    fn style(engine: &mut Engine, node: NodeHandle, styles: &[(StyleProperty, &str)]) {
        for &(property, input) in styles {
            engine
                .set_property(node, property, parse_literal(property, input).unwrap())
                .unwrap();
        }
    }

    fn element(
        engine: &mut Engine,
        parent: NodeHandle,
        styles: &[(StyleProperty, &str)],
    ) -> NodeHandle {
        let node = engine.create_native_element(ElementTag::Div).unwrap();
        engine.append_child(parent, node).unwrap();
        style(engine, node, styles);
        node
    }

    fn size(inline_size: f64, block_size: f64) -> LogicalBoxSize {
        LogicalBoxSize {
            inline_size,
            block_size,
        }
    }

    #[test]
    fn a_block_reports_its_border_and_content_boxes_and_content_rect() {
        let mut engine = engine(1.0);
        let root = engine.root();
        let node = element(
            &mut engine,
            root,
            &[
                (StyleProperty::BoxSizing, "content-box"),
                (StyleProperty::Width, "100px"),
                (StyleProperty::Height, "50px"),
                (StyleProperty::Padding, "10px 6px 4px 3px"),
                (StyleProperty::Border, "2px solid black"),
            ],
        );
        let observation = engine.resize_observation(node).unwrap();
        assert_eq!(observation.border_box, size(113.0, 68.0));
        assert_eq!(observation.content_box, size(100.0, 50.0));
        assert_eq!(observation.device_pixel_content_box, size(100.0, 50.0));
        assert_eq!(
            observation.content_rect,
            SceneRect {
                x: 3.0,
                y: 10.0,
                width: 100.0,
                height: 50.0,
            }
        );
        assert_eq!(observation.depth, 2);

        // `box-sizing: border-box` moves the authored size to the border box.
        style(
            &mut engine,
            node,
            &[(StyleProperty::BoxSizing, "border-box")],
        );
        let observation = engine.resize_observation(node).unwrap();
        assert_eq!(observation.border_box, size(100.0, 50.0));
        assert_eq!(observation.content_box, size(87.0, 32.0));
    }

    #[test]
    fn elements_without_a_layout_box_report_zero_sizes() {
        let mut engine = engine(1.0);
        let root = engine.root();
        let parent = element(&mut engine, root, &[(StyleProperty::Width, "40px")]);
        let hidden = element(
            &mut engine,
            parent,
            &[
                (StyleProperty::Display, "none"),
                (StyleProperty::Height, "10px"),
            ],
        );
        let contents = element(
            &mut engine,
            parent,
            &[
                (StyleProperty::Display, "contents"),
                (StyleProperty::Height, "10px"),
            ],
        );
        let inline = engine.create_native_element(ElementTag::Span).unwrap();
        engine.append_child(parent, inline).unwrap();
        let text = engine.create_text("inline text").unwrap();
        engine.append_child(inline, text).unwrap();
        let detached = engine.create_native_element(ElementTag::Div).unwrap();
        style(&mut engine, detached, &[(StyleProperty::Width, "10px")]);
        for (node, depth) in [(hidden, 3), (contents, 3), (inline, 3), (detached, 1)] {
            assert_eq!(
                engine.resize_observation(node).unwrap(),
                ResizeObservation {
                    depth,
                    ..ResizeObservation::default()
                }
            );
        }
        // An atomic inline has a box.
        engine.remove(text).unwrap();
        style(
            &mut engine,
            inline,
            &[
                (StyleProperty::Display, "inline-block"),
                (StyleProperty::Width, "12px"),
                (StyleProperty::Height, "7px"),
            ],
        );
        assert_eq!(
            engine.resize_observation(inline).unwrap().border_box,
            size(12.0, 7.0)
        );
    }

    #[test]
    fn vertical_writing_modes_swap_the_inline_and_block_sizes() {
        let mut engine = engine(1.0);
        let root = engine.root();
        let node = element(
            &mut engine,
            root,
            &[
                (StyleProperty::BoxSizing, "content-box"),
                (StyleProperty::WritingMode, "vertical-rl"),
                (StyleProperty::Width, "100px"),
                (StyleProperty::Height, "50px"),
                (StyleProperty::PaddingLeft, "5px"),
            ],
        );
        let observation = engine.resize_observation(node).unwrap();
        assert_eq!(observation.border_box, size(50.0, 105.0));
        assert_eq!(observation.content_box, size(50.0, 100.0));
        assert_eq!(observation.device_pixel_content_box, size(50.0, 100.0));
        // The content rectangle stays physical.
        assert_eq!(observation.content_rect.x, 5.0);
        assert_eq!(observation.content_rect.width, 100.0);
    }

    #[test]
    fn scrollbar_gutters_leave_the_content_box() {
        let mut engine = engine(1.0);
        let root = engine.root();
        let node = element(
            &mut engine,
            root,
            &[
                (StyleProperty::BoxSizing, "content-box"),
                (StyleProperty::Width, "100px"),
                (StyleProperty::Height, "50px"),
                (StyleProperty::Padding, "4px"),
                (StyleProperty::Overflow, "hidden"),
                (StyleProperty::ScrollbarGutter, "stable"),
            ],
        );
        let observation = engine.resize_observation(node).unwrap();
        let client_width = f64::from(engine.client_width(node).unwrap());
        assert!(client_width < 108.0, "the gutter takes inline space");
        assert_eq!(observation.border_box, size(108.0, 58.0));
        assert_eq!(observation.content_box, size(client_width - 8.0, 50.0));
    }

    #[test]
    fn the_device_pixel_content_box_snaps_at_the_paint_offset() {
        // `SnapSizeToPixel(size, location)` keeps the location's fraction:
        // 20.5px at x = 10.5 covers device pixels 11 to 31 (20), and at x = 0
        // covers 0 to 21 (21).
        for (scale, margin, expected) in [
            (1.0, 10.5, 20.0),
            (1.0, 0.0, 21.0),
            (1.5, 10.5, 31.0),
            (2.0, 10.5, 41.0),
        ] {
            let mut engine = engine(scale);
            let root = engine.root();
            let margin_literal = format!("{margin}px");
            let node = element(
                &mut engine,
                root,
                &[
                    (StyleProperty::BoxSizing, "content-box"),
                    (StyleProperty::MarginLeft, &margin_literal),
                    (StyleProperty::Width, "20.5px"),
                    (StyleProperty::Height, "10px"),
                ],
            );
            let x = engine.bounds(node).unwrap().unwrap().x;
            assert_eq!(f64::from(x), margin, "the box starts at its margin");
            let observation = engine.resize_observation(node).unwrap();
            assert_eq!(observation.content_box, size(20.5, 10.0));
            assert_eq!(
                observation.device_pixel_content_box,
                size(expected, 10.0 * scale),
                "scale {scale}, margin {margin}"
            );
        }
    }

    #[test]
    fn depth_counts_elements_up_to_the_root() {
        let mut engine = engine(1.0);
        let root = engine.root();
        let child = element(&mut engine, root, &[]);
        let grandchild = element(&mut engine, child, &[]);
        assert_eq!(engine.resize_observation(root).unwrap().depth, 1);
        assert_eq!(engine.resize_observation(child).unwrap().depth, 2);
        assert_eq!(engine.resize_observation(grandchild).unwrap().depth, 3);
    }
}
