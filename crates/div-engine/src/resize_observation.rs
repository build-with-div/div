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
        let mut border_box = stitched_size(&parts, mode);
        // A native document's root is both the viewport and the root element.
        // Its scrollbars are the viewport's, outside the element's box: the
        // browser's `<html>` fills the initial containing block inside them,
        // as Div lays out the root's content. The root's scroll area's client
        // rect is that area. Chromium 147's sizes for `<html>` are in
        // `tests/fidelity/fixtures/resize-observer-root-v1`.
        let native_root =
            node == self.document.root() && self.document.document_element().is_none();
        if native_root {
            if let Some(area) = first.scroll_area {
                border_box = PhysicalSize::new(
                    border_box.width.min_of(area.client_rect.width()),
                    border_box.height.min_of(area.client_rect.height()),
                );
            }
        }
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
        // The native root's scrollbars, the viewport's, are already outside
        // its clamped box: it has no gutters left to subtract.
        let gutters = if native_root {
            BoxStrut::zero()
        } else {
            first
                .element_scrollbars
                .map_or(BoxStrut::zero(), |bars| bars.insets)
        };
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
    use div_layout::ElementScrollbars;
    use div_style::{parse_literal, StyleProperty};
    use std::sync::Arc;

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
    fn the_native_root_leaves_the_viewport_scrollbars_out() {
        // The root is the viewport and the root element. Its scrollbars are
        // the viewport's, outside the element's box, where the root's content
        // is laid out: Chromium 147's `<html>` reports the same inline sizes
        // (`tests/fidelity/fixtures/resize-observer-root-v1`).
        let mut engine = engine(1.0);
        let root = engine.root();
        style(
            &mut engine,
            root,
            &[
                (StyleProperty::Border, "1px solid black"),
                (StyleProperty::Padding, "5px"),
            ],
        );
        let child = element(&mut engine, root, &[(StyleProperty::Height, "20px")]);
        let visible = engine.resize_observation(root).unwrap();
        assert_eq!(visible.border_box, size(320.0, 240.0));
        assert_eq!(visible.content_box, size(308.0, 228.0));

        style(&mut engine, root, &[(StyleProperty::Overflow, "scroll")]);
        let scrolling = engine.resize_observation(root).unwrap();
        let client_width = f64::from(engine.client_width(root).unwrap());
        let client_height = f64::from(engine.client_height(root).unwrap());
        assert!(client_width < 320.0, "the scrollbar takes inline space");
        assert_eq!(scrolling.border_box, size(client_width, client_height));
        assert_eq!(
            scrolling.content_box,
            size(client_width - 12.0, client_height - 12.0)
        );
        assert_eq!(
            scrolling.content_box.inline_size,
            f64::from(engine.client_width(child).unwrap()),
            "the content box is where the root's content is laid out"
        );
    }

    #[test]
    fn the_native_root_counts_its_scrollbar_gutters_once() {
        // The clamp already leaves the viewport's scrollbars out of the
        // root's box. Were the root fragment to carry element gutters too
        // (for example from `scrollbar-gutter` on the root), they must not be
        // subtracted a second time. Layout gives the native root none today,
        // so the test gives the retained root fragment a 15px gutter on the
        // right and bottom.
        let mut engine = engine(1.0);
        let root = engine.root();
        style(
            &mut engine,
            root,
            &[
                (StyleProperty::Border, "1px solid black"),
                (StyleProperty::Padding, "5px"),
                (StyleProperty::Overflow, "scroll"),
            ],
        );
        element(&mut engine, root, &[(StyleProperty::Height, "20px")]);
        let measured = engine.resize_observation(root).unwrap();
        let client_width = f64::from(engine.client_width(root).unwrap());
        let client_height = f64::from(engine.client_height(root).unwrap());
        assert!(client_width < 320.0, "the scrollbar takes inline space");

        let node = engine.resolve(root).unwrap();
        let fragment = Arc::make_mut(engine.latest_fragment.as_mut().unwrap());
        let gutter = LayoutUnit::from_f64(15.0);
        let mut given = 0;
        give_gutters(fragment, node, gutter, &mut given);
        assert!(given > 0, "the root has a retained box");
        let observation = engine.retained_resize_observation(node);
        assert_eq!(observation.border_box, size(client_width, client_height));
        assert_eq!(
            observation.content_box,
            size(client_width - 12.0, client_height - 12.0),
            "the scrollbars are subtracted once"
        );
        assert_eq!(observation.content_box, measured.content_box);
    }

    /// The browser-shaped twin of the native root's oracle cases: `<html>`
    /// styled as the oracle styles it, with a `<body>` and the child, at 1x.
    /// Overflow on `<html>` moves to the viewport, so its box sits inside the
    /// viewport's scrollbars on the ordinary path, and its `auto` height is
    /// its content's.
    #[test]
    fn a_browser_shaped_html_matches_chromium_at_1x() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fidelity/fixtures/resize-observer-root-v1/chromium-windows.json"
        ))
        .unwrap();
        let mut compared = 0;
        for case in oracle["cases"].as_array().unwrap() {
            if case["scale"].as_f64().unwrap() != 1.0 {
                continue;
            }
            let name = case["case"].as_str().unwrap();
            let mut engine = Engine::new_with_font_collection(
                ViewportMetrics::from_logical_size(200.0, 100.0, 1.0).unwrap(),
                FontCollection::deterministic_test(),
            )
            .unwrap();
            let root = engine.root();
            let html = engine.create_native_element(ElementTag::Html).unwrap();
            engine.append_child(root, html).unwrap();
            style(
                &mut engine,
                html,
                &[
                    (StyleProperty::Margin, "0px"),
                    (StyleProperty::Border, "1px solid black"),
                    (StyleProperty::Padding, "5px"),
                    (StyleProperty::Overflow, case["overflow"].as_str().unwrap()),
                ],
            );
            let body = engine.create_native_element(ElementTag::Body).unwrap();
            engine.append_child(html, body).unwrap();
            style(&mut engine, body, &[(StyleProperty::Margin, "0px")]);
            let child_height = format!("{}px", case["child_height"].as_f64().unwrap());
            element(
                &mut engine,
                body,
                &[
                    (StyleProperty::Width, "40px"),
                    (StyleProperty::Height, &child_height),
                ],
            );
            let observation = engine.resize_observation(html).unwrap();
            let chromium = |key: &str| {
                size(
                    case[key]["inline"].as_f64().unwrap(),
                    case[key]["block"].as_f64().unwrap(),
                )
            };
            assert_eq!(observation.border_box, chromium("border_box"), "{name}");
            assert_eq!(observation.content_box, chromium("content_box"), "{name}");
            assert_eq!(
                observation.device_pixel_content_box,
                chromium("device_pixel_content_box"),
                "{name}"
            );
            let rect: Vec<f32> = case["content_rect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_f64().unwrap() as f32)
                .collect();
            let actual = observation.content_rect;
            assert_eq!(
                vec![actual.x, actual.y, actual.width, actual.height],
                rect,
                "{name}"
            );
            compared += 1;
        }
        assert_eq!(compared, 3, "the oracle's three 1x cases");
    }

    fn give_gutters(fragment: &mut Fragment, node: NodeId, gutter: LayoutUnit, given: &mut usize) {
        if fragment.node_id == node && fragment.owns_element_box() {
            fragment.element_scrollbars = Some(ElementScrollbars {
                insets: BoxStrut {
                    top: LayoutUnit::zero(),
                    right: gutter,
                    bottom: gutter,
                    left: LayoutUnit::zero(),
                },
                horizontal: true,
                vertical: true,
                thickness: gutter,
            });
            *given += 1;
        }
        for child in &mut fragment.children {
            give_gutters(child, node, gutter, given);
        }
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
