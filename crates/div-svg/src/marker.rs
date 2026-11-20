//! Markers (SVG 2 §11.6): where each marker of a shape goes, which way it
//! points, and how its content maps into the shape's user space.
//!
//! This is a clean-room implementation from the SVG 2 specification
//! (§11.6 and the path directionality notes of §9.5) and from black-box
//! measurement of Chromium 147 (`tools/svg/paint_corpus.py`, set v7).
//! Chromium's painting of a marker (`SVGShapePainter::PaintMarker`, BSD)
//! saves the canvas, concatenates the marker transform, clips to the
//! marker viewport unless `overflow` is visible, and paints the marker's
//! content; the transform and the positions are this module's.
//!
//! Markers apply to `path`, `line`, `polyline` and `polygon`; Chromium
//! draws none on the other basic shapes. There is a vertex at every move
//! and at the end of every segment, a closing segment included. The first
//! vertex of the path takes `marker-start`, the last `marker-end` (a path
//! of one vertex has only an end), and every other one `marker-mid`.
//!
//! Directions follow SVG 2 where Chromium does, with these differences,
//! each measured in set v7:
//!
//! - A segment of zero length points along the positive x axis, where
//!   SVG 2 borrows the direction of a neighbouring segment
//!   (`dir/zero-diagonal`, `dir/zero-close`).
//! - A move has a direction too, from the current point (the origin, for
//!   the first) to the move's point. A vertex at a move points along the
//!   segment leaving it, or, when none does, along the move
//!   (`dir/moves`, `dir/move-pair`, `dir/lone-move`). The vertex before a
//!   move points along the segment entering it.
//! - Halfway between an incoming and an outgoing direction is the incoming
//!   one plus half the turn, the turn taken in [-180°, 180°): a full
//!   reversal turns by -90° (`dir/reversal`).
//!
//! `refX` and `refY` are lengths in the `viewBox`'s units: the SVG 2
//! keywords are not supported (`ref/keywords`), and a percentage resolves
//! against the shape's viewport (`ref/percent`), which is the
//! `LengthContext` the caller passes.

use crate::length::{Axis, Length, LengthContext};
use crate::path_data::Segment;
use crate::transform::Matrix;
use crate::viewport::{view_box_transform, PreserveAspectRatio};
use crate::{Point, Rect};

/// Which marker property a vertex takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Start,
    Mid,
    End,
}

/// A vertex that gets a marker, with the direction `orient="auto"` gives
/// it, in degrees clockwise from the positive x axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerPosition {
    pub kind: MarkerKind,
    pub origin: Point,
    pub angle: f64,
}

/// A vertex with the directions of the path into and out of it.
#[derive(Clone, Copy, Debug)]
struct Vertex {
    point: Point,
    incoming: Option<f64>,
    outgoing: Option<f64>,
    /// A move's vertex, other than the start of a closed subpath.
    at_move: bool,
}

/// The direction from `from` to `to` in degrees; 0 when they coincide.
fn direction(from: Point, to: Point) -> f64 {
    let (dx, dy) = (
        f64::from(to.x) - f64::from(from.x),
        f64::from(to.y) - f64::from(from.y),
    );
    dy.atan2(dx).to_degrees()
}

/// The direction from `from` to the first of `points` that differs from
/// it; 0 when none does.
fn first_direction(from: Point, points: &[Point]) -> f64 {
    points
        .iter()
        .find(|&&p| p != from)
        .map_or(0.0, |&p| direction(from, p))
}

/// The direction into `to` from the last of `points` that differs from it;
/// 0 when none does.
fn last_direction(points: &[Point], to: Point) -> f64 {
    points
        .iter()
        .rev()
        .find(|&&p| p != to)
        .map_or(0.0, |&p| direction(p, to))
}

/// The tangent directions at the start and end of an elliptical arc
/// (SVG 2 §9.5.1, the centre parameterization of B.2.4), in double
/// precision. An arc with a zero radius is a line, and one whose ends
/// coincide has zero length.
fn arc_directions(
    from: Point,
    radii: Point,
    rotation: f32,
    large: bool,
    sweep: bool,
    to: Point,
) -> (f64, f64) {
    if from == to {
        return (0.0, 0.0);
    }
    let (mut rx, mut ry) = (f64::from(radii.x).abs(), f64::from(radii.y).abs());
    if rx == 0.0 || ry == 0.0 {
        let d = direction(from, to);
        return (d, d);
    }
    let phi = f64::from(rotation).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let (x1, y1) = (f64::from(from.x), f64::from(from.y));
    let (x2, y2) = (f64::from(to.x), f64::from(to.y));
    let (hx, hy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let x1p = cos_phi * hx + sin_phi * hy;
    let y1p = -sin_phi * hx + cos_phi * hy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut k = (num / den).sqrt();
    if large == sweep {
        k = -k;
    }
    let cxp = k * rx * y1p / ry;
    let cyp = -k * ry * x1p / rx;
    let theta1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
    let mut delta = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx) - theta1;
    let tau = std::f64::consts::TAU;
    if sweep && delta < 0.0 {
        delta += tau;
    } else if !sweep && delta > 0.0 {
        delta -= tau;
    }
    let sign = if delta >= 0.0 { 1.0 } else { -1.0 };
    // The derivative of the ellipse point at `t`, rotated by `phi`.
    let tangent = |t: f64| {
        let (dx, dy) = (-rx * t.sin() * sign, ry * t.cos() * sign);
        let (x, y) = (cos_phi * dx - sin_phi * dy, sin_phi * dx + cos_phi * dy);
        y.atan2(x).to_degrees()
    };
    (tangent(theta1), tangent(theta1 + delta))
}

