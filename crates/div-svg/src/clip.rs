//! `clipPath` (RFC 0009 V3a): clipping an element to a resolved
//! `clip-path: url(#id)` reference.
//!
//! Ported from Blink's BSD `core/paint/clip_path_clipper.cc`:
//!
//! - the choice between a path-based clip and a mask-based one
//!   (`PathBasedClipInternal`). A clip is path-based when its content
//!   reduces to one path, and a `clip-path` on the `clipPath` itself is
//!   intersected with Skia's path ops while both stay paths and together
//!   have at most 500 verbs (`ClipPathNestedRasterOptimization`, stable in
//!   Chromium 147);
//! - the mask-based clip (`PaintClipPathAsMaskImage`). The first `clipPath`
//!   in the chain draws its content as coverage, each further one is a
//!   `DstIn` layer, and the chain stops at the first `clip-path` that is
//!   path-based, which clips the rest;
//! - the reference box for `url()` on an SVG element: its fill box.
//!
//! Written from SVG 2, CSS Masking and black-box observation of Chromium
//! 147 (Blink's resource clipper and `clipPath` element are LGPL and are
//! not read), checked against Chromium by `tests/paint_corpus.rs`
//! (`tests/fidelity/fixtures/svg-paint-v3`):
//!
//! - **The content transform.** The `clipPath`'s `transform`, then for
//!   `objectBoundingBox` units the map from the unit square to the
//!   reference box.
//! - **When the content is one path.** Every child is a shape with no
//!   `clip-path` of its own. Several children are united in one path-ops
//!   build, each with its own transform and `clip-rule`; one child's path
//!   is used as it is. Strokes, `fill` and `fill-rule` play no part. With
//!   no nested `clip-path`, the content transform is applied by the canvas
//!   rather than to the path.
//! - **The coverage of a mask-based clip.** Each child's fill, opaque, with
//!   its `clip-rule` and its anti-aliasing, drawn as the shape painter
//!   draws it (a rectangle or an oval as such), clipped by its own
//!   `clip-path`.
//! - **The mask's layers.** The content is drawn into a layer, clipped to
//!   the bounds of the `clipPath`'s content, and the mask is composited
//!   onto it with `DstIn`, itself unclipped. Group opacity is applied
//!   outside the clip, to the clipped result.
//! - **Unresolved references.** A nested reference that resolves to
//!   nothing, or closes a cycle, rules out the path-based clip; the mask
//!   chain stops there.
//!
//! Which children contribute is the caller's: `display: none` and
//! `visibility: hidden` children are left out. So are references that do
//! not resolve to a `clipPath`, which leave the element unclipped, and the
//! reference that closes a cycle.

use crate::geometry::FillRule;
use crate::paint::to_sk_matrix;
use crate::shapes::Geometry;
use crate::transform::Matrix;
use crate::Rect;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::{
    BlendMode, Canvas, ClipOp, Matrix as SkMatrix, OpBuilder, Paint, Path, PathFillType, PathOp,
    Rect as SkRect,
};

/// `clipPathUnits`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipPathUnits {
    #[default]
    UserSpaceOnUse,
    ObjectBoundingBox,
}

impl ClipPathUnits {
    /// Parses the attribute; anything else is the initial value.
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "objectBoundingBox" => Self::ObjectBoundingBox,
            _ => Self::UserSpaceOnUse,
        }
    }
}

/// What a child of a `clipPath` draws.
#[derive(Clone, Debug)]
pub enum ClipContent {
    /// A shape's fill geometry in the child's user space.
    Shape {
        path: Path,
        /// How the mask draws it: a rectangle or an oval is drawn as such,
        /// as the shape painter draws it.
        geometry: Geometry,
        clip_rule: FillRule,
        /// Off for `shape-rendering: crispEdges` and `optimizeSpeed`.
        anti_alias: bool,
    },
    /// Content that cannot be drawn yet (text). A `clipPath` with such a
    /// child is not a path, and its mask leaves the child out. A `use` of
    /// a shape is a [`ClipContent::Shape`] under the `use`'s placement
    /// ([`crate::use_element`]).
    Unsupported,
}

