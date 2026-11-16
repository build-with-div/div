//! SVG path data (SVG 2 §9.3), parsed into absolute segments.
//!
//! Relative commands become absolute, `H` and `V` become lines, `S` and `T`
//! get their reflected control points, and arcs keep their endpoint
//! parameters for the path builder. Error handling follows SVG 2 §9.5.4: the path renders up
//! to the last complete segment before the first error, and a path that does
//! not begin with a move renders nothing.

use crate::number::Scanner;
use crate::Point;

/// One absolute path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    /// An elliptical arc in endpoint parameterization (SVG 2 §9.3.8), as
    /// written: radii may be negative, zero or too small.
    ArcTo {
        radii: Point,
        rotation: f32,
        large: bool,
        sweep: bool,
        to: Point,
    },
    Close,
}

/// Parsed path data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathData {
    pub segments: Vec<Segment>,
    /// Whether parsing stopped at an error. The segments before it are kept.
    pub had_error: bool,
}

impl PathData {
    pub fn parse(text: &str) -> Self {
        let mut parser = Parser::new(text);
        let had_error = !parser.run();
        Self {
            segments: parser.segments,
            had_error,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

struct Parser<'a> {
    scanner: Scanner<'a>,
    segments: Vec<Segment>,
    current: Point,
    subpath_start: Point,
    /// The second control point of the previous cubic, for `S`.
    last_cubic: Option<Point>,
    /// The control point of the previous quadratic, for `T`.
    last_quad: Option<Point>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            scanner: Scanner::new(text),
            segments: Vec::new(),
            current: Point::default(),
            subpath_start: Point::default(),
            last_cubic: None,
            last_quad: None,
        }
    }

    /// Parses the whole string; returns false if it stopped at an error.
    fn run(&mut self) -> bool {
        self.scanner.skip_ws();
        if self.scanner.at_end() {
            return true;
        }
        if !matches!(self.scanner.peek(), Some(b'M' | b'm')) {
            return false;
        }
        let mut command = 0u8;
        loop {
            self.scanner.skip_ws();
            if self.scanner.at_end() {
                return true;
            }
            let byte = self.scanner.peek().unwrap_or(0);
            if byte.is_ascii_alphabetic() {
                command = byte;
                self.scanner.pos += 1;
                self.scanner.skip_ws();
            } else if !self.scanner.starts_number() || command == 0 {
                return false;
            } else if matches!(command, b'Z' | b'z') {
                // A number cannot follow a close path.
                return false;
            }
            if !self.segment(command) {
                return false;
            }
            // After a move, further coordinate pairs are implicit lines.
            command = match command {
                b'M' => b'L',
                b'm' => b'l',
                other => other,
            };
        }
    }

    fn point(&mut self, relative: bool) -> Option<Point> {
        let x = self.scanner.number_sep()?;
        let y = self.scanner.number_sep()?;
        Some(if relative {
            Point::new(self.current.x + x, self.current.y + y)
        } else {
            Point::new(x, y)
        })
    }

    /// Parses one segment of `command`. Nothing is emitted unless the whole
    /// segment parses.
    fn segment(&mut self, command: u8) -> bool {
        let relative = command.is_ascii_lowercase();
        let (cubic, quad) = (self.last_cubic.take(), self.last_quad.take());
        match command.to_ascii_uppercase() {
            b'M' => {
                let Some(p) = self.point(relative) else {
                    return false;
                };
                self.segments.push(Segment::MoveTo(p));
                self.current = p;
                self.subpath_start = p;
            }
            b'L' => {
                let Some(p) = self.point(relative) else {
                    return false;
                };
                self.line_to(p);
            }
            b'H' => {
                let Some(x) = self.scanner.number_sep() else {
                    return false;
                };
                let x = if relative { self.current.x + x } else { x };
                self.line_to(Point::new(x, self.current.y));
            }
            b'V' => {
                let Some(y) = self.scanner.number_sep() else {
                    return false;
                };
                let y = if relative { self.current.y + y } else { y };
                self.line_to(Point::new(self.current.x, y));
            }
            b'C' => {
                let (Some(c1), Some(c2), Some(p)) = (
                    self.point(relative),
                    self.point(relative),
                    self.point(relative),
                ) else {
                    return false;
                };
                self.cubic_to(c1, c2, p);
            }
            b'S' => {
                let c1 = reflect(cubic, self.current);
                let (Some(c2), Some(p)) = (self.point(relative), self.point(relative)) else {
                    return false;
                };
                self.cubic_to(c1, c2, p);
            }
            b'Q' => {
                let (Some(c), Some(p)) = (self.point(relative), self.point(relative)) else {
                    return false;
                };
                self.quad_to(c, p);
            }
            b'T' => {
                let c = reflect(quad, self.current);
                let Some(p) = self.point(relative) else {
                    return false;
                };
                self.quad_to(c, p);
            }
            b'A' => {
                let s = &mut self.scanner;
                let Some(rx) = s.number_sep() else {
                    return false;
                };
                let Some(ry) = s.number_sep() else {
                    return false;
                };
                let Some(angle) = s.number_sep() else {
                    return false;
                };
                let Some(large) = s.flag() else { return false };
                s.skip_comma_ws();
                let Some(sweep) = s.flag() else { return false };
                s.skip_comma_ws();
                let Some(p) = self.point(relative) else {
                    return false;
                };
                self.segments.push(Segment::ArcTo {
                    radii: Point::new(rx, ry),
                    rotation: angle,
                    large,
                    sweep,
                    to: p,
                });
                self.current = p;
            }
            b'Z' => {
                self.segments.push(Segment::Close);
                self.current = self.subpath_start;
            }
            _ => return false,
        }
        true
    }

    fn line_to(&mut self, p: Point) {
        self.segments.push(Segment::LineTo(p));
        self.current = p;
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.segments.push(Segment::CubicTo(c1, c2, p));
        self.current = p;
        self.last_cubic = Some(c2);
    }

    fn quad_to(&mut self, c: Point, p: Point) {
        self.segments.push(Segment::QuadTo(c, p));
        self.current = p;
        self.last_quad = Some(c);
    }
}

/// The reflection of the previous control point about the current point,
/// or the current point when the previous segment was of another kind.
fn reflect(control: Option<Point>, current: Point) -> Point {
    match control {
        Some(c) => Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y),
        None => current,
    }
}

#[cfg(test)]
mod tests {
    use super::{PathData, Segment};
    use crate::Point;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn relative_and_implicit_commands() {
        let path = PathData::parse("m10 10 40 40 h10 v-5 z");
        assert_eq!(
            path.segments,
            [
                Segment::MoveTo(p(10.0, 10.0)),
                Segment::LineTo(p(50.0, 50.0)),
                Segment::LineTo(p(60.0, 50.0)),
                Segment::LineTo(p(60.0, 45.0)),
                Segment::Close,
            ]
        );
        assert!(!path.had_error);
    }

    #[test]
    fn stops_at_the_first_error() {
        let path = PathData::parse("M10 10 L190 10 X L190 190");
        assert_eq!(path.segments.len(), 2);
        assert!(path.had_error);
        let path = PathData::parse("M10 10 L190 10 L190");
        assert_eq!(path.segments.len(), 2);
        assert!(PathData::parse("L190 10").segments.is_empty());
    }

    #[test]
    fn smooth_commands_reflect_only_matching_kinds() {
        let path = PathData::parse("M10 100 S100 10 190 100");
        assert_eq!(
            path.segments[1],
            Segment::CubicTo(p(10.0, 100.0), p(100.0, 10.0), p(190.0, 100.0))
        );
    }
}
