//! Painting SVG shapes onto a Skia canvas, as Chromium 147 paints them.
//!
//! Ported from Blink's SVG painters at Chromium 147.0.7727.50, which are
//! BSD-licensed (Copyright The Chromium Authors; "Use of this source code
//! is governed by a BSD-style license that can be found in the LICENSE
//! file"):
//!
//! - `core/paint/svg_shape_painter.cc`: the `paint-order` loop, fill and
//!   stroke, rectangles and ellipses drawn as such, anti-aliasing from
//!   `shape-rendering`, and the non-scaling stroke's drawing;
//! - `core/paint/svg_object_painter.cc`: `HasFill`, `HasVisibleStroke` and
//!   `PreparePaint`, for a colour and for a paint server
//!   ([`crate::paint_server`]);
//! - `platform/geometry/stroke_data.cc` (Copyright 2013 Google Inc., BSD):
//!   `StrokeData::SetupPaint` and `SetLineDash`;
//! - `core/paint/svg_root_painter.cc`: `TransformToPixelSnappedBorderBox`
//!   and the empty-viewport check;
//! - `core/paint/svg_container_painter.cc` and
//!   `core/paint/svg_model_object_painter.cc`: viewport containers clip
//!   when overflow is hidden, and an empty `viewBox` disables rendering.
//!
//! Blink's layout objects (`layout_svg_*`) and `svg_layout_support` are
//! LGPL and are not used. What they compute is written here from SVG 2 and
//! settled against Chromium's pixels (`tests/paint_corpus.rs`): which dash
//! arrays draw, the `pathLength` scale ([`dash_scale_factor`]) and the
//! non-scaling stroke's transform, which the caller supplies.
//!
//! The canvas is whatever the caller records into, Div's display list or a
//! raster surface; nothing here flushes, reads back or assumes it is the
//! final surface. Transforms are composed by the caller in double
//! precision ([`Matrix`]) and set with [`to_sk_matrix`], as Chromium
//! composes its paint property transforms before handing Skia one matrix.

use crate::geometry::{dash_intervals, total_length, FillRule, LineCap, LineJoin, StrokeStyle};
use crate::paint_server::PaintServer;
use crate::shapes::{Geometry, Shape};
use crate::transform::Matrix;
use crate::Rect;
use skia_safe::{
    Canvas, ClipOp, Color4f, Paint, PaintCap, PaintJoin, PaintStyle, PathEffect, PathFillType,
    Rect as SkRect,
};
use std::sync::Arc;

/// A resolved `fill` or `stroke`. Context paints come with a later stage.
#[derive(Clone, Debug)]
pub enum SvgPaint {
    None,
    /// An sRGB colour, not premultiplied, with `currentColor` already
    /// resolved.
    Color(Color4f),
    /// `url(#id)` resolved to a paint server, with its fallback colour.
    /// A reference that resolves to no server is its fallback colour, or
    /// `None` without one; the caller resolves it so.
    Server {
        server: Arc<PaintServer>,
        fallback: Option<Color4f>,
    },
}

impl PartialEq for SvgPaint {
    /// Servers compare by identity.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Color(a), Self::Color(b)) => a == b,
            (
                Self::Server {
                    server: a,
                    fallback: fa,
                },
                Self::Server {
                    server: b,
                    fallback: fb,
                },
            ) => Arc::ptr_eq(a, b) && fa == fb,
            _ => false,
        }
    }
}

/// One of the layers `paint-order` orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintLayer {
    Fill,
    Stroke,
    Markers,
}

/// `paint-order`: the three layers, first painted first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintOrder(pub [PaintLayer; 3]);

impl Default for PaintOrder {
    /// `normal`: fill, stroke, markers.
    fn default() -> Self {
        Self([PaintLayer::Fill, PaintLayer::Stroke, PaintLayer::Markers])
    }
}

impl PaintOrder {
    /// `normal | [ fill || stroke || markers ]` (SVG 2 §13.10): the listed
    /// layers first, then the others in their normal order. `None` for an
    /// invalid value (a repeated or unknown keyword), which the cascade
    /// ignores.
    pub fn parse(text: &str) -> Option<Self> {
        let words: Vec<&str> = text.split_ascii_whitespace().collect();
        if words == ["normal"] {
            return Some(Self::default());
        }
        if words.is_empty() || words.len() > 3 {
            return None;
        }
        let mut listed = Vec::new();
        for word in words {
            let layer = match word {
                "fill" => PaintLayer::Fill,
                "stroke" => PaintLayer::Stroke,
                "markers" => PaintLayer::Markers,
                _ => return None,
            };
            if listed.contains(&layer) {
                return None;
            }
            listed.push(layer);
        }
        for layer in Self::default().0 {
            if !listed.contains(&layer) {
                listed.push(layer);
            }
        }
        Some(Self([listed[0], listed[1], listed[2]]))
    }
}