/// The angle halfway between an incoming and an outgoing direction: the
/// incoming one plus half the turn, the turn taken in [-180°, 180°).
fn bisect(incoming: f64, outgoing: f64) -> f64 {
    let mut turn = outgoing - incoming;
    if turn >= 180.0 {
        turn -= 360.0;
    } else if turn < -180.0 {
        turn += 360.0;
    }
    incoming + turn / 2.0
}

/// The marker positions of a path's segments.
pub fn marker_positions(segments: &[Segment]) -> Vec<MarkerPosition> {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut subpath_first = 0;
    let mut current = Point::default();
    let mut subpath_start = Point::default();
    for segment in segments {
        let (to, (start, end)) = match *segment {
            Segment::MoveTo(p) => {
                subpath_first = vertices.len();
                vertices.push(Vertex {
                    point: p,
                    incoming: Some(direction(current, p)),
                    outgoing: None,
                    at_move: true,
                });
                current = p;
                subpath_start = p;
                continue;
            }
            Segment::LineTo(p) => (p, {
                let d = direction(current, p);
                (d, d)
            }),
            Segment::QuadTo(c, p) => (
                p,
                (
                    first_direction(current, &[c, p]),
                    last_direction(&[current, c], p),
                ),
            ),
            Segment::CubicTo(c1, c2, p) => (
                p,
                (
                    first_direction(current, &[c1, c2, p]),
                    last_direction(&[current, c1, c2], p),
                ),
            ),
            Segment::ArcTo {
                radii,
                rotation,
                large,
                sweep,
                to,
            } => (
                to,
                arc_directions(current, radii, rotation, large, sweep, to),
            ),
            Segment::Close => (subpath_start, {
                let d = direction(current, subpath_start);
                (d, d)
            }),
        };
        if let Some(previous) = vertices.last_mut() {
            previous.outgoing = Some(start);
        }
        vertices.push(Vertex {
            point: to,
            incoming: Some(end),
            outgoing: None,
            at_move: false,
        });
        current = to;
        if matches!(segment, Segment::Close) {
            // The start of a closed subpath is entered by the closing
            // segment, and the closing vertex leaves along the first.
            let leaving = vertices[subpath_first].outgoing;
            let first = &mut vertices[subpath_first];
            first.incoming = Some(end);
            first.at_move = false;
            vertices.last_mut().unwrap().outgoing = leaving;
            // A segment after the close starts from the closing vertex.
            subpath_first = vertices.len() - 1;
        }
    }

    let last = vertices.len().saturating_sub(1);
    vertices
        .iter()
        .enumerate()
        .map(|(i, v)| MarkerPosition {
            kind: if i == last {
                MarkerKind::End
            } else if i == 0 {
                MarkerKind::Start
            } else {
                MarkerKind::Mid
            },
            origin: v.point,
            angle: match (v.incoming, v.outgoing) {
                (_, Some(out)) if v.at_move => out,
                (Some(a), Some(b)) => bisect(a, b),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => 0.0,
            },
        })
        .collect()
}

/// `markerUnits`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkerUnits {
    #[default]
    StrokeWidth,
    UserSpaceOnUse,
}

/// `orient`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orient {
    Auto,
    AutoStartReverse,
    /// An angle in degrees.
    Angle(f64),
}

impl Default for Orient {
    fn default() -> Self {
        Self::Angle(0.0)
    }
}

impl Orient {
    /// `auto | auto-start-reverse | <angle> | <number>`; `None` for anything
    /// else, which leaves the initial value, 0.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
        match text {
            "auto" => return Some(Self::Auto),
            "auto-start-reverse" => return Some(Self::AutoStartReverse),
            _ => {}
        }
        let mut scanner = crate::number::Scanner::new(text);
        let value = f64::from(scanner.number()?);
        let degrees = match &text[scanner.pos..] {
            "" | "deg" => value,
            "rad" => value.to_degrees(),
            "grad" => value * 0.9,
            "turn" => value * 360.0,
            _ => return None,
        };
        Some(Self::Angle(degrees))
    }

    /// The angle a marker at `position` is rotated by.
    pub fn angle(self, position: &MarkerPosition) -> f64 {
        match self {
            Self::Auto => position.angle,
            Self::AutoStartReverse if position.kind == MarkerKind::Start => position.angle + 180.0,
            Self::AutoStartReverse => position.angle,
            Self::Angle(degrees) => degrees,
        }
    }
}

