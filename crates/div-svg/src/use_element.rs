//! `use` and `symbol` (RFC 0009 V3d, first part): where a `use` places its
//! instance, and the viewport it gives a `symbol` or an `svg`.
//!
//! Blink's `use` element and its layout container are LGPL and are not
//! read. This is written from SVG 2 (§5.6, §5.5) and black-box observation
//! of Chromium 147, checked by `tests/paint_corpus.rs`
//! (`tests/fidelity/fixtures/svg-paint-v6`). Building the instance tree is
//! the caller's (it is DOM work); the corpus pins down these rules for it:
//!
//! - **The instance.** A deep copy of the element `href` names (`href` wins
//!   over `xlink:href`), as the `use` element's only child. It inherits from
//!   the `use`, not from where the original sits; the original's own
//!   properties, `transform`, `display` and `visibility` come with it.
//! - **Placement.** The `use`'s `transform`, then `translate(x, y)`
//!   ([`UseAttributes::translation`]). Both are the `use`'s own transform,
//!   so a `clip-path` or `mask` on it is in the translated user space, and
//!   its reference box is the instance's.
//! - **`symbol` and `svg`.** The instance is a nested viewport: the
//!   `use`'s `width` and `height`, where set, replace the target's, and
//!   both are initially 100% ([`UseAttributes::instance_viewport`]). It
//!   clips to the viewport unless `overflow` is visible, and an empty
//!   `viewBox` disables it. A `symbol` outside a `use` renders nothing.
//! - **Other targets.** `width` and `height` play no part; a target that
//!   does not render (a gradient, say) renders nothing.
//! - **Cycles.** A `use` whose target is the `use` itself or one of its
//!   ancestors renders nothing; inside an instance, an ancestor counts as
//!   the element it is a copy of. Only that `use` is dropped: what the
//!   outer instances hold still renders.
//! - **Inside a `clipPath`.** A `use` contributes its target's geometry
//!   when the target is a shape, under the `use`'s placement and the
//!   target's own `transform`, with `clip-rule` inherited from the `use`.
//!   A `use` of anything else (a `g`, a `symbol`) contributes nothing.
//!
//! Not covered: `context-fill` and `context-stroke`, references into other
//! documents, and `use` of `text`.

use crate::length::{Axis, Length, LengthContext};
use crate::transform::Matrix;
use crate::viewport::{NestedViewport, ViewportAttributes};

/// A `use` element's geometry attributes as set on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UseAttributes {
    pub x: Option<Length>,
    pub y: Option<Length>,
    pub width: Option<Length>,
    pub height: Option<Length>,
}

impl UseAttributes {
    /// `translate(x, y)`, applied after the `use`'s `transform`: its full
    /// transform is `transform · translation`.
    pub fn translation(&self, context: &LengthContext) -> Matrix {
        let len = |length: Option<Length>, axis| length.map_or(0.0, |l| l.resolve(axis, context));
        Matrix::translate(
            f64::from(len(self.x, Axis::Horizontal)),
            f64::from(len(self.y, Axis::Vertical)),
        )
    }

    /// The viewport of the instance of a `symbol` or `svg` target with
    /// attributes `target`, in the `use`'s translated user space; `None`
    /// when it is disabled.
    pub fn instance_viewport(
        &self,
        target: &ViewportAttributes,
        context: &LengthContext,
    ) -> Option<NestedViewport> {
        ViewportAttributes {
            width: self.width.or(target.width),
            height: self.height.or(target.height),
            ..*target
        }
        .resolve(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::parse_view_box;
    use crate::Rect;

    fn context() -> LengthContext {
        LengthContext {
            viewport_width: 200.0,
            viewport_height: 100.0,
            font_size: 16.0,
            root_font_size: 16.0,
            x_height: 8.0,
        }
    }

    #[test]
    fn the_translation_resolves_percentages() {
        let attributes = UseAttributes {
            x: Length::parse("10%"),
            y: Length::parse("4"),
            ..UseAttributes::default()
        };
        assert_eq!(
            attributes.translation(&context()),
            Matrix::translate(20.0, 4.0)
        );
    }

    #[test]
    fn the_use_size_replaces_the_target_size() {
        let symbol = ViewportAttributes {
            width: Length::parse("30"),
            height: Length::parse("20"),
            view_box: parse_view_box("0 0 20 10"),
            ..ViewportAttributes::default()
        };
        let own = UseAttributes::default()
            .instance_viewport(&symbol, &context())
            .unwrap();
        assert_eq!(own.viewport, Rect::new(0.0, 0.0, 30.0, 20.0));
        let set = UseAttributes {
            width: Length::parse("50%"),
            ..UseAttributes::default()
        };
        let viewport = set.instance_viewport(&symbol, &context()).unwrap();
        assert_eq!(viewport.viewport, Rect::new(0.0, 0.0, 100.0, 20.0));
        let bare = UseAttributes::default()
            .instance_viewport(&ViewportAttributes::default(), &context())
            .unwrap();
        assert_eq!(bare.viewport, Rect::new(0.0, 0.0, 200.0, 100.0));
    }
}
