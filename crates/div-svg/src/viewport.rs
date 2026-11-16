//! `viewBox` and `preserveAspectRatio` (SVG 2 §8.2, §8.5, §8.6): the
//! transform from a viewport's user space to its parent's coordinates.

use crate::number::Scanner;
use crate::transform::Matrix;
use crate::Rect;

/// `preserveAspectRatio`'s alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    /// `x`/`y` are 0 (min), 1 (mid) or 2 (max).
    Fit {
        x: u8,
        y: u8,
    },
}

/// `preserveAspectRatio`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreserveAspectRatio {
    pub align: Align,
    pub slice: bool,
}

impl Default for PreserveAspectRatio {
    /// `xMidYMid meet`.
    fn default() -> Self {
        Self {
            align: Align::Fit { x: 1, y: 1 },
            slice: false,
        }
    }
}

impl PreserveAspectRatio {
    /// Parses the attribute; `None` for invalid syntax (the initial value
    /// then applies).
    pub fn parse(text: &str) -> Option<Self> {
        let mut words = text.split_ascii_whitespace();
        let align = match words.next()? {
            "none" => Align::None,
            word => {
                let rest = word.strip_prefix('x')?;
                let (x, rest) = axis(rest)?;
                let rest = rest.strip_prefix('Y')?;
                let (y, rest) = axis(rest)?;
                if !rest.is_empty() {
                    return None;
                }
                Align::Fit { x, y }
            }
        };
        let slice = match words.next() {
            None | Some("meet") => false,
            Some("slice") => true,
            Some(_) => return None,
        };
        if words.next().is_some() {
            return None;
        }
        Some(Self { align, slice })
    }
}

fn axis(text: &str) -> Option<(u8, &str)> {
    for (index, name) in ["Min", "Mid", "Max"].iter().enumerate() {
        if let Some(rest) = text.strip_prefix(name) {
            return Some((index as u8, rest));
        }
    }
    None
}

/// Parses `viewBox`. A negative width or height is an error, and so is a
/// zero one: the corpus shows Chromium then ignores the attribute.
pub fn parse_view_box(text: &str) -> Option<Rect> {
    let mut scanner = Scanner::new(text);
    scanner.skip_ws();
    let mut values = [0.0f32; 4];
    for value in &mut values {
        *value = scanner.number_sep()?;
    }
    scanner.skip_ws();
    if !scanner.at_end() || values[2] <= 0.0 || values[3] <= 0.0 {
        return None;
    }
    Some(Rect::new(values[0], values[1], values[2], values[3]))
}

/// The transform that maps `view_box` into a viewport of `width` × `height`
/// at the origin (SVG 2 §8.2, "equivalent transform of an SVG viewport").
pub fn view_box_transform(
    view_box: Rect,
    par: PreserveAspectRatio,
    width: f32,
    height: f32,
) -> Matrix {
    let (vx, vy) = (f64::from(view_box.x), f64::from(view_box.y));
    let (vw, vh) = (f64::from(view_box.width), f64::from(view_box.height));
    let (w, h) = (f64::from(width), f64::from(height));
    let mut sx = w / vw;
    let mut sy = h / vh;
    let Align::Fit { x, y } = par.align else {
        return Matrix::new(sx, 0.0, 0.0, sy, -vx * sx, -vy * sy);
    };
    let s = if par.slice { sx.max(sy) } else { sx.min(sy) };
    sx = s;
    sy = s;
    let mut tx = -vx * sx;
    let mut ty = -vy * sy;
    let offset = |free: f64, align: u8| match align {
        1 => free / 2.0,
        2 => free,
        _ => 0.0,
    };
    tx += offset(w - vw * sx, x);
    ty += offset(h - vh * sy, y);
    Matrix::new(sx, 0.0, 0.0, sy, tx, ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alignments() {
        assert_eq!(
            PreserveAspectRatio::parse("xMaxYMin slice"),
            Some(PreserveAspectRatio {
                align: Align::Fit { x: 2, y: 0 },
                slice: true
            })
        );
        assert_eq!(PreserveAspectRatio::parse("xMaxymin"), None);
    }

    #[test]
    fn rejects_empty_view_boxes() {
        assert_eq!(parse_view_box("0 0 0 50"), None);
        assert_eq!(
            parse_view_box("0,0,100,50"),
            Some(Rect::new(0.0, 0.0, 100.0, 50.0))
        );
    }
}