/// A `clip-path: url(#id)` on a `clipPath` or on one of its children.
#[derive(Clone, Debug)]
pub enum ClipReference {
    Resolved(Box<ClipPath>),
    /// A reference that names no `clipPath`, or closes a cycle. On a
    /// `clipPath` or its child it still rules out the path-based clip, and
    /// the mask chain stops at it.
    Unresolved,
}

impl ClipReference {
    fn resolved(&self) -> Option<&ClipPath> {
        match self {
            Self::Resolved(clip) => Some(clip),
            Self::Unresolved => None,
        }
    }
}

/// One rendered child of a `clipPath`.
#[derive(Clone, Debug)]
pub struct ClipChild {
    pub content: ClipContent,
    /// The child's `transform`, into the `clipPath`'s content space.
    pub transform: Matrix,
    /// The child's own `clip-path`. Its reference box is the child's fill
    /// box.
    pub clip_path: Option<ClipReference>,
}

/// A resolved `clipPath` element.
#[derive(Clone, Debug, Default)]
pub struct ClipPath {
    pub units: ClipPathUnits,
    /// The `clipPath`'s own `transform`.
    pub transform: Matrix,
    pub children: Vec<ClipChild>,
    /// The `clipPath`'s own `clip-path`.
    pub clip_path: Option<ClipReference>,
}

/// Skia's path ops can take quadratic time, so a nested clip falls back to
/// a mask past this many verbs (Blink: `kMaxVerbs`).
const MAX_NESTED_VERBS: usize = 500;

impl ClipPath {
    /// The map from the content's coordinates to the user space of the
    /// clipped element (Blink: `MaskToContentTransform` for an SVG element).
    pub fn content_transform(&self, reference_box: Rect) -> Matrix {
        match self.units {
            ClipPathUnits::UserSpaceOnUse => self.transform,
            ClipPathUnits::ObjectBoundingBox => self
                .transform
                .then_apply(&Matrix::translate(
                    f64::from(reference_box.x),
                    f64::from(reference_box.y),
                ))
                .then_apply(&Matrix::scale(
                    f64::from(reference_box.width),
                    f64::from(reference_box.height),
                )),
        }
    }

    /// The content as one path in its own coordinates, or `None` when it
    /// needs a mask. Several children are united in one Skia path-ops
    /// build; a single child's path is used as it is.
    fn content_path(&self) -> Option<Path> {
        let mut paths = Vec::with_capacity(self.children.len());
        for child in &self.children {
            if child.clip_path.is_some() {
                return None;
            }
            let ClipContent::Shape {
                path, clip_rule, ..
            } = &child.content
            else {
                return None;
            };
            let mut path = path.clone();
            path.set_fill_type(fill_type(*clip_rule));
            if child.transform != Matrix::IDENTITY {
                path = path.with_transform(&to_sk_matrix(&child.transform));
            }
            paths.push(path);
        }
        match paths.len() {
            0 => Some(Path::new()),
            1 => paths.pop(),
            _ => {
                let mut builder = OpBuilder::default();
                for path in &paths {
                    builder.add(path, PathOp::Union);
                }
                builder.resolve()
            }
        }
    }

    /// The path-based clip as a path and the transform that maps it to the
    /// clipped element's user space, or `None` when the clip must be a
    /// mask. Without a nested `clip-path` the path stays in the content's
    /// coordinates and the transform is applied when clipping, which is
    /// how Chromium's pixels come out; a nested one is intersected in user
    /// space.
    fn path_clip(&self, reference_box: Rect) -> Option<(Path, Matrix)> {
        let path = self.content_path()?;
        let transform = self.content_transform(reference_box);
        let Some(nested) = &self.clip_path else {
            return Some((path, transform));
        };
        let nested = nested.resolved()?.path_based_clip(reference_box)?;
        let path = if transform == Matrix::IDENTITY {
            path
        } else {
            path.with_transform(&to_sk_matrix(&transform))
        };
        if path.count_verbs() + nested.count_verbs() > MAX_NESTED_VERBS {
            return None;
        }
        Some((path.op(&nested, PathOp::Intersect)?, Matrix::IDENTITY))
    }

