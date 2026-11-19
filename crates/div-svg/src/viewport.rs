//! `viewBox` and `preserveAspectRatio` (SVG 2 §8.2, §8.5, §8.6): the
//! transform from a viewport's user space to its parent's coordinates.

use crate::length::{Axis, Length, LengthContext, Unit};
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
    scan_view_box(text).filter(|vb| vb.width > 0.0 && vb.height > 0.0)
}

/// Whether `viewBox` is valid with a zero width or height. On a nested
/// viewport (an inner `svg`, or the one a `use` makes of a `symbol`) that
/// disables rendering (Blink's BSD `svg_container_painter.cc`); on the
/// outermost `svg` the attribute is ignored ([`parse_view_box`]).
pub fn is_empty_view_box(text: &str) -> bool {
    scan_view_box(text).is_some_and(|vb| vb.width == 0.0 || vb.height == 0.0)
}

/// The four numbers, with a negative width or height an error.
fn scan_view_box(text: &str) -> Option<Rect> {
    let mut scanner = Scanner::new(text);
    scanner.skip_ws();
    let mut values = [0.0f32; 4];
    for value in &mut values {
        *value = scanner.number_sep()?;
    }
    scanner.skip_ws();
    if !scanner.at_end() || values[2] < 0.0 || values[3] < 0.0 {
        return None;
    }
    Some(Rect::new(values[0], values[1], values[2], values[3]))
}

/// What a nested viewport element states: an inner `svg`, or the `svg` a
/// `use` makes of a `symbol` or an `svg`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewportAttributes {
    pub x: Option<Length>,
    pub y: Option<Length>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// A valid `viewBox` of positive size.
    pub view_box: Option<Rect>,
    /// A valid `viewBox` of zero width or height ([`is_empty_view_box`]).
    pub empty_view_box: bool,
    pub preserve_aspect_ratio: PreserveAspectRatio,
}

impl ViewportAttributes {
    /// The viewport, with `x` and `y` initially 0 and `width` and `height`
    /// initially 100%, resolved against `context`; `None` when an empty
    /// `viewBox` disables rendering.
    pub fn resolve(&self, context: &LengthContext) -> Option<NestedViewport> {
        if self.empty_view_box {
            return None;
        }
        let hundred = Length {
            value: 100.0,
            unit: Unit::Percent,
        };
        let zero = Length {
            value: 0.0,
            unit: Unit::Number,
        };
        let len = |length: Option<Length>, initial: Length, axis: Axis| {
            length.unwrap_or(initial).resolve(axis, context)
        };
        Some(NestedViewport {
            viewport: Rect::new(
                len(self.x, zero, Axis::Horizontal),
                len(self.y, zero, Axis::Vertical),
                len(self.width, hundred, Axis::Horizontal),
                len(self.height, hundred, Axis::Vertical),
            ),
            view_box: self.view_box,
            preserve_aspect_ratio: self.preserve_aspect_ratio,
        })
    }
}

/// A resolved nested viewport. Its content is clipped to `viewport` unless
/// `overflow` is visible, which is the caller's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NestedViewport {
    /// `x`, `y`, `width` and `height` in the parent's user space.
    pub viewport: Rect,
    pub view_box: Option<Rect>,
    pub preserve_aspect_ratio: PreserveAspectRatio,
}

impl NestedViewport {
    /// The map from the content's user space to the parent's: the
    /// translation to the viewport's origin, then the `viewBox` transform.
    pub fn content_transform(&self) -> Matrix {
        let v = self.viewport;
        let origin = Matrix::translate(f64::from(v.x), f64::from(v.y));
        match self.view_box {
            Some(vb) => origin.then_apply(&view_box_transform(
                vb,
                self.preserve_aspect_ratio,
                v.width,
                v.height,
            )),
            None => origin,
        }
    }

    /// The size percentages in the content resolve against: the `viewBox`'s,
    /// else the viewport's.
    pub fn content_size(&self) -> (f32, f32) {
        let size = self.view_box.unwrap_or(self.viewport);
        (size.width, size.height)
    }
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
    fn nested_viewports_default_to_the_parent_size() {
        let context = LengthContext {
            viewport_width: 64.0,
            viewport_height: 32.0,
            font_size: 16.0,
            root_font_size: 16.0,
            x_height: 8.0,
        };
        let attributes = ViewportAttributes {
            x: Length::parse("8"),
            view_box: parse_view_box("0 0 16 16"),
            ..ViewportAttributes::default()
        };
        let viewport = attributes.resolve(&context).unwrap();
        assert_eq!(viewport.viewport, Rect::new(8.0, 0.0, 64.0, 32.0));
        assert_eq!(viewport.content_size(), (16.0, 16.0));
        // 16 units into 64 × 32, meet: scale 2, centred horizontally.
        assert_eq!(
            viewport.content_transform(),
            Matrix::new(2.0, 0.0, 0.0, 2.0, 24.0, 0.0)
        );
        let empty = ViewportAttributes {
            empty_view_box: is_empty_view_box("0 0 0 10"),
            ..ViewportAttributes::default()
        };
        assert_eq!(empty.resolve(&context), None);
        assert!(!is_empty_view_box("0 0 -1 10"));
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
