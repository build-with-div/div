//! Checkbox: shadcn/ui `checkbox.tsx` over the checkbox primitive
//! ([`CheckboxState`]).
//!
//! Root: `peer size-4 shrink-0 rounded-[4px] border border-input shadow-xs
//! outline-none`, `dark:bg-input/30`; when checked `border-primary
//! bg-primary text-primary-foreground`; when disabled `cursor-not-allowed
//! opacity-50`; with `aria-invalid` (the `invalid` prop) `border-destructive`.
//! Indicator: `grid place-content-center text-current`, holding Lucide's
//! `CheckIcon` at `size-3.5`.
//!
//! Div has no attribute selectors in the cascade yet (RFC 0003, request 6),
//! so the `data-state` styles are set inline by an effect on the state.
//! `focus-visible` (a 3px `ring`, or the destructive ring when invalid, and
//! the `ring` border color) and `transition-shadow` are interactive and not
//! built yet; the pixel fixtures compare the settled states.
//!
//! Lucide's check is an inline `<svg>` with one stroked `<path>`. Div
//! creates the outer `svg` element, a replaced box, but no SVG shapes yet
//! (RFC 0009, V1 stage 1), so the icon's box is laid out and its stroke is
//! not drawn.

use div::prelude::*;

use crate::css::{
    black_shadow, set, set_border, set_border_color, set_box_shadow, set_color, styled,
    with_opacity,
};
use crate::current_theme;
use crate::primitives::{CheckboxState, CheckedState};

/// A shadcn/ui checkbox. `state` holds whether it is checked; see
/// [`CheckboxState`]. `invalid` sets `aria-invalid="true"`.
#[allow(non_snake_case)]
#[component]
pub fn Checkbox(state: CheckboxState, invalid: bool) -> ViewNode {
    let theme = current_theme();
    let root = state.root();
    styled(
        &root,
        "checkbox",
        &[
            // Preflight for buttons: `background-color: transparent`,
            // replaced below by the state's color.
            ("background-color", "transparent"),
            ("width", "16px"),
            ("height", "16px"),
            ("flex-shrink", "0"),
            ("border-radius", "4px"),
            ("outline-style", "none"),
        ],
    );
    root.set_native_appearance(false)
        .expect("authored button appearance");
    if invalid {
        root.set_attribute("aria-invalid", "true")
            .expect("set aria-invalid");
    }
    // `border`
    set_border(&root, "1");
    // `shadow-xs`: 0 1px 2px 0 rgb(0 0 0 / 0.05)
    set_box_shadow(&root, vec![black_shadow(0.0, 1.0, 2.0, 0.0, 0.05)]);
    if state.disabled() {
        set(&root, &[("cursor", "not-allowed"), ("opacity", "0.5")]);
    }

    let weak_root = root.downgrade();
    let effect_state = state.clone();
    create_effect(move || {
        let checked = effect_state.checked() == CheckedState::Checked;
        let Some(root) = weak_root.upgrade() else {
            return;
        };
        // Tailwind puts `data-*` variants after `aria-*` ones, so a checked
        // checkbox keeps `border-primary` when it is also invalid.
        let border = match (checked, invalid) {
            (true, _) => theme.primary,
            (false, true) => theme.destructive,
            (false, false) => theme.input,
        };
        set_border_color(&root, border.css());
        let background = match (checked, theme.dark) {
            (true, _) => theme.primary.css().to_owned(),
            (false, false) => "transparent".to_owned(),
            (false, true) => with_opacity(theme.input, 30),
        };
        set_color(&root, "background-color", background);
        // Preflight's `color: inherit`, or `text-primary-foreground` when
        // checked; the icon strokes in `currentColor`.
        let text = if checked {
            theme.primary_foreground
        } else {
            theme.foreground
        };
        set_color(&root, "color", text);
    });

    state.indicator(&root, |indicator| {
        styled(
            indicator,
            "checkbox-indicator",
            &[
                ("display", "grid"),
                ("align-content", "center"),
                ("justify-content", "center"),
            ],
        );
        indicator
            .append_child(&check_icon())
            .expect("attach the check icon");
    });

    ViewNode::Element(root)
}

/// Lucide's `CheckIcon` (`lucide-react`), `size-3.5`: an `svg` 24 units
/// square, stroked in `currentColor` at width 2 with round caps and joins,
/// holding the path `M20 6 9 17l-5-5`. The path is not built: Div's DOM
/// has no SVG shapes yet (RFC 0009).
fn check_icon() -> Element {
    let svg = Element::create(&current_document(), "svg").expect("create an svg");
    for (name, value) in [
        ("xmlns", "http://www.w3.org/2000/svg"),
        ("width", "24"),
        ("height", "24"),
        ("viewBox", "0 0 24 24"),
        ("fill", "none"),
        ("stroke", "currentColor"),
        ("stroke-width", "2"),
        ("stroke-linecap", "round"),
        ("stroke-linejoin", "round"),
        ("class", "lucide lucide-check size-3.5"),
        ("aria-hidden", "true"),
    ] {
        svg.set_attribute(name, value)
            .unwrap_or_else(|error| panic!("set {name}: {error}"));
    }
    // Preflight: `box-sizing: border-box; margin: 0; padding: 0; border: 0
    // solid` and, for `svg`, `display: block; vertical-align: middle`;
    // `size-3.5` is 14px.
    set(
        &svg,
        &[
            ("box-sizing", "border-box"),
            ("margin", "0"),
            ("padding", "0"),
            ("display", "block"),
            ("vertical-align", "middle"),
            ("width", "14px"),
            ("height", "14px"),
        ],
    );
    set_border(&svg, "0");
    svg
}