/// `shape-rendering`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapeRendering {
    #[default]
    Auto,
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

impl ShapeRendering {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "auto" => Self::Auto,
            "optimizeSpeed" => Self::OptimizeSpeed,
            "crispEdges" => Self::CrispEdges,
            "geometricPrecision" => Self::GeometricPrecision,
            _ => return None,
        })
    }

    /// `SVGShapePainter::PaintShape`: everything but `crispEdges` and
    /// `optimizeSpeed` is anti-aliased.
    pub fn anti_alias(self) -> bool {
        !matches!(self, Self::CrispEdges | Self::OptimizeSpeed)
    }
}

/// The computed paint properties of one shape.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgPaintStyle {
    pub fill: SvgPaint,
    pub fill_opacity: f32,
    pub fill_rule: FillRule,
    pub stroke: SvgPaint,
    pub stroke_opacity: f32,
    /// The stroke, with lengths in user units. Dashes are scaled by
    /// `pathLength` when painting.
    pub stroke_style: StrokeStyle,
    pub paint_order: PaintOrder,
    pub shape_rendering: ShapeRendering,
    /// `vector-effect: non-scaling-stroke`.
    pub non_scaling_stroke: bool,
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
            paint_order: PaintOrder::default(),
            shape_rendering: ShapeRendering::Auto,
            non_scaling_stroke: false,
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

/// How much `pathLength` scales distances along a shape: its computed
/// length over `pathLength` (SVG 2 §9.3). A negative `pathLength` is an
/// error and is ignored. Zero makes every distance infinite, so a dash
/// pattern's first dash covers the shape and it strokes solid; this
/// returns infinity for it, which [`paint_shape`] draws solid.
pub fn dash_scale_factor(shape: &Shape) -> f32 {
    match shape.path_length {
        Some(path_length) if path_length > 0.0 => total_length(&shape.path) / path_length,
        Some(path_length) if path_length == 0.0 => f32::INFINITY,
        _ => 1.0,
    }
}

/// `SVGShapePainter::PaintShape` for the layers in `paint-order` (markers
/// come with a later stage). The canvas carries the shape's transform.
///
/// `non_scaling_transform` is the transform from the shape's user space to
/// the host space of `vector-effect: non-scaling-stroke`, which layout
/// computes; it is used only when the style asks for a non-scaling stroke.
/// In Chromium 147 the host space is the outer `svg`'s own coordinates
/// before its paint offset and zoom: the corpus's non-scaling cases match
/// at scales 1 and 2 with that transform, and a stroke under real zoom
/// keeps its zoomed width.
/// The stroke is then drawn as the path mapped into that space, on a
/// canvas carrying the inverse. If the transform is not invertible, the
/// shape's remaining layers are not painted, as in Chromium.
pub fn paint_shape(
    canvas: &Canvas,
    shape: &Shape,
    style: &SvgPaintStyle,
    non_scaling_transform: Option<&Matrix>,
) {
    // SVG 2 and 1.1: shapes with a zero width or radius, and empty paths,
    // do not render.
    if shape.empty {
        return;
    }
    let anti_alias = style.shape_rendering.anti_alias();
    for layer in style.paint_order.0 {
        match layer {
            PaintLayer::Fill => {
                if !style.has_fill() {
                    continue;
                }
                if let Some(mut flags) =
                    prepare_paint(&style.fill, style.fill_opacity, Some(shape.bbox), None)
                {
                    flags.set_anti_alias(anti_alias);
                    fill_shape(canvas, shape, &flags, style.fill_rule);
                }
            }
            PaintLayer::Stroke => {
                if !style.has_visible_stroke() {
                    continue;
                }
                let non_scaling = if style.non_scaling_stroke {
                    let transform = non_scaling_transform.copied().unwrap_or(Matrix::IDENTITY);
                    let Some(inverse) = transform.inverse() else {
                        return;
                    };
                    Some((transform, inverse))
                } else {
                    None
                };
                let Some(mut flags) =
                    prepare_paint(&style.stroke, style.stroke_opacity, Some(shape.bbox), None)
                else {
                    continue;
                };
                flags.set_anti_alias(anti_alias);
                setup_stroke(&style.stroke_style, dash_scale_factor(shape), &mut flags);
                match non_scaling {
                    Some((transform, inverse)) => {
                        canvas.save();
                        canvas.concat(&to_sk_matrix(&inverse));
                        let path = shape.path.with_transform(&to_sk_matrix(&transform));
                        canvas.draw_path(&path, &flags);
                        canvas.restore();
                    }
                    None => stroke_shape(canvas, shape, &flags),
                }
            }
            PaintLayer::Markers => {}
        }
    }
}

