//! `blink::CanvasPath` (`modules/canvas/canvas2d/canvas_path.{h,cc}`, Apple
//! and Google BSD licences) with `blink::PathBuilder`
//! (`platform/geometry/path_builder.cc`, Chromium BSD).
//!
//! A path keeps two fast forms besides the general `SkPathBuilder`: a single
//! line (drawn with `drawLine`) and a single arc on an empty path (drawn with
//! `drawArc`, since `CanvasUsesArcPaintOp` is `stable` in 147). The general
//! builder doubles as a cache of those forms, exactly as in Blink, because
//! the cache's presence changes later decisions (`IsEmpty`).

use skia_safe::{Path, PathBuilder as SkPathBuilder, PathDirection, RRect, Rect, Vector};

use crate::affine::AffineTransform;
use crate::math::{
    float_nearly_equal, fmodf, rad2deg_f32, saturated_f32, PI_OVER_TWO_FLOAT, TWO_PI_FLOAT,
};
use crate::CanvasError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Line {
    pub start: (f32, f32),
    pub end: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Arc {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub start_angle_radians: f32,
    pub sweep_angle_radians: f32,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineState {
    Empty,
    StartingPoint,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArcState {
    Empty,
    Arc,
    Closed,
}

/// One radius argument of `roundRect`: a number or a `DOMPointInit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoundRectRadius {
    Uniform(f64),
    Point { x: f64, y: f64 },
}

/// The current default path of a context.
#[derive(Clone)]
pub struct CanvasPath {
    line_state: LineState,
    line: Line,
    arc_state: ArcState,
    arc: Arc,
    builder: SkPathBuilder,
    pub(crate) transform: AffineTransform,
    pub(crate) transform_invertible: bool,
}

impl Default for CanvasPath {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasPath {
    pub fn new() -> Self {
        Self {
            line_state: LineState::Empty,
            line: Line {
                start: (0.0, 0.0),
                end: (0.0, 0.0),
            },
            arc_state: ArcState::Empty,
            arc: Arc {
                x: 0.0,
                y: 0.0,
                radius: 0.0,
                start_angle_radians: 0.0,
                sweep_angle_radians: 0.0,
                closed: false,
            },
            builder: SkPathBuilder::new(),
            transform: AffineTransform::IDENTITY,
            transform_invertible: true,
        }
    }

    // ---- Builder state -------------------------------------------------

    pub(crate) fn is_line(&self) -> bool {
        self.line_state == LineState::Line
    }

    pub(crate) fn is_arc(&self) -> bool {
        matches!(self.arc_state, ArcState::Arc | ArcState::Closed)
    }

    pub(crate) fn line_data(&self) -> Line {
        self.line
    }

    pub(crate) fn arc_data(&self) -> Arc {
        self.arc
    }

    pub fn is_empty(&self) -> bool {
        self.line_state == LineState::Empty
            && self.arc_state == ArcState::Empty
            && self.builder.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.line_state = LineState::Empty;
        self.arc_state = ArcState::Empty;
        self.builder.reset();
    }

    fn needs_update_from_line_or_arc(&self) -> bool {
        (self.line_state != LineState::Empty || self.arc_state != ArcState::Empty)
            && self.builder.is_empty()
    }

    fn update_from_line_or_arc_if_necessary(&mut self) {
        if !self.needs_update_from_line_or_arc() {
            return;
        }
        if self.line_state != LineState::Empty {
            self.builder.move_to(self.line.start);
            if self.is_line() {
                self.builder.line_to(self.line.end);
            }
        } else {
            let arc = self.arc;
            add_ellipse(
                &mut self.builder,
                (arc.x, arc.y),
                arc.radius,
                arc.radius,
                arc.start_angle_radians,
                arc.start_angle_radians + arc.sweep_angle_radians,
            );
            if self.arc_state == ArcState::Closed {
                self.builder.close();
            }
        }
    }

    fn update_for_mutation(&mut self) {
        self.update_from_line_or_arc_if_necessary();
        self.line_state = LineState::Empty;
        self.arc_state = ArcState::Empty;
    }

    /// `GetPath()`: the general form, materializing the fast forms into the
    /// builder (which then acts as their cache).
    pub(crate) fn sk_path(&mut self) -> Path {
        self.update_from_line_or_arc_if_necessary();
        self.builder.snapshot()
    }

    /// `GetModifiablePath().Transform(t)`.
    pub(crate) fn transform_path(&mut self, t: &AffineTransform) {
        self.update_for_mutation();
        self.builder.transform(&t.to_sk_matrix());
    }

    /// `BoundingRect()` as `(x, y, width, height)`.
    pub(crate) fn bounding_rect(&self) -> (f32, f32, f32, f32) {
        if self.line_state != LineState::Empty {
            let (s, e) = if self.line_state == LineState::StartingPoint {
                (self.line.start, self.line.start)
            } else {
                (self.line.start, self.line.end)
            };
            return bounding(s, e);
        }
        if self.is_arc() {
            let a = self.arc;
            return bounding(
                (a.x - a.radius, a.y - a.radius),
                (a.x + a.radius, a.y + a.radius),
            );
        }
        // Blink calls SkPathBuilder::computeBounds, deprecated in skia-safe.
        #[allow(deprecated)]
        let r = self.builder.compute_bounds();
        (r.left, r.top, r.width(), r.height())
    }

    fn current_point(&self) -> Option<(f32, f32)> {
        self.builder.get_last_pt().map(|p| (p.x, p.y))
    }

    fn map_if_not_invertible(&self, p: (f32, f32)) -> (f32, f32) {
        if self.transform_invertible {
            p
        } else {
            self.transform.map_point(p.0, p.1)
        }
    }

    // ---- CanvasPath API ------------------------------------------------

    pub fn close_path(&mut self) {
        if self.is_empty() {
            return;
        }
        #[allow(deprecated)]
        let path_bounds = self.builder.compute_bounds();
        if path_bounds.height() == 0.0
            && path_bounds.width() == 0.0
            && (self.is_line() && {
                let (_, _, w, h) = self.bounding_rect();
                h == 0.0 && w == 0.0
            })
        {
            let p = self.current_point();
            self.clear();
            if let Some((x, y)) = p {
                self.move_to(x as f64, y as f64);
            }
            return;
        }
        if self.is_arc() {
            if self.arc_state != ArcState::Closed {
                self.builder.reset();
                self.arc_state = ArcState::Closed;
                self.arc.closed = true;
            }
        } else {
            self.update_for_mutation();
            self.builder.close();
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        let x = saturated_f32(x);
        let y = saturated_f32(y);
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let point = self.map_if_not_invertible((x, y));
        if self.is_empty() {
            self.line_state = LineState::StartingPoint;
            self.line.start = point;
        } else {
            self.update_for_mutation();
            self.builder.move_to(point);
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        let x = saturated_f32(x);
        let y = saturated_f32(y);
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let p1 = self.map_if_not_invertible((x, y));
        if self.is_empty() {
            self.line_state = LineState::StartingPoint;
            self.line.start = p1;
        }
        if self.line_state == LineState::StartingPoint {
            self.builder.reset();
            self.line_state = LineState::Line;
            self.line.end = p1;
            return;
        }
        self.update_for_mutation();
        self.builder.line_to(p1);
    }

    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        let [cpx, cpy, x, y] = [cpx, cpy, x, y].map(saturated_f32);
        if !(cpx.is_finite() && cpy.is_finite() && x.is_finite() && y.is_finite()) {
            return;
        }
        self.update_for_mutation();
        let p1 = self.map_if_not_invertible((x, y));
        let cp = self.map_if_not_invertible((cpx, cpy));
        if self.current_point().is_none() {
            self.builder.move_to((cpx, cpy));
        }
        self.builder.quad_to(cp, p1);
    }

    pub fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        let [cp1x, cp1y, cp2x, cp2y, x, y] = [cp1x, cp1y, cp2x, cp2y, x, y].map(saturated_f32);
        if ![cp1x, cp1y, cp2x, cp2y, x, y].iter().all(|v| v.is_finite()) {
            return;
        }
        self.update_for_mutation();
        let p1 = self.map_if_not_invertible((x, y));
        let cp1 = self.map_if_not_invertible((cp1x, cp1y));
        let cp2 = self.map_if_not_invertible((cp2x, cp2y));
        if self.current_point().is_none() {
            self.builder.move_to((cp1x, cp1y));
        }
        self.builder.cubic_to(cp1, cp2, p1);
    }

    pub fn arc_to(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        r: f64,
    ) -> Result<(), CanvasError> {
        let [x1, y1, x2, y2, r] = [x1, y1, x2, y2, r].map(saturated_f32);
        if ![x1, y1, x2, y2, r].iter().all(|v| v.is_finite()) {
            return Ok(());
        }
        if r < 0.0 {
            return Err(CanvasError::IndexSize);
        }
        self.update_for_mutation();
        let p1 = self.map_if_not_invertible((x1, y1));
        let p2 = self.map_if_not_invertible((x2, y2));
        match self.current_point() {
            None => {
                self.builder.move_to(p1);
            }
            Some(current) if p1 == current || p1 == p2 || r == 0.0 => {
                self.line_to(x1 as f64, y1 as f64);
            }
            Some(_) => {
                self.builder.arc_to_tangent(p1, p2, r);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        anticlockwise: bool,
    ) -> Result<(), CanvasError> {
        let [x, y, radius, mut start_angle, mut end_angle] =
            [x, y, radius, start_angle, end_angle].map(saturated_f32);
        if ![x, y, radius, start_angle, end_angle]
            .iter()
            .all(|v| v.is_finite())
        {
            return Ok(());
        }
        if radius < 0.0 {
            return Err(CanvasError::IndexSize);
        }
        if !self.transform_invertible {
            return Ok(());
        }
        self.update_for_mutation();
        if radius == 0.0 || start_angle == end_angle {
            self.line_to(
                (x + radius * start_angle.cos()) as f64,
                (y + radius * start_angle.sin()) as f64,
            );
            return Ok(());
        }
        canonicalize_angle(&mut start_angle, &mut end_angle);
        let end_angle = adjust_end_angle(start_angle, end_angle, anticlockwise);
        if self.is_empty() && radius >= 1.0 {
            self.arc_state = ArcState::Arc;
            self.arc = Arc {
                x,
                y,
                radius,
                start_angle_radians: start_angle,
                sweep_angle_radians: end_angle - start_angle,
                closed: false,
            };
            return Ok(());
        }
        add_ellipse(
            &mut self.builder,
            (x, y),
            radius,
            radius,
            start_angle,
            end_angle,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        &mut self,
        x: f64,
        y: f64,
        radius_x: f64,
        radius_y: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
        anticlockwise: bool,
    ) -> Result<(), CanvasError> {
        let [x, y, radius_x, radius_y, rotation, mut start_angle, mut end_angle] =
            [x, y, radius_x, radius_y, rotation, start_angle, end_angle].map(saturated_f32);
        if ![x, y, radius_x, radius_y, rotation, start_angle, end_angle]
            .iter()
            .all(|v| v.is_finite())
        {
            return Ok(());
        }
        if radius_x < 0.0 || radius_y < 0.0 {
            return Err(CanvasError::IndexSize);
        }
        if !self.transform_invertible {
            return Ok(());
        }
        self.update_for_mutation();
        canonicalize_angle(&mut start_angle, &mut end_angle);
        let adjusted_end_angle = adjust_end_angle(start_angle, end_angle, anticlockwise);
        if radius_x == 0.0 || radius_y == 0.0 || start_angle == adjusted_end_angle {
            self.degenerate_ellipse(
                x,
                y,
                radius_x,
                radius_y,
                rotation,
                start_angle,
                adjusted_end_angle,
                anticlockwise,
            );
            return Ok(());
        }
        if rotation == 0.0 {
            add_ellipse(
                &mut self.builder,
                (x, y),
                radius_x,
                radius_y,
                start_angle,
                adjusted_end_angle,
            );
            return Ok(());
        }
        let mut ellipse_transform = AffineTransform::new(1.0, 0.0, 0.0, 1.0, x as f64, y as f64);
        ellipse_transform.rotate_radians(rotation as f64);
        let inverse = ellipse_transform.inverse();
        self.builder.transform(&inverse.to_sk_matrix());
        add_ellipse(
            &mut self.builder,
            (0.0, 0.0),
            radius_x,
            radius_y,
            start_angle,
            adjusted_end_angle,
        );
        self.builder.transform(&ellipse_transform.to_sk_matrix());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn degenerate_ellipse(
        &mut self,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        rotation: f32,
        start_angle: f32,
        end_angle: f32,
        anticlockwise: bool,
    ) {
        let mut rotation_matrix = AffineTransform::IDENTITY;
        rotation_matrix.rotate_radians(rotation as f64);
        let point_at = |angle: f32| {
            let (px, py) = (radius_x * angle.cos(), radius_y * angle.sin());
            let (mx, my) = rotation_matrix.map_point(px, py);
            (x + mx, y + my)
        };
        let (sx, sy) = point_at(start_angle);
        self.line_to(sx as f64, sy as f64);
        if (radius_x == 0.0 && radius_y == 0.0) || start_angle == end_angle {
            return;
        }
        if !anticlockwise {
            let mut angle = start_angle - fmodf(start_angle, PI_OVER_TWO_FLOAT) + PI_OVER_TWO_FLOAT;
            while angle < end_angle {
                let (px, py) = point_at(angle);
                self.line_to(px as f64, py as f64);
                angle += PI_OVER_TWO_FLOAT;
            }
        } else {
            let mut angle = start_angle - fmodf(start_angle, PI_OVER_TWO_FLOAT);
            while angle > end_angle {
                let (px, py) = point_at(angle);
                self.line_to(px as f64, py as f64);
                angle -= PI_OVER_TWO_FLOAT;
            }
        }
        let (ex, ey) = point_at(end_angle);
        self.line_to(ex as f64, ey as f64);
    }

    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let [x, y, width, height] = [x, y, width, height].map(saturated_f32);
        if !self.transform_invertible {
            return;
        }
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return;
        }
        if width == 0.0 && height == 0.0 {
            self.move_to(x as f64, y as f64);
            return;
        }
        self.update_for_mutation();
        add_rect_ltrb(&mut self.builder, x, y, x + width, y + height);
    }

    pub fn round_rect(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radii: &[RoundRectRadius],
    ) -> Result<(), CanvasError> {
        if radii.is_empty() || radii.len() > 4 {
            return Err(CanvasError::Range);
        }
        let [mut x, mut y, mut width, mut height] = [x, y, width, height].map(saturated_f32);
        if !self.transform_invertible {
            return Ok(());
        }
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return Ok(());
        }
        self.update_for_mutation();
        let mut r = [(0.0f32, 0.0f32); 4];
        for (slot, radius) in r.iter_mut().zip(radii) {
            match *radius {
                RoundRectRadius::Point { x: px, y: py } => {
                    let (rx, ry) = (saturated_f32(px), saturated_f32(py));
                    if !rx.is_finite() || !ry.is_finite() {
                        return Ok(());
                    }
                    if rx < 0.0 || ry < 0.0 {
                        return Err(CanvasError::Range);
                    }
                    *slot = (rx, ry);
                }
                RoundRectRadius::Uniform(value) => {
                    let a = saturated_f32(value);
                    if !a.is_finite() {
                        return Ok(());
                    }
                    if a < 0.0 {
                        return Err(CanvasError::Range);
                    }
                    *slot = (a, a);
                }
            }
        }
        if width == 0.0 || height == 0.0 {
            add_rect_ltrb(&mut self.builder, x, y, x + width, y + height);
            return Ok(());
        }
        // Row-wise: top-left, top-right, bottom-left, bottom-right.
        let mut c = [(0.0f32, 0.0f32); 4];
        match radii.len() {
            1 => c = [r[0]; 4],
            2 => {
                c[0] = r[0];
                c[3] = r[0];
                c[1] = r[1];
                c[2] = r[1];
            }
            3 => {
                c[0] = r[0];
                c[1] = r[1];
                c[2] = r[1];
                c[3] = r[2];
            }
            _ => {
                c[0] = r[0];
                c[1] = r[1];
                c[2] = r[3];
                c[3] = r[2];
            }
        }
        let mut clockwise = true;
        if width < 0.0 {
            clockwise = false;
            x += width;
            width = -width;
            c.swap(0, 1);
            c.swap(2, 3);
        }
        if height < 0.0 {
            clockwise = !clockwise;
            y += height;
            height = -height;
            c.swap(0, 2);
            c.swap(1, 3);
        }
        let rect = Rect::from_xywh(x, y, width, height);
        // FloatRoundedRect → SkRRect: setRectRadii when any corner is rounded,
        // in Skia's corner order (upper-left, upper-right, lower-right,
        // lower-left); setRect otherwise.
        let is_rounded = c.iter().any(|&(w, h)| w != 0.0 || h != 0.0);
        let rrect = if is_rounded {
            let radii = [
                Vector::new(c[0].0, c[0].1),
                Vector::new(c[1].0, c[1].1),
                Vector::new(c[3].0, c[3].1),
                Vector::new(c[2].0, c[2].1),
            ];
            RRect::new_rect_radii(rect, &radii)
        } else {
            RRect::new_rect(rect)
        };
        let dir = if clockwise {
            PathDirection::CW
        } else {
            PathDirection::CCW
        };
        self.builder.add_rrect(rrect, dir, 0);
        self.builder.move_to((x, y));
        Ok(())
    }
}

fn bounding(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    // gfx::BoundingRect(p0, p1).
    let rx = a.0.min(b.0);
    let ry = a.1.min(b.1);
    let rr = a.0.max(b.0);
    let rb = a.1.max(b.1);
    (rx, ry, rr - rx, rb - ry)
}

fn add_rect_ltrb(builder: &mut SkPathBuilder, l: f32, t: f32, r: f32, b: f32) {
    builder.add_rect(Rect::new(l, t, r, b), PathDirection::CW, 0);
}

/// `PathBuilder::AddEllipse(p, rx, ry, start, end)`.
fn add_ellipse(
    builder: &mut SkPathBuilder,
    p: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    start_angle: f32,
    end_angle: f32,
) {
    let oval = Rect::new(
        p.0 - radius_x,
        p.1 - radius_y,
        p.0 + radius_x,
        p.1 + radius_y,
    );
    let start_degrees = rad2deg_f32(start_angle);
    let sweep_degrees = rad2deg_f32(end_angle - start_angle);
    add_ellipse_degrees(builder, oval, start_degrees, sweep_degrees);
}

/// The arc part of `PathBuilder::AddEllipse`, in degrees: a sweep within
/// 1/4096 of ±360 is drawn as two half arcs.
fn add_ellipse_degrees(
    builder: &mut SkPathBuilder,
    oval: Rect,
    start_degrees: f32,
    sweep_degrees: f32,
) {
    if float_nearly_equal(sweep_degrees.abs(), 360.0) {
        builder.inc_reserve(10, 5, 4);
        let sweep180 = 180.0f32.copysign(sweep_degrees);
        builder.arc_to(oval, start_degrees, sweep180, false);
        builder.arc_to(oval, start_degrees + sweep180, sweep180, false);
    } else {
        builder.arc_to(oval, start_degrees, sweep_degrees, false);
    }
}

fn canonicalize_angle(start_angle: &mut f32, end_angle: &mut f32) {
    let mut new_start_angle = fmodf(*start_angle, TWO_PI_FLOAT);
    if new_start_angle < 0.0 {
        new_start_angle += TWO_PI_FLOAT;
        if new_start_angle >= TWO_PI_FLOAT {
            new_start_angle -= TWO_PI_FLOAT;
        }
    }
    let delta = new_start_angle - *start_angle;
    *start_angle = new_start_angle;
    *end_angle += delta;
}

fn adjust_end_angle(start_angle: f32, end_angle: f32, anticlockwise: bool) -> f32 {
    if !anticlockwise && end_angle - start_angle >= TWO_PI_FLOAT {
        start_angle + TWO_PI_FLOAT
    } else if anticlockwise && start_angle - end_angle >= TWO_PI_FLOAT {
        start_angle - TWO_PI_FLOAT
    } else if !anticlockwise && start_angle > end_angle {
        start_angle + (TWO_PI_FLOAT - fmodf(start_angle - end_angle, TWO_PI_FLOAT))
    } else if anticlockwise && start_angle < end_angle {
        start_angle - (TWO_PI_FLOAT - fmodf(end_angle - start_angle, TWO_PI_FLOAT))
    } else {
        end_angle
    }
}
