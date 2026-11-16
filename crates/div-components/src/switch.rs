//! Switch: shadcn/ui `switch.tsx` over the switch primitive
//! ([`SwitchState`]).
//!
//! Root: `inline-flex shrink-0 items-center rounded-full border
//! border-transparent shadow-xs outline-none`, `h-[1.15rem] w-8` at the
//! default size and `h-3.5 w-6` at `sm`, `bg-primary` when checked and
//! `bg-input` when not (dark: `bg-input/80`), and when disabled
//! `cursor-not-allowed opacity-50`. Thumb: `pointer-events-none block
//! rounded-full bg-background ring-0`, `size-4` (`size-3` at `sm`), moved
//! `translate-x-[calc(100%-2px)]` when checked (dark: `bg-primary-foreground`
//! when checked, `bg-foreground` when not).
//!
//! Div has no attribute selectors in the cascade yet (RFC 0003, request 6),
//! so the `data-state` styles are set inline by an effect on the state.
//! `focus-visible` (a 3px `ring` and the `ring` border color) and the
//! `transition-all`/`transition-transform` animations are interactive and
//! not built yet; the pixel fixtures compare the settled states. `ring-0`
//! is a zero-width shadow that draws nothing and is left out.

use div::prelude::*;

use crate::css::{
    black_shadow, set, set_border, set_border_color, set_box_shadow, set_color, styled,
    with_opacity,
};
use crate::current_theme;
use crate::primitives::SwitchState;

/// `size` of shadcn's switch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwitchSize {
    /// `h-[1.15rem] w-8`, with a 16px thumb.
    #[default]
    Default,
    /// `h-3.5 w-6`, with a 12px thumb.
    Sm,
}

impl SwitchSize {
    /// The size's name in shadcn's `data-size` attribute.
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
        }
    }

    /// The thumb's width and height in CSS pixels (`size-4` or `size-3`).
    fn thumb(self) -> f32 {
        match self {
            Self::Default => 16.0,
            Self::Sm => 12.0,
        }
    }
}

/// A shadcn/ui switch. `state` holds whether it is on; see
/// [`SwitchState`].
#[allow(non_snake_case)]
#[component]
pub fn Switch(state: SwitchState, size: SwitchSize) -> ViewNode {
    let theme = current_theme();
    let root = state.root();
    let (height, width) = match size {
        // `h-[1.15rem]`: 1.15 × 16px.
        SwitchSize::Default => ("18.4px", "32px"),
        SwitchSize::Sm => ("14px", "24px"),
    };
    styled(
        &root,
        "switch",
        &[
            // Preflight for buttons: `background-color: transparent`,
            // replaced below by the state's color.
            ("background-color", "transparent"),
            ("display", "inline-flex"),
            ("flex-shrink", "0"),
            ("align-items", "center"),
            // `rounded-full` is calc(infinity * 1px); any radius of at least
            // half the height draws the same.
            ("border-radius", "9999px"),
            ("outline-style", "none"),
            ("height", height),
            ("width", width),
        ],
    );
    root.set_native_appearance(false)
        .expect("authored button appearance");
    root.set_attribute("data-size", size.name())
        .expect("set data-size");
    // `border border-transparent`
    set_border(&root, "1");
    set_border_color(&root, "transparent");
    // `shadow-xs`: 0 1px 2px 0 rgb(0 0 0 / 0.05)
    set_box_shadow(&root, vec![black_shadow(0.0, 1.0, 2.0, 0.0, 0.05)]);
    if state.disabled() {
        set(&root, &[("cursor", "not-allowed"), ("opacity", "0.5")]);
    }

    let thumb = state.thumb();
    let side = format!("{}px", size.thumb());
    styled(
        &thumb,
        "switch-thumb",
        &[
            ("pointer-events", "none"),
            ("display", "block"),
            ("border-radius", "9999px"),
            ("width", &side),
            ("height", &side),
        ],
    );
    root.append_child(&thumb).expect("attach the thumb");

    let weak_root = root.downgrade();
    let weak_thumb = thumb.downgrade();
    create_effect(move || {
        let checked = state.is_checked();
        let (Some(root), Some(thumb)) = (weak_root.upgrade(), weak_thumb.upgrade()) else {
            return;
        };
        let track = match (checked, theme.dark) {
            (true, _) => theme.primary.css().to_owned(),
            (false, false) => theme.input.css().to_owned(),
            (false, true) => with_opacity(theme.input, 80),
        };
        set_color(&root, "background-color", track);
        let knob = match (checked, theme.dark) {
            (_, false) => theme.background,
            (true, true) => theme.primary_foreground,
            (false, true) => theme.foreground,
        };
        set_color(&thumb, "background-color", knob);
        // `translate-x-[calc(100%-2px)]`: the thumb's width less 2px. Div
        // has no `translate` property; with no other transform on the
        // thumb, `transform` places it the same.
        let offset = if checked { size.thumb() - 2.0 } else { 0.0 };
        set(
            &thumb,
            &[("transform", &format!("translateX({offset}px)") as &str)],
        );
    });

    ViewNode::Element(root)
}
