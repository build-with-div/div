//! The basic shapes as paths (SVG 2 chapter 10), with values already
//! resolved to user units.
//!
//! A shape whose size is zero still has a path, so the geometry queries
//! (`getTotalLength`, `getBBox`) answer for it as Chromium does; SVG 2 §10
//! says such shapes do not render, and painting checks that separately
//! with [`renders`].

use crate::geometry::{bounding_box, path_from_data};
use crate::number::Scanner;
use crate::path_data::{PathData, Segment};
use crate::{Point, Rect};
use skia_safe::{Path, PathBuilder, PathDirection, RRect, Rect as SkRect};

/// How a shape is drawn. Chromium draws a square-cornered `rect` as a
/// rectangle and a `circle` or `ellipse` as an oval (`drawRect` and
/// `drawOval`, on the object bounding box), and everything else as its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    Rectangle,
    Ellipse,
    Path,
}

/// A shape's path and its object bounding box (`getBBox`).
#[derive(Clone, Debug)]
pub struct Shape {
    pub path: Path,
    pub bbox: Rect,
    pub geometry: Geometry,
    /// SVG 2 §10: a `rect` or `ellipse` whose used size is zero, a `circle`
    /// with no radius, or an empty path draws nothing.
    pub empty: bool,
    /// The `pathLength` attribute, which scales distances along the shape
    /// (dashes) by the computed length over it.
    pub path_length: Option<f32>,
}

impl Shape {
    /// A shape whose box is its path's tight bounds.
    pub fn from_path(path: Path) -> Self {
        let bbox = bounding_box(&path);
        let empty = path.is_empty();
        Self {
            path,
            bbox,
            geometry: Geometry::Path,
            empty,
            path_length: None,
        }
    }
}

/// Whether a `rect` or `ellipse` of this used size draws anything.
pub fn renders(width: f32, height: f32) -> bool {
    width > 0.0 && height > 0.0
}

/// `rect`. `rx` and `ry` are the used values: `None` is `auto`, which takes
/// the other radius. Each is clamped to half the side (SVG 2 §10.2); if
/// either is then zero the corners are square. A negative width or height
/// is used as zero.
pub fn rect(x: f32, y: f32, width: f32, height: f32, rx: Option<f32>, ry: Option<f32>) -> Shape {
    let width = width.max(0.0);
    let height = height.max(0.0);
    let (rx, ry) = match (rx, ry) {
        (None, None) => (0.0, 0.0),
        (Some(rx), None) => (rx, rx),
        (None, Some(ry)) => (ry, ry),
        (Some(rx), Some(ry)) => (rx, ry),
    };
    let rx = rx.max(0.0).min(width / 2.0);
    let ry = ry.max(0.0).min(height / 2.0);
    let bounds = SkRect::from_xywh(x, y, width, height);
    let mut builder = PathBuilder::new();
    let rounded = rx > 0.0 && ry > 0.0;
    if rounded {
        builder.add_rrect(RRect::new_rect_xy(bounds, rx, ry), PathDirection::CW, 0);
    } else {
        builder.add_rect(bounds, PathDirection::CW, 0);
    }
    Shape {
        path: builder.detach(),
        bbox: Rect::new(x, y, width, height),
        geometry: if rounded {
            Geometry::Path
        } else {
            Geometry::Rectangle
        },
        empty: !renders(width, height),
        path_length: None,
    }
}

/// `circle`.
pub fn circle(cx: f32, cy: f32, r: f32) -> Shape {
    ellipse(cx, cy, r, r)
}

/// `ellipse`, with used radii. With both radii zero the path is empty; with
/// one zero it is the degenerate oval.
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Shape {
    let rx = rx.max(0.0);
    let ry = ry.max(0.0);
    let bounds = SkRect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry);
    let mut builder = PathBuilder::new();
    if rx > 0.0 || ry > 0.0 {
        builder.add_oval(bounds, PathDirection::CW, 1);
    }
    Shape {
        path: builder.detach(),
        bbox: Rect::new(bounds.left, bounds.top, bounds.width(), bounds.height()),
        geometry: Geometry::Ellipse,
        empty: !renders(rx, ry),
        path_length: None,
    }
}

/// `line`.
pub fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Shape {
    let mut builder = PathBuilder::new();
    builder.move_to((x1, y1));
    builder.line_to((x2, y2));
    Shape::from_path(builder.detach())
}

/// `polyline` and `polygon` from a `points` attribute. Parsing stops at
/// the first error; an odd trailing coordinate is dropped.
pub fn poly(points: &str, closed: bool) -> Shape {
    let list = parse_points(points);
    let mut data = PathData::default();
    for (index, p) in list.iter().enumerate() {
        data.segments.push(if index == 0 {
            Segment::MoveTo(*p)
        } else {
            Segment::LineTo(*p)
        });
    }
    if closed && !list.is_empty() {
        data.segments.push(Segment::Close);
    }
    Shape::from_path(path_from_data(&data))
}

/// The coordinate pairs of a `points` attribute.
pub fn parse_points(text: &str) -> Vec<Point> {
    let mut scanner = Scanner::new(text);
    scanner.skip_ws();
    let mut out = Vec::new();
    while let Some(x) = scanner.number_sep() {
        let Some(y) = scanner.number_sep() else { break };
        out.push(Point::new(x, y));
    }
    out
}