/// `SVGObjectPainter::PreparePaint`. A paint server that applies to
/// `reference_box` (the shape's fill box, `ReferenceBoxForEffects`) paints
/// black with its shader, dithered, the opacity quantized to 8 bits as the
/// paint's alpha; `additional` is the text painter's shader transform.
/// Otherwise a colour, or a server's fallback colour, has its alpha
/// multiplied by the opacity in single precision.
pub(crate) fn prepare_paint(
    paint: &SvgPaint,
    opacity: f32,
    reference_box: Option<Rect>,
    additional: Option<&Matrix>,
) -> Option<Paint> {
    let color = match paint {
        SvgPaint::None => return None,
        SvgPaint::Color(color) => *color,
        SvgPaint::Server { server, fallback } => {
            if let Some(shader) = server.shader(reference_box, additional) {
                let mut flags = Paint::new(Color4f::new(0.0, 0.0, 0.0, 1.0), None);
                flags.set_alpha_f(
                    f32::from((opacity * 255.0).round().clamp(0.0, 255.0) as u8) / 255.0,
                );
                flags.set_shader(shader);
                // `Gradient::ApplyToFlags`: gradients are always dithered.
                if matches!(**server, PaintServer::Gradient(_)) {
                    flags.set_dither(true);
                }
                return Some(flags);
            }
            (*fallback)?
        }
    };
    let color = Color4f::new(color.r, color.g, color.b, color.a * opacity);
    Some(Paint::new(color, None))
}

/// `StrokeData::SetupPaint`, with `SetLineDash`'s intervals scaled by
/// `pathLength`'s factor.
pub(crate) fn setup_stroke(stroke: &StrokeStyle, dash_scale: f32, flags: &mut Paint) {
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
    if !dash_scale.is_finite() {
        return;
    }
    if let Some(mut intervals) = dash_intervals(&stroke.dash_array) {
        let mut offset = stroke.dash_offset;
        if dash_scale != 1.0 {
            for interval in &mut intervals {
                *interval *= dash_scale;
            }
            offset *= dash_scale;
        }
        flags.set_path_effect(PathEffect::dash(&intervals, offset));
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

/// `SVGShapePainter::StrokeShape` for a scaling stroke.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    #[test]
    fn paint_order_lists_the_named_layers_first() {
        use PaintLayer::{Fill, Markers, Stroke};
        let parse = |t| PaintOrder::parse(t).map(|o| o.0);
        assert_eq!(parse("normal"), Some([Fill, Stroke, Markers]));
        assert_eq!(parse("stroke"), Some([Stroke, Fill, Markers]));
        assert_eq!(parse("markers stroke"), Some([Markers, Stroke, Fill]));
        assert_eq!(parse("stroke markers fill"), Some([Stroke, Markers, Fill]));
        assert_eq!(parse("fill fill"), None);
        assert_eq!(parse("normal fill"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn crisp_edges_and_optimize_speed_turn_anti_aliasing_off() {
        let aa = |t| ShapeRendering::parse(t).unwrap().anti_alias();
        assert!(aa("auto") && aa("geometricPrecision"));
        assert!(!aa("crispEdges") && !aa("optimizeSpeed"));
        assert_eq!(ShapeRendering::parse("crispedges"), None);
    }

    #[test]
    fn path_length_scales_by_computed_length() {
        let mut line = shapes::line(0.0, 0.0, 40.0, 0.0);
        assert_eq!(dash_scale_factor(&line), 1.0);
        line.path_length = Some(10.0);
        assert_eq!(dash_scale_factor(&line), 4.0);
        line.path_length = Some(-5.0);
        assert_eq!(dash_scale_factor(&line), 1.0);
        line.path_length = Some(0.0);
        assert!(dash_scale_factor(&line).is_infinite());
    }
}
