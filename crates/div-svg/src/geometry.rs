//! Skia paths for SVG geometry, and the queries Chromium answers with Skia:
//! length, point at length, bounding box, and fill and stroke containment.

use crate::arc::{arc_pieces, ArcPiece};
use crate::path_data::{PathData, Segment};
use crate::{Point, Rect};
use skia_safe::{
    path_utils, ContourMeasureIter, Paint, PaintCap, PaintJoin, PaintStyle, Path, PathBuilder,
    PathEffect, PathFillType,
};

/// `fill-rule` and `clip-rule`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// `stroke-linecap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// `stroke-linejoin`. `arcs` and `miter-clip` are SVG 2 values that
/// Chromium parses but draws as `miter`; the corpus checks this.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// The stroke properties that shape a stroke's outline.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    pub dash_array: Vec<f32>,
    pub dash_offset: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash_array: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

/// Builds the Skia path for parsed path data. Arcs are conics, as Skia's
/// SVG `arcTo` builds them (`crate::arc`): Chromium's arc bounding boxes in
/// the corpus are the exact ellipse extremes.
pub fn path_from_data(data: &PathData) -> Path {
    let mut builder = PathBuilder::new();
    let mut current = Point::default();
    let mut subpath_start = Point::default();
    for segment in &data.segments {
        match *segment {
            Segment::MoveTo(p) => {
                builder.move_to((p.x, p.y));
                subpath_start = p;
                current = p;
            }
            Segment::LineTo(p) => {
                builder.line_to((p.x, p.y));
                current = p;
            }
            Segment::QuadTo(c, p) => {
                builder.quad_to((c.x, c.y), (p.x, p.y));
                current = p;
            }
            Segment::CubicTo(c1, c2, p) => {
                builder.cubic_to((c1.x, c1.y), (c2.x, c2.y), (p.x, p.y));
                current = p;
            }
            Segment::ArcTo {
                radii,
                rotation,
                large,
                sweep,
                to,
            } => {
                for piece in arc_pieces(current, radii.x, radii.y, rotation, large, sweep, to) {
                    match piece {
                        ArcPiece::Line(p) => {
                            builder.line_to((p.x, p.y));
                        }
                        ArcPiece::Conic(c, p, w) => {
                            builder.conic_to((c.x, c.y), (p.x, p.y), w);
                        }
                    }
                }
                current = to;
            }
            Segment::Close => {
                builder.close();
                current = subpath_start;
            }
        }
    }
    builder.detach()
}

/// The total length of all contours (`getTotalLength`).
pub fn total_length(path: &Path) -> f32 {
    ContourMeasureIter::new(path, false, None)
        .map(|contour| contour.length())
        .sum()
}

/// The point at `distance` along the path (`getPointAtLength`), clamped to
/// the path's ends. A path whose contours all have zero length reports its
/// first point; `None` only for an empty path.
pub fn point_at_length(path: &Path, distance: f32) -> Option<Point> {
    let mut remaining = distance.max(0.0);
    let mut last = path.points().first().map(|p| Point::new(p.x, p.y));
    for contour in ContourMeasureIter::new(path, false, None) {
        let length = contour.length();
        if remaining <= length {
            return contour
                .pos_tan(remaining)
                .map(|(p, _)| Point::new(p.x, p.y));
        }
        remaining -= length;
        last = contour.pos_tan(length).map(|(p, _)| Point::new(p.x, p.y));
    }
    last
}

/// The fill bounding box (`getBBox`): the tight bounds of the geometry.
pub fn bounding_box(path: &Path) -> Rect {
    let r = path.compute_tight_bounds();
    Rect::new(r.left, r.top, r.width(), r.height())
}

/// `isPointInFill`.
pub fn contains_in_fill(path: &Path, point: Point, rule: FillRule) -> bool {
    let mut path = path.clone();
    path.set_fill_type(match rule {
        FillRule::NonZero => PathFillType::Winding,
        FillRule::EvenOdd => PathFillType::EvenOdd,
    });
    path.contains((point.x, point.y))
}

/// `isPointInStroke`: containment in the stroke's outline.
pub fn contains_in_stroke(path: &Path, point: Point, stroke: &StrokeStyle) -> bool {
    if stroke.width <= 0.0 {
        return false;
    }
    let outline = stroke_outline(path, stroke);
    outline.contains((point.x, point.y))
}

/// The filled outline of a stroke, as Skia's stroker draws it.
pub fn stroke_outline(path: &Path, stroke: &StrokeStyle) -> Path {
    let mut paint = Paint::default();
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(stroke.width);
    paint.set_stroke_cap(match stroke.cap {
        LineCap::Butt => PaintCap::Butt,
        LineCap::Round => PaintCap::Round,
        LineCap::Square => PaintCap::Square,
    });
    paint.set_stroke_join(match stroke.join {
        LineJoin::Miter => PaintJoin::Miter,
        LineJoin::Round => PaintJoin::Round,
        LineJoin::Bevel => PaintJoin::Bevel,
    });
    paint.set_stroke_miter(stroke.miter_limit);
    if let Some(intervals) = dash_intervals(&stroke.dash_array) {
        paint.set_path_effect(PathEffect::dash(&intervals, stroke.dash_offset));
    }
    let mut builder = PathBuilder::new();
    path_utils::fill_path_with_paint(path, &paint, &mut builder, None, None);
    builder.detach()
}

/// The dash intervals to draw, or `None` for a solid stroke: an empty
/// array, a negative entry or an all-zero array draws solid (SVG 2
/// §13.5.3). An odd-length array repeats to make it even.
pub fn dash_intervals(array: &[f32]) -> Option<Vec<f32>> {
    if array.is_empty() || array.iter().any(|v| *v < 0.0) || array.iter().all(|v| *v == 0.0) {
        return None;
    }
    let mut intervals = array.to_vec();
    if intervals.len() % 2 == 1 {
        intervals.extend_from_slice(array);
    }
    Some(intervals)
}
