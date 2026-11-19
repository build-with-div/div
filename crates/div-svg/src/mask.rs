//! `mask` (RFC 0009 V3c): masking an element with a resolved
//! `mask: url(#id)` reference.
//!
//! Ported from Blink's BSD `core/paint/svg_mask_painter.cc` and
//! `css_mask_painter.cc`:
//!
//! - the mask layer (`PaintSVGMaskLayer`): clipped to the mask region, the
//!   content drawn under the content transform (`MaskToContentTransform`),
//!   through a luminance-to-alpha colour filter (`ColorFilter::MakeLuma`)
//!   unless `mask-type` is `alpha`;
//! - the mask clip of an SVG element (`ResourceBoundsForSVGChild`): the
//!   mask region united with the element's visual rect;
//! - the reference box: the element's fill box;
//! - a single mask reference that does not resolve leaves the element
//!   unmasked (`HasSingleInvalidSVGMaskLayer`).
//!
//! Written from SVG 2 and CSS Masking and black-box observation of
//! Chromium 147 (Blink's resource masker and `mask` element are LGPL and
//! are not read), checked against Chromium by `tests/paint_corpus.rs`
//! (`tests/fidelity/fixtures/svg-paint-v5`):
//!
//! - **The mask region.** `x`, `y`, `width` and `height` in `maskUnits`,
//!   initially `objectBoundingBox` -10%, -10%, 120% and 120%. In
//!   `objectBoundingBox` units a number or a percentage is a fraction of
//!   the reference box. A region of zero or negative size, or an
//!   `objectBoundingBox` region on an empty reference box, masks the
//!   element out entirely.
//! - **The content transform.** For `maskContentUnits="objectBoundingBox"`
//!   the map from the unit square to the reference box, otherwise none.
//!
//! The content is the caller's: the `mask` element's children, painted in
//! the content's own user space, recorded into a picture.

use crate::length::{Axis, Length, LengthContext, Unit};
use crate::paint::to_sk_matrix;
use crate::paint_server::{units_context, Units};
use crate::transform::Matrix;
use crate::Rect;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::{BlendMode, Canvas, ClipOp, ColorFilter, Paint, Picture, Rect as SkRect};

/// `mask-type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaskType {
    #[default]
    Luminance,
    Alpha,
}

impl MaskType {
    /// Parses the property; `None` for anything else, which leaves it
    /// unset.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "luminance" => Some(Self::Luminance),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }
}

/// A `mask` element's attributes as set on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaskAttributes {
    pub units: Option<Units>,
    pub content_units: Option<Units>,
    pub x: Option<Length>,
    pub y: Option<Length>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub mask_type: Option<MaskType>,
}

impl MaskAttributes {
    /// The mask with the initial values for what is unset and its region
    /// resolved (against `context` for `userSpaceOnUse`).
    pub fn resolve<T>(&self, context: &LengthContext, content: T) -> Mask<T> {
        let units = self.units.unwrap_or(Units::ObjectBoundingBox);
        let context = units_context(units, context);
        let percent = |value| Length {
            value,
            unit: Unit::Percent,
        };
        let len = |length: Option<Length>, initial: f32, axis: Axis| {
            length.unwrap_or(percent(initial)).resolve(axis, &context)
        };
        Mask {
            units,
            content_units: self.content_units.unwrap_or(Units::UserSpaceOnUse),
            region: Rect::new(
                len(self.x, -10.0, Axis::Horizontal),
                len(self.y, -10.0, Axis::Vertical),
                len(self.width, 120.0, Axis::Horizontal),
                len(self.height, 120.0, Axis::Vertical),
            ),
            mask_type: self.mask_type.unwrap_or_default(),
            content,
        }
    }
}

/// A resolved mask. `content` is its children in the content's own user
/// space.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask<T> {
    pub units: Units,
    pub content_units: Units,
    /// `x`, `y`, `width` and `height` in `units`.
    pub region: Rect,
    pub mask_type: MaskType,
    pub content: T,
}

impl<T> Mask<T> {
    /// The mask region in the masked element's user space, or `None` when
    /// it masks the element out entirely.
    pub fn region(&self, reference_box: Rect) -> Option<Rect> {
        let region = match self.units {
            Units::UserSpaceOnUse => self.region,
            Units::ObjectBoundingBox => {
                let b = reference_box;
                if !(b.width > 0.0 && b.height > 0.0) {
                    return None;
                }
                Rect::new(
                    b.x + self.region.x * b.width,
                    b.y + self.region.y * b.height,
                    self.region.width * b.width,
                    self.region.height * b.height,
                )
            }
        };
        (region.width > 0.0 && region.height > 0.0).then_some(region)
    }

