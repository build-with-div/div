//! SVG lengths in attributes: a number with an optional unit, resolved to
//! user units (SVG 2 §8.6, CSS Values 4).

use crate::number::Scanner;

/// Which viewport dimension a percentage refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
    /// `sqrt((w² + h²) / 2)`, for radii and other non-directional lengths.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Number,
    Px,
    Percent,
    Em,
    Ex,
    Rem,
    Cm,
    Mm,
    Q,
    In,
    Pt,
    Pc,
}

/// A parsed length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: Unit,
}

/// What a length resolves against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LengthContext {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub font_size: f32,
    pub root_font_size: f32,
    /// The x-height in px, from the computed font.
    pub x_height: f32,
}

impl Length {
    /// Parses an attribute value such as `10`, `2.5mm` or `40%`. `None` for
    /// anything else (including `auto`, which callers handle).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
        let mut scanner = Scanner::new(text);
        let value = scanner.number()?;
        let unit = match &text[scanner.pos..] {
            "" => Unit::Number,
            "px" => Unit::Px,
            "%" => Unit::Percent,
            "em" => Unit::Em,
            "ex" => Unit::Ex,
            "rem" => Unit::Rem,
            "cm" => Unit::Cm,
            "mm" => Unit::Mm,
            "q" | "Q" => Unit::Q,
            "in" => Unit::In,
            "pt" => Unit::Pt,
            "pc" => Unit::Pc,
            _ => return None,
        };
        Some(Self { value, unit })
    }

    /// The value in user units.
    pub fn resolve(&self, axis: Axis, context: &LengthContext) -> f32 {
        let v = self.value;
        match self.unit {
            Unit::Number | Unit::Px => v,
            Unit::Percent => {
                let (w, h) = (context.viewport_width, context.viewport_height);
                let base = match axis {
                    Axis::Horizontal => w,
                    Axis::Vertical => h,
                    Axis::Other => ((w * w + h * h) / 2.0).sqrt(),
                };
                v * base / 100.0
            }
            Unit::Em => v * context.font_size,
            Unit::Ex => v * context.x_height,
            Unit::Rem => v * context.root_font_size,
            Unit::Cm => v * 96.0 / 2.54,
            Unit::Mm => v * 96.0 / 25.4,
            Unit::Q => v * 96.0 / 101.6,
            Unit::In => v * 96.0,
            Unit::Pt => v * 4.0 / 3.0,
            Unit::Pc => v * 16.0,
        }
    }
}