    /// The clip as a path in the clipped element's user space, or `None`
    /// when it must be applied as a mask (Blink: `PathBasedClipInternal`).
    pub fn path_based_clip(&self, reference_box: Rect) -> Option<Path> {
        let (path, transform) = self.path_clip(reference_box)?;
        Some(if transform == Matrix::IDENTITY {
            path
        } else {
            path.with_transform(&to_sk_matrix(&transform))
        })
    }

    /// Clips `canvas` to the path-based clip. `false`, with the canvas
    /// untouched, when the clip must be a mask.
    fn clip_canvas(&self, canvas: &Canvas, reference_box: Rect) -> bool {
        let Some((path, transform)) = self.path_clip(reference_box) else {
            return false;
        };
        let matrix = canvas.local_to_device();
        canvas.concat(&to_sk_matrix(&transform));
        canvas.clip_path(&path, ClipOp::Intersect, true);
        canvas.set_matrix(&matrix);
        true
    }

    /// The bounds of the clip's content in the clipped element's user space:
    /// the children's transformed bounds, united. `None` when no child draws.
    pub fn bounds(&self, reference_box: Rect) -> Option<SkRect> {
        let transform = to_sk_matrix(&self.content_transform(reference_box));
        let mut out: Option<SkRect> = None;
        for child in &self.children {
            let ClipContent::Shape { path, .. } = &child.content else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            let matrix = SkMatrix::concat(&transform, &to_sk_matrix(&child.transform));
            let bounds = matrix.map_rect(path.bounds()).0;
            out = Some(match out {
                None => bounds,
                Some(mut so_far) => {
                    so_far.join(bounds);
                    so_far
                }
            });
        }
        out
    }

    /// Draws the clip's coverage for a mask-based clip, in the clipped
    /// element's user space, which is the canvas's current one (Blink:
    /// `PaintClipPathAsMaskImage`).
    pub fn paint_mask(&self, canvas: &Canvas, reference_box: Rect) {
        let mut current = Some(self);
        let mut first = true;
        let mut rest_applied = false;
        while let (false, Some(clip)) = (rest_applied, current) {
            if first {
                canvas.save();
            } else {
                let mut paint = Paint::default();
                paint.set_blend_mode(BlendMode::DstIn);
                canvas.save_layer(&SaveLayerRec::default().paint(&paint));
            }
            if let Some(nested) = clip.clip_path.as_ref().and_then(ClipReference::resolved) {
                rest_applied = nested.clip_canvas(canvas, reference_box);
            }
            canvas.concat(&to_sk_matrix(&clip.content_transform(reference_box)));
            clip.paint_content(canvas);
            canvas.restore();
            first = false;
            current = clip.clip_path.as_ref().and_then(ClipReference::resolved);
        }
    }

    /// The content's coverage in its own coordinates: each shape child's
    /// fill, opaque, drawn as the shape painter draws it.
    fn paint_content(&self, canvas: &Canvas) {
        for child in &self.children {
            let ClipContent::Shape {
                path,
                geometry,
                clip_rule,
                anti_alias,
            } = &child.content
            else {
                continue;
            };
            canvas.save();
            canvas.concat(&to_sk_matrix(&child.transform));
            let mut path = path.clone();
            path.set_fill_type(fill_type(*clip_rule));
            let draw = |canvas: &Canvas| {
                let mut paint = Paint::default();
                paint.set_anti_alias(*anti_alias);
                match geometry {
                    Geometry::Rectangle => canvas.draw_rect(path.bounds(), &paint),
                    Geometry::Ellipse => canvas.draw_oval(path.bounds(), &paint),
                    Geometry::Path => canvas.draw_path(&path, &paint),
                };
            };
            match child.clip_path.as_ref().and_then(ClipReference::resolved) {
                Some(clip) => {
                    let reference_box = crate::geometry::bounding_box(&path);
                    clip_content(canvas, clip, reference_box, draw);
                }
                None => draw(canvas),
            }
            canvas.restore();
        }
    }
}

