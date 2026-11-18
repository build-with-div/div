//! SVG geometry and painting for Div
//! ([RFC 0009](../../docs/alpha/rfcs/0009-inline-svg.md)).
//!
//! Path data, basic shapes, lengths, transform lists and viewport transforms,
//! producing what Chromium 147 produces. Blink's code for these is LGPL, so
//! none of it is ported or read: the behaviour comes from SVG 2 and is
//! checked against Chromium as a black box by `tests/geometry_corpus.rs`
//! (oracle and known gaps: `tests/fidelity/fixtures/svg-geometry-v1`).
//!
//! [`paint`] paints shapes onto a Skia canvas. It is ported from Blink's
//! SVG painters, which are BSD-licensed, and is checked pixel for pixel
//! against Chromium by `tests/paint_corpus.rs`
//! (`tests/fidelity/fixtures/svg-paint-v1` and `svg-paint-v2`).
//!
//! [`clip`] clips content to a resolved `clipPath` (V3a). It ports Blink's
//! BSD `clip_path_clipper` and writes the rest from the specifications and
//! black-box observation, checked by the same test
//! (`tests/fidelity/fixtures/svg-paint-v3`).
//!
//! [`paint_server`] builds the shaders of `linearGradient`, `radialGradient`
//! and `pattern` (V3b), ported from Blink's BSD gradient and paint-record
//! pattern code and written from SVG 2 for the rest, checked by the same
//! test (`tests/fidelity/fixtures/svg-paint-v4`).
//!
//! [`text`] lays out `<text>` and `<tspan>`, answers the SVG text DOM
//! queries (character positions, extents, rotations and the computed text
//! length) and paints the glyphs. It ports Blink's BSD SVG text layout
//! algorithm, query code and text fragment painter; the LGPL parts
//! (attribute resolution, the scaled-font rule) are written from SVG 2 and
//! black-box observation. `tests/text_corpus.rs` checks the positions
//! exactly and the pixels byte for byte against Chromium per OS
//! (`tests/fidelity/fixtures/svg-text-v1`).
//!
//! The crate depends on Skia for arcs, path measurement, containment,
//! stroking and drawing, which Chromium also does with Skia, and on
//! `div-text` (with `div-style` for its font types) to shape text. It has
//! no dependency on the DOM, layout or Div's paint crate.

mod arc;
pub mod clip;
pub mod geometry;
pub mod length;
mod number;
pub mod paint;
pub mod paint_server;
pub mod path_data;
pub mod shapes;
pub mod text;
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
