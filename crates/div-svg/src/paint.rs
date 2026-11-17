//! Painting SVG shapes onto a Skia canvas, as Chromium 147 paints them.
//!
//! Ported from Blink's SVG painters at Chromium 147.0.7727.50, which are
//! BSD-licensed (Copyright The Chromium Authors; "Use of this source code
//! is governed by a BSD-style license that can be found in the LICENSE
//! file"):
//!
//! - `core/paint/svg_shape_painter.cc`: fill then stroke, rectangles and
//!   ellipses drawn as such, anti-aliasing;
//! - `core/paint/svg_object_painter.cc`: `HasFill`, `HasVisibleStroke` and
//!   the colour setup in `PreparePaint`;
//! - `platform/geometry/stroke_data.cc` (Copyright 2013 Google Inc., BSD):
//!   `StrokeData::SetupPaint` and `SetLineDash`;
//! - `core/paint/svg_root_painter.cc`: `TransformToPixelSnappedBorderBox`
//!   and the empty-viewport check;
//! - `core/paint/svg_container_painter.cc` and
//!   `core/paint/svg_model_object_painter.cc`: viewport containers clip
//!   when overflow is hidden, and an empty `viewBox` disables rendering.
//!
//! Blink's layout objects (`layout_svg_*`) are LGPL and are not used: the
//! painters take `div-svg`'s own shapes and already-resolved values.
//!
//! The canvas is whatever the caller records into, Div's display list or a
//! raster surface; nothing here flushes, reads back or assumes it is the
//! final surface. Transforms are composed by the caller in double
//! precision ([`Matrix`]) and set with [`to_sk_matrix`], as Chromium
//! composes its paint property transforms before handing Skia one matrix.

use crate::geometry::{dash_intervals, FillRule, LineCap, LineJoin, StrokeStyle};
use crate::shapes::{Geometry, Shape};
use crate::transform::Matrix;
use crate::Rect;
use skia_safe::{
    Canvas, ClipOp, Color4f, Paint, PaintCap, PaintJoin, PaintStyle, PathEffect, PathFillType,
    Rect as SkRect,
};

/// A resolved `fill` or `stroke`. Paint servers (`url(...)`) and context
/// paints come with later stages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgPaint {
    None,
    /// An sRGB colour, not premultiplied, with `currentColor` already
    /// resolved.
    Color(Color4f),
}

/// The computed paint properties of one shape.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgPaintStyle {
    pub fill: SvgPaint,
    pub fill_opacity: f32,
    pub fill_rule: FillRule,
    pub stroke: SvgPaint,
    pub stroke_opacity: f32,
    pub stroke_style: StrokeStyle,
}

impl Default for SvgPaintStyle {
    /// The initial values: a black fill, no stroke.
    fn default() -> Self {
        Self {
            fill: SvgPaint::Color(Color4f::new(0.0, 0.0, 0.0, 1.0)),
            fill_opacity: 1.0,
            fill_rule: FillRule::NonZero,
            stroke: SvgPaint::None,
            stroke_opacity: 1.0,
            stroke_style: StrokeStyle::default(),
        }
    }
}

impl SvgPaintStyle {
    /// `ComputedStyle::HasFill`.
    pub fn has_fill(&self) -> bool {
        self.fill != SvgPaint::None
    }

    /// `SVGObjectPainter::HasVisibleStroke`: a stroke paint and a width
    /// above zero.
    pub fn has_visible_stroke(&self) -> bool {
        self.stroke != SvgPaint::None && self.stroke_style.width > 0.0
    }
}

/// `SVGShapePainter::PaintShape` for the default `paint-order` (fill,
/// stroke, markers; markers come with a later stage) and anti-aliased
/// `shape-rendering`. The canvas carries the shape's transform.
pub fn paint_shape(canvas: &Canvas, shape: &Shape, style: &SvgPaintStyle) {
    // SVG 2 and 1.1: shapes with a zero width or radius, and empty paths,
    // do not render.
    if shape.empty {
        return;
    }
    if style.has_fill() {
        if let Some(mut flags) = prepare_paint(style.fill, style.fill_opacity) {
            flags.set_anti_alias(true);
            fill_shape(canvas, shape, &flags, style.fill_rule);
        }
    }
    if style.has_visible_stroke() {
        if let Some(mut flags) = prepare_paint(style.stroke, style.stroke_opacity) {
            flags.set_anti_alias(true);
            setup_stroke(&style.stroke_style, &mut flags);
            stroke_shape(canvas, shape, &flags);
        }
    }
}

/// `SVGObjectPainter::PreparePaint` for a colour: the colour's alpha is
/// multiplied by the opacity, in single precision.
fn prepare_paint(paint: SvgPaint, opacity: f32) -> Option<Paint> {
    let SvgPaint::Color(color) = paint else {
        return None;
    };
    let color = Color4f::new(color.r, color.g, color.b, color.a * opacity);
    Some(Paint::new(color, None))
}