fn fill_type(rule: FillRule) -> PathFillType {
    match rule {
        FillRule::NonZero => PathFillType::Winding,
        FillRule::EvenOdd => PathFillType::EvenOdd,
    }
}

/// Paints `content` clipped by `clip`, in the clipped element's user space,
/// which is the canvas's current one. `reference_box` is the element's
/// reference box (its fill box).
///
/// A path-based clip clips the canvas, anti-aliased. Otherwise the content
/// is drawn into a layer, clipped to the clip's bounds, and the mask is
/// composited onto it with `DstIn`. The bounds clip only the content: the
/// mask is drawn unclipped, as Chromium's pixels show.
pub fn clip_content(
    canvas: &Canvas,
    clip: &ClipPath,
    reference_box: Rect,
    content: impl FnOnce(&Canvas),
) {
    canvas.save();
    if clip.clip_canvas(canvas, reference_box) {
        content(canvas);
    } else {
        let matrix = canvas.local_to_device();
        canvas.save_layer(&SaveLayerRec::default());
        canvas.save();
        if let Some(bounds) = clip.bounds(reference_box) {
            canvas.clip_rect(bounds, ClipOp::Intersect, false);
        }
        content(canvas);
        canvas.restore();
        canvas.set_matrix(&matrix);
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::DstIn);
        canvas.save_layer(&SaveLayerRec::default().paint(&paint));
        clip.paint_mask(canvas, reference_box);
        canvas.restore();
        canvas.restore();
    }
    canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_child(x: f32, y: f32, w: f32, h: f32) -> ClipChild {
        ClipChild {
            content: ClipContent::Shape {
                path: Path::rect(SkRect::from_xywh(x, y, w, h), None),
                geometry: Geometry::Rectangle,
                clip_rule: FillRule::NonZero,
                anti_alias: true,
            },
            transform: Matrix::IDENTITY,
            clip_path: None,
        }
    }

    #[test]
    fn object_bounding_box_maps_the_unit_square() {
        let clip = ClipPath {
            units: ClipPathUnits::ObjectBoundingBox,
            children: vec![rect_child(0.0, 0.0, 0.5, 1.0)],
            ..ClipPath::default()
        };
        let path = clip
            .path_based_clip(Rect::new(10.0, 20.0, 40.0, 30.0))
            .unwrap();
        let bounds = path.bounds();
        assert_eq!(
            (bounds.left, bounds.top, bounds.right, bounds.bottom),
            (10.0, 20.0, 30.0, 50.0)
        );
    }

    #[test]
    fn a_child_with_its_own_clip_needs_a_mask() {
        let mut child = rect_child(0.0, 0.0, 10.0, 10.0);
        child.clip_path = Some(ClipReference::Resolved(Box::default()));
        let clip = ClipPath {
            children: vec![child],
            ..ClipPath::default()
        };
        assert!(clip.path_based_clip(Rect::default()).is_none());
    }

    #[test]
    fn an_unresolved_nested_reference_needs_a_mask() {
        let clip = ClipPath {
            children: vec![rect_child(0.0, 0.0, 10.0, 10.0)],
            clip_path: Some(ClipReference::Unresolved),
            ..ClipPath::default()
        };
        assert!(clip.path_based_clip(Rect::default()).is_none());
    }

    #[test]
    fn a_nested_clip_intersects() {
        let nested = ClipPath {
            children: vec![rect_child(5.0, 0.0, 20.0, 20.0)],
            ..ClipPath::default()
        };
        let clip = ClipPath {
            children: vec![rect_child(0.0, 0.0, 10.0, 10.0)],
            clip_path: Some(ClipReference::Resolved(Box::new(nested))),
            ..ClipPath::default()
        };
        let path = clip.path_based_clip(Rect::default()).unwrap();
        let bounds = path.bounds();
        assert_eq!(
            (bounds.left, bounds.top, bounds.right, bounds.bottom),
            (5.0, 0.0, 10.0, 10.0)
        );
    }

    #[test]
    fn an_empty_clip_path_clips_everything() {
        let path = ClipPath::default()
            .path_based_clip(Rect::default())
            .unwrap();
        assert!(path.is_empty());
    }
}
