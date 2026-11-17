//! Input: shadcn/ui `input.tsx` (an `<input>` with `data-slot="input"`).
//!
//! Classes: `h-9 w-full min-w-0 rounded-md border border-input
//! bg-transparent px-3 py-1 text-base shadow-xs outline-none
//! placeholder:text-muted-foreground disabled:pointer-events-none
//! disabled:cursor-not-allowed disabled:opacity-50 md:text-sm
//! dark:bg-input/30`, `focus-visible:border-ring focus-visible:ring-[3px]
//! focus-visible:ring-ring/50`, and `aria-invalid:border-destructive
//! aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40`,
//! over Tailwind's preflight for inputs (`font: inherit; color: inherit;
//! border-radius: 0; background-color: transparent`). Div's literals have
//! no `inherit`; the text color is set from the theme, which is what
//! `inherit` gives on a page whose text is `--foreground`.
//!
//! Interims, until the stylesheet cascade reaches components (RFC 0003,
//! request 6):
//!
//! - `md:text-sm` is a media query. Div has no media queries for inline
//!   styles, so the input always has the 14px text of viewports 768px and
//!   wider; below that Chromium gives `text-base` (16px).
//! - `:focus-visible` is set inline from `focus` and `blur` events. A text
//!   field matches `:focus-visible` whenever it has focus, however it got
//!   it, so the two agree for an input.
//! - `aria-invalid` is a prop, as Div restyles nothing on an attribute.
//!
//! Not built: `transition-[color,box-shadow]`; the `selection:` colors
//! (Div has no `::selection` style); the `file:` styles of a file input's
//! button.

use div::prelude::*;
use div::typed_style::{
    BoxShadow, Declaration, PseudoStyleTarget, Style, StyleProperty, StyleValue,
};

use crate::css::{
    black_shadow, element, set, set_border, set_border_color, set_box_shadow, set_color,
    with_opacity,
};
use crate::{current_theme, Token};

/// A shadcn/ui input of `input_type` (`"text"`, `"email"`, `"password"`
/// and the other text types). `default_value` is its initial value (the
/// `value` attribute); `placeholder` is shown while it is empty. `invalid`
/// sets `aria-invalid="true"`, which shadcn styles with the destructive
/// color.
///
/// Every prop is required in `view!`, which does not fill omitted optional
/// props yet (RFC 0003, request 3).
#[allow(non_snake_case)]
#[component]
pub fn Input(
    input_type: &'static str,
    placeholder: Option<&'static str>,
    default_value: Option<&'static str>,
    disabled: bool,
    invalid: bool,
) -> ViewNode {
    let theme = current_theme();
    // `rounded-md`: calc(var(--radius) * 0.8).
    let radius = format!("{}px", theme.radius * 0.8);
    let input = element(
        "input",
        "input",
        &[
            ("height", "36px"),
            ("width", "100%"),
            ("min-width", "0px"),
            ("border-radius", &radius),
            ("padding", "4px 12px"),
            // `md:text-sm`: 0.875rem with line-height calc(1.25 / 0.875),
            // 20px at this size (see the module documentation).
            ("font-size", "14px"),
            ("line-height", "20px"),
            ("outline-style", "none"),
        ],
    );
    // Chromium drops a text field's native look once the author styles its
    // background or border; Div is told explicitly.
    input
        .set_native_appearance(false)
        .expect("authored input appearance");
    for (name, value) in [
        ("type", Some(input_type)),
        ("placeholder", placeholder),
        ("value", default_value),
        ("aria-invalid", invalid.then_some("true")),
        ("disabled", disabled.then_some("")),
    ] {
        if let Some(value) = value {
            input
                .set_attribute(name, value)
                .unwrap_or_else(|error| panic!("set {name}: {error}"));
        }
    }
    set_color(&input, "color", theme.foreground);
    // `bg-transparent`, and `dark:bg-input/30`.
    let background = if theme.dark {
        with_opacity(theme.input, 30)
    } else {
        "transparent".to_owned()
    };
    set_color(&input, "background-color", background);
    set_border(&input, "1");
    if disabled {
        set(
            &input,
            &[
                ("pointer-events", "none"),
                ("cursor", "not-allowed"),
                ("opacity", "0.5"),
            ],
        );
    }

    // `placeholder:text-muted-foreground`
    let placeholder_style: Style = [Declaration {
        property: StyleProperty::Color,
        value: StyleValue::Color(theme.muted_foreground.color()),
    }]
    .into_iter()
    .collect();
    input
        .set_pseudo_style(PseudoStyleTarget::Placeholder, &placeholder_style)
        .expect("set the placeholder style");

    // `border-input`, `aria-invalid:border-destructive` and, while focused,
    // `focus-visible:border-ring`; the invalid border wins, as its rule
    // comes later in Tailwind's variant order. The same holds for the ring
    // color. `shadow-xs` is always under the ring.
    let shadow = black_shadow(0.0, 1.0, 2.0, 0.0, 0.05);
    let focused = create_signal(false);
    for (event, value) in [("focus", true), ("blur", false)] {
        input
            .on(event, move |_| focused.set(value))
            .unwrap_or_else(|error| panic!("listen for {event}: {error}"));
    }
    let weak = input.downgrade();
    create_effect(move || {
        let focused = focused.get();
        let Some(input) = weak.upgrade() else {
            return;
        };
        let border = match (invalid, focused) {
            (true, _) => theme.destructive,
            (false, true) => theme.ring,
            (false, false) => theme.input,
        };
        set_border_color(&input, border.css());
        let mut layers = Vec::new();
        if focused {
            // `ring-[3px]`: 0 0 0 3px in the ring color.
            let ring = match (invalid, theme.dark) {
                (true, false) => translucent(theme.destructive, 0.2),
                (true, true) => translucent(theme.destructive, 0.4),
                (false, _) => translucent(theme.ring, 0.5),
            };
            layers.push(BoxShadow {
                offset_x: 0.0,
                offset_y: 0.0,
                blur_radius: 0.0,
                spread_radius: 3.0,
                color: ring,
                inset: false,
            });
        }
        layers.push(shadow.clone());
        set_box_shadow(&input, layers);
    });

    ViewNode::Element(input)
}

/// Tailwind's opacity modifier as a color: `color-mix(in oklab, <token>
/// <fraction>, transparent)` keeps the token's color and scales its alpha.
/// `box-shadow` takes a typed color, so the mix is computed here.
fn translucent(token: Token, fraction: f32) -> Color {
    let color = token.color();
    Color {
        a: color.a * fraction,
        ..color
    }
}