/// `StrokeData::SetupPaint`, with `SetLineDash`'s intervals.
fn setup_stroke(stroke: &StrokeStyle, flags: &mut Paint) {
    flags.set_style(PaintStyle::Stroke);
    flags.set_stroke_width(stroke.width);
    flags.set_stroke_cap(match stroke.cap {
        LineCap::Butt => PaintCap::Butt,
        LineCap::Round => PaintCap::Round,
        LineCap::Square => PaintCap::Square,
    });
    flags.set_stroke_join(match stroke.join {
        LineJoin::Miter => PaintJoin::Miter,
        LineJoin::Round => PaintJoin::Round,
        LineJoin::Bevel => PaintJoin::Bevel,
    });
    flags.set_stroke_miter(stroke.miter_limit);
    if let Some(intervals) = dash_intervals(&stroke.dash_array) {
        flags.set_path_effect(PathEffect::dash(&intervals, stroke.dash_offset));
    }
}

fn sk_rect(rect: Rect) -> SkRect {
    SkRect::from_xywh(rect.x, rect.y, rect.width, rect.height)
}

/// `SVGShapePainter::FillShape`.
fn fill_shape(canvas: &Canvas, shape: &Shape, flags: &Paint, rule: FillRule) {
    match shape.geometry {
        Geometry::Rectangle => {
            canvas.draw_rect(sk_rect(shape.bbox), flags);
        }
        Geometry::Ellipse => {
            canvas.draw_oval(sk_rect(shape.bbox), flags);
        }
        Geometry::Path => {
            let mut path = shape.path.clone();
            path.set_fill_type(match rule {
                FillRule::NonZero => PathFillType::Winding,
                FillRule::EvenOdd => PathFillType::EvenOdd,
            });
            canvas.draw_path(&path, flags);
        }
    }
}

/// `SVGShapePainter::StrokeShape` (`vector-effect` comes with a later
/// stage).
fn stroke_shape(canvas: &Canvas, shape: &Shape, flags: &Paint) {
    match shape.geometry {
        Geometry::Rectangle => {
            canvas.draw_rect(sk_rect(shape.bbox), flags);
        }
        Geometry::Ellipse => {
            canvas.draw_oval(sk_rect(shape.bbox), flags);
        }
        Geometry::Path => {
            canvas.draw_path(&shape.path, flags);
        }
    }
}

/// An outer `svg`'s border box, laid out and pixel-snapped by the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootBox {
    /// `PixelSnappedSize(paint_offset)`: the snapped border box, in the
    /// paint offset's space.
    pub snapped: (i32, i32, i32, i32),
    /// The unsnapped border-box size (`StitchedSize`).
    pub size: (f32, f32),
    /// Whether the `svg` is the document element (an SVG document) rather
    /// than inline in HTML.
    pub is_document_element: bool,
}

impl RootBox {
    /// An empty snapped viewport disables rendering
    /// (`SVGRootPainter::PaintReplaced`).
    pub fn renders(&self) -> bool {
        self.snapped.2 > 0 && self.snapped.3 > 0
    }
}

/// `SVGRootPainter::TransformToPixelSnappedBorderBox`: from the paint
/// offset's space to the root's user space. `local_to_border_box` is the
/// root's own transform within its border box (content-box offset, zoom
/// and `viewBox`), which layout computes.
///
/// The scale that fits an inline root's content into a box that snapping
/// shrank (`SvgInlineRootPixelSnappingScaleAdjustment`) is a test-only
/// feature in Chromium 147, so only document roots are scaled.
pub fn root_transform(root: &RootBox, local_to_border_box: &Matrix) -> Matrix {
    let (x, y, width, height) = root.snapped;
    let mut transform = Matrix::translate(f64::from(x), f64::from(y));
    let (size_width, size_height) = root.size;
    if size_width > 0.0 && size_height > 0.0 && root.is_document_element {
        // AffineTransform::Scale takes doubles; the quotients are floats.
        let scale_x = width as f32 / size_width;
        let scale_y = height as f32 / size_height;
        transform = transform.then_apply(&Matrix::scale(f64::from(scale_x), f64::from(scale_y)));
    }
    transform.then_apply(local_to_border_box)
}

/// Clips to a viewport: the outer `svg`'s snapped content box, or a nested
/// `svg`'s viewport when its `overflow` is hidden (Chromium's overflow clip
/// paint property, which `SVGContainerPainter` applies). The canvas carries
/// the transform of the space the rectangle is in.
pub fn clip_viewport(canvas: &Canvas, viewport: Rect, anti_alias: bool) {
    canvas.clip_rect(sk_rect(viewport), ClipOp::Intersect, anti_alias);
}

/// A double-precision transform as the single-precision matrix Skia draws
/// with.
pub fn to_sk_matrix(matrix: &Matrix) -> skia_safe::Matrix {
    let [a, b, c, d, e, f] = matrix.to_f32();
    skia_safe::Matrix::new_all(a, c, e, b, d, f, 0.0, 0.0, 1.0)
}