/// What a `marker` element states.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarkerAttributes {
    pub marker_width: Option<Length>,
    pub marker_height: Option<Length>,
    pub ref_x: Option<Length>,
    pub ref_y: Option<Length>,
    pub units: MarkerUnits,
    pub orient: Orient,
    /// A valid `viewBox` of positive size.
    pub view_box: Option<Rect>,
    /// A valid `viewBox` of zero width or height, which disables the marker.
    pub empty_view_box: bool,
    pub preserve_aspect_ratio: PreserveAspectRatio,
}

/// A marker resolved for one vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerPlacement {
    /// From the marker viewport's space to the shape's user space.
    pub transform: Matrix,
    /// The marker viewport, which clips the content unless `overflow` is
    /// visible, in the space of `transform`.
    pub viewport: Rect,
    /// From the content's user space (after the `viewBox`) to the
    /// viewport's.
    pub content_transform: Matrix,
}

impl MarkerAttributes {
    /// The marker at `position` on a shape whose stroke width is
    /// `stroke_width`; `None` when the marker renders nothing (a zero or
    /// negative size, or an empty `viewBox`).
    pub fn place(
        &self,
        position: &MarkerPosition,
        stroke_width: f32,
        context: &LengthContext,
    ) -> Option<MarkerPlacement> {
        let three = Length::parse("3").unwrap();
        let width = self
            .marker_width
            .unwrap_or(three)
            .resolve(Axis::Horizontal, context);
        let height = self
            .marker_height
            .unwrap_or(three)
            .resolve(Axis::Vertical, context);
        if !(width > 0.0 && height > 0.0) || self.empty_view_box {
            return None;
        }
        let content_transform = match self.view_box {
            Some(vb) => view_box_transform(vb, self.preserve_aspect_ratio, width, height),
            None => Matrix::IDENTITY,
        };
        let zero = Length::parse("0").unwrap();
        let reference = content_transform.map(Point::new(
            self.ref_x
                .unwrap_or(zero)
                .resolve(Axis::Horizontal, context),
            self.ref_y.unwrap_or(zero).resolve(Axis::Vertical, context),
        ));
        let scale = match self.units {
            MarkerUnits::StrokeWidth => f64::from(stroke_width),
            MarkerUnits::UserSpaceOnUse => 1.0,
        };
        let transform =
            Matrix::translate(f64::from(position.origin.x), f64::from(position.origin.y))
                .then_apply(&Matrix::rotate(self.orient.angle(position)))
                .then_apply(&Matrix::scale(scale, scale))
                .then_apply(&Matrix::translate(
                    -f64::from(reference.x),
                    -f64::from(reference.y),
                ));
        Some(MarkerPlacement {
            transform,
            viewport: Rect::new(0.0, 0.0, width, height),
            content_transform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_data::PathData;

    fn positions(d: &str) -> Vec<(MarkerKind, (f32, f32), f64)> {
        marker_positions(&PathData::parse(d).segments)
            .into_iter()
            .map(|p| (p.kind, (p.origin.x, p.origin.y), p.angle))
            .collect()
    }

    #[test]
    fn vertices_take_start_mid_and_end() {
        use MarkerKind::*;
        assert_eq!(
            positions("M0,0L10,0L10,10"),
            vec![
                (Start, (0.0, 0.0), 0.0),
                (Mid, (10.0, 0.0), 45.0),
                (End, (10.0, 10.0), 90.0)
            ]
        );
    }

    #[test]
    fn a_closed_subpath_joins_its_ends() {
        let p = positions("M0,0L10,0L10,10Z");
        assert_eq!(p.len(), 4);
        // Into the start along the closing segment (-135°), out along the
        // first (0°): halfway is -67.5°.
        assert_eq!(p[0].2, -67.5);
        assert_eq!(p[3].1, (0.0, 0.0));
        assert_eq!(p[3].2, -67.5);
    }

    #[test]
    fn moves_and_reversals() {
        use MarkerKind::*;
        // A lone move points from the origin and takes only an end marker.
        assert_eq!(positions("M10,10"), vec![(End, (10.0, 10.0), 45.0)]);
        // A reversal turns by -90°.
        assert_eq!(positions("M0,0L10,0L0,0")[1].2, -90.0);
    }

    #[test]
    fn orient_parses_angles_and_keywords() {
        assert_eq!(Orient::parse("auto"), Some(Orient::Auto));
        assert_eq!(Orient::parse("30"), Some(Orient::Angle(30.0)));
        assert_eq!(Orient::parse("0.5turn"), Some(Orient::Angle(180.0)));
        assert_eq!(Orient::parse("100grad"), Some(Orient::Angle(90.0)));
        assert_eq!(Orient::parse("sideways"), None);
    }
}
