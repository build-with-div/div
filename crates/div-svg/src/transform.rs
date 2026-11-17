//! 2D affine matrices and the `transform` attribute's list syntax
//! (CSS Transforms 1 §6.1, SVG 1.1 §7.6).

use crate::number::Scanner;
use crate::Point;

/// The matrix `[a c e; b d f; 0 0 1]`, as SVG's `matrix(a b c d e f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub const fn translate(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    pub const fn scale(x: f64, y: f64) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    pub fn rotate(degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::new(c, s, -s, c, 0.0, 0.0)
    }

    pub fn skew_x(degrees: f64) -> Self {
        Self::new(1.0, 0.0, degrees.to_radians().tan(), 1.0, 0.0, 0.0)
    }

    pub fn skew_y(degrees: f64) -> Self {
        Self::new(1.0, degrees.to_radians().tan(), 0.0, 1.0, 0.0, 0.0)
    }

    /// `self · other`: `other` applies first.
    pub fn then_apply(&self, other: &Self) -> Self {
        Self::new(
            self.a * other.a + self.c * other.b,
            self.b * other.a + self.d * other.b,
            self.a * other.c + self.c * other.d,
            self.b * other.c + self.d * other.d,
            self.a * other.e + self.c * other.f + self.e,
            self.b * other.e + self.d * other.f + self.f,
        )
    }

    pub fn map(&self, p: Point) -> Point {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        Point::new(
            (self.a * x + self.c * y + self.e) as f32,
            (self.b * x + self.d * y + self.f) as f32,
        )
    }

    /// The inverse, in double precision, or `None` when the matrix is not
    /// invertible (a zero or non-finite determinant).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Self::new(
            a,
            b,
            c,
            d,
            -(a * self.e + c * self.f),
            -(b * self.e + d * self.f),
        ))
    }

    /// The six values in single precision, as Chromium reports them.
    pub fn to_f32(&self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f].map(|v| v as f32)
    }
}

/// Parses a `transform` attribute. `None` for invalid syntax, in which case
/// the whole attribute is ignored (the corpus's `transform/invalid`).
pub fn parse_transform_list(text: &str) -> Option<Matrix> {
    let mut scanner = Scanner::new(text);
    let mut result = Matrix::IDENTITY;
    scanner.skip_ws();
    while !scanner.at_end() {
        let name_start = scanner.pos;
        while scanner.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
            scanner.pos += 1;
        }
        let name = text.get(name_start..scanner.pos)?;
        scanner.skip_ws();
        if !scanner.eat("(") {
            return None;
        }
        scanner.skip_ws();
        let mut args = Vec::new();
        while let Some(value) = scanner.number_sep() {
            args.push(f64::from(value));
        }
        if !scanner.eat(")") {
            return None;
        }
        let m = match (name, args.as_slice()) {
            ("matrix", &[a, b, c, d, e, f]) => Matrix::new(a, b, c, d, e, f),
            ("translate", &[x]) => Matrix::translate(x, 0.0),
            ("translate", &[x, y]) => Matrix::translate(x, y),
            ("scale", &[s]) => Matrix::scale(s, s),
            ("scale", &[x, y]) => Matrix::scale(x, y),
            ("rotate", &[a]) => Matrix::rotate(a),
            ("rotate", &[a, x, y]) => Matrix::translate(x, y)
                .then_apply(&Matrix::rotate(a))
                .then_apply(&Matrix::translate(-x, -y)),
            ("skewX", &[a]) => Matrix::skew_x(a),
            ("skewY", &[a]) => Matrix::skew_y(a),
            _ => return None,
        };
        result = result.then_apply(&m);
        scanner.skip_comma_ws();
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{parse_transform_list, Matrix};

    #[test]
    fn parses_lists_with_commas() {
        let m = parse_transform_list("translate(10,20),scale(2,0.5)").unwrap();
        assert_eq!(m, Matrix::new(2.0, 0.0, 0.0, 0.5, 10.0, 20.0));
    }

    #[test]
    fn rejects_the_whole_list_on_an_unknown_function() {
        assert_eq!(parse_transform_list("translate(10 20) bogus(3)"), None);
        assert_eq!(parse_transform_list("rotate(1 2)"), None);
    }

    #[test]
    fn inverts_affine_matrices() {
        let m = Matrix::new(2.0, 0.0, 0.0, 4.0, 10.0, 20.0);
        assert_eq!(
            m.inverse(),
            Some(Matrix::new(0.5, 0.0, 0.0, 0.25, -5.0, -5.0))
        );
        assert_eq!(Matrix::scale(3.0, 0.0).inverse(), None);
    }
}