    /// `MaskToContentTransform` at zoom 1: the map from the content's user
    /// space to the masked element's.
    pub fn content_transform(&self, reference_box: Rect) -> Matrix {
        match self.content_units {
            Units::UserSpaceOnUse => Matrix::IDENTITY,
            Units::ObjectBoundingBox => {
                Matrix::translate(f64::from(reference_box.x), f64::from(reference_box.y))
                    .then_apply(&Matrix::scale(
                        f64::from(reference_box.width),
                        f64::from(reference_box.height),
                    ))
            }
        }
    }
}

fn sk_rect(rect: Rect) -> SkRect {
    SkRect::from_xywh(rect.x, rect.y, rect.width, rect.height)
}

impl Mask<Picture> {
    /// `PaintSVGMaskLayer` for the bottom (only) mask layer: the mask's
    /// coverage in the masked element's user space, the canvas's current
    /// one.
    pub fn paint_mask(&self, canvas: &Canvas, reference_box: Rect) {
        let Some(region) = self.region(reference_box) else {
            return;
        };
        canvas.save();
        canvas.clip_rect(sk_rect(region), ClipOp::Intersect, false);
        let luminance = self.mask_type == MaskType::Luminance;
        if luminance {
            let mut paint = Paint::default();
            paint.set_color_filter(ColorFilter::luma());
            canvas.save_layer(&SaveLayerRec::default().paint(&paint));
        }
        canvas.concat(&to_sk_matrix(&self.content_transform(reference_box)));
        canvas.draw_picture(&self.content, None, None);
        if luminance {
            canvas.restore();
        }
        canvas.restore();
    }
}

/// Paints `content` masked by `mask`, in the masked element's user space,
/// which is the canvas's current one. `reference_box` is the element's
/// fill box and `visual_rect` its visual rect (strokes included).
///
/// The content is drawn into a layer and the mask is composited onto it
/// with `DstIn`, both clipped to the mask clip: the mask region united
/// with the visual rect.
pub fn mask_content(
    canvas: &Canvas,
    mask: &Mask<Picture>,
    reference_box: Rect,
    visual_rect: Rect,
    content: impl FnOnce(&Canvas),
) {
    let Some(region) = mask.region(reference_box) else {
        return;
    };
    let mut clip = sk_rect(region);
    clip.join(sk_rect(visual_rect));
    canvas.save();
    canvas.clip_rect(clip, ClipOp::Intersect, true);
    canvas.save_layer(&SaveLayerRec::default());
    let matrix = canvas.local_to_device();
    content(canvas);
    canvas.set_matrix(&matrix);
    let mut paint = Paint::default();
    paint.set_blend_mode(BlendMode::DstIn);
    canvas.save_layer(&SaveLayerRec::default().paint(&paint));
    mask.paint_mask(canvas, reference_box);
    canvas.restore();
    canvas.restore();
    canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> LengthContext {
        LengthContext {
            viewport_width: 200.0,
            viewport_height: 100.0,
            font_size: 16.0,
            root_font_size: 16.0,
            x_height: 8.0,
        }
    }

    fn assert_near(a: Rect, b: Rect) {
        let near = |x: f32, y: f32| (x - y).abs() < 1e-4;
        assert!(
            near(a.x, b.x) && near(a.y, b.y) && near(a.width, b.width) && near(a.height, b.height),
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn the_initial_region_is_ten_percent_outside_the_box() {
        let mask = MaskAttributes::default().resolve(&context(), ());
        let region = mask.region(Rect::new(10.0, 20.0, 50.0, 40.0)).unwrap();
        assert_near(region, Rect::new(5.0, 16.0, 60.0, 48.0));
        assert_eq!(mask.mask_type, MaskType::Luminance);
    }

    #[test]
    fn user_space_regions_resolve_against_the_viewport() {
        let attributes = MaskAttributes {
            units: Some(Units::UserSpaceOnUse),
            x: Length::parse("10%"),
            width: Length::parse("50%"),
            ..MaskAttributes::default()
        };
        let mask = attributes.resolve(&context(), ());
        let region = mask.region(Rect::new(0.0, 0.0, 1.0, 1.0)).unwrap();
        assert_near(region, Rect::new(20.0, -10.0, 100.0, 120.0));
    }

    #[test]
    fn empty_regions_and_boxes_mask_everything() {
        let zero = MaskAttributes {
            width: Length::parse("0"),
            ..MaskAttributes::default()
        };
        let mask = zero.resolve(&context(), ());
        assert_eq!(mask.region(Rect::new(0.0, 0.0, 10.0, 10.0)), None);
        let mask = MaskAttributes::default().resolve(&context(), ());
        assert_eq!(mask.region(Rect::new(0.0, 0.0, 10.0, 0.0)), None);
    }
}
