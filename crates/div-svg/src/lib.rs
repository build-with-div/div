//! SVG geometry for Div ([RFC 0009](../../docs/alpha/rfcs/0009-inline-svg.md)).
//!
//! Path data, basic shapes, lengths, transform lists and viewport transforms,
//! producing what Chromium 147 produces. Blink's code for these is LGPL, so
//! none of it is ported or read: the behaviour comes from SVG 2 and is
//! checked against Chromium as a black box by `tests/geometry_corpus.rs`
//! (oracle and known gaps: `tests/fidelity/fixtures/svg-geometry-v1`).
//!
//! The crate depends on Skia for arcs, path measurement, containment and
//! stroking, which Chromium also does with Skia. It has no dependency on
//! the DOM, style, layout or paint.

pub mod geometry;
pub mod length;
mod number;
pub mod path_data;
pub mod shapes;
pub mod transform;
pub mod viewport;

/// A point in user units. Chromium keeps SVG geometry in single precision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in user units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub use path_data::{PathData, Segment};
pub use transform::Matrix;
