//! Tabs: shadcn/ui `tabs.tsx`, the default variant, over the tabs primitive
//! ([`TabsState`]).
//!
//! `Tabs`: `flex gap-2`, plus `flex-col` when horizontal. `TabsList`:
//! `inline-flex w-fit items-center justify-center rounded-lg p-[3px]
//! text-muted-foreground bg-muted`, `h-9` when horizontal and `h-fit
//! flex-col` when vertical. `TabsTrigger`: `relative inline-flex
//! h-[calc(100%-1px)] flex-1 items-center justify-center gap-1.5 rounded-md
//! border border-transparent px-2 py-1 text-sm font-medium whitespace-nowrap
//! text-foreground/60`, `w-full justify-start` when vertical,
//! `disabled:pointer-events-none disabled:opacity-50`, and when active
//! `shadow-sm bg-background text-foreground` (dark: `border-input
//! bg-input/30`; inactive dark text is `text-muted-foreground`).
//! `TabsContent`: `flex-1 outline-none`.
//!
//! Div has no context API or attribute selectors yet (RFC 0003), so every
//! part takes the root's [`TabsState`] as a prop, and the active styles are
//! set inline by an effect on the selection. The `line` variant is not
//! built: its active indicator is an `::after` box, which Div cannot draw.
//! Hover, focus-visible and transition styles are interactive and not
//! built yet.

use div::prelude::*;
use div::typed_style::{Length, RendererStyleValue, StyleProperty};

use crate::css::{
    black_shadow, set, set_border, set_border_color, set_box_shadow, set_color, set_typed, styled,
    with_opacity,
};
use crate::primitives::TabsState;
use crate::{current_theme, Orientation};

/// `flex-1`: `flex: 1`, that is `1 1 0%`, written as longhands (Div has
/// no `flex` shorthand) with an `auto` basis.
///
/// Interim: Div sizes a `0%` basis as zero when the flex container's main
/// size is indefinite, where CSS treats it as `content`, and so lays out a
/// `w-fit` list of zero-basis triggers 6px wide. Here an `auto` basis gives
/// Chromium's sizes: the triggers' text does not wrap, so each ends at its
/// content size either way, and the panel's and the vertical list's main
/// sizes are indefinite, where `0%` is `content`. It goes back to `0%`
/// once the engine resolves it as CSS does.
const FLEX_1: [(&str, &str); 3] = [
    ("flex-grow", "1"),
    ("flex-shrink", "1"),
    ("flex-basis", "auto"),
];

fn mount(parent: &Element, children: Vec<ViewNode>) {
    for child in children {
        mount_view(parent, child);
    }
}

/// The tabs root. `state` is the tabs' state, which every part shares.
#[allow(non_snake_case)]
#[component]
pub fn Tabs(state: TabsState, children: Vec<ViewNode>) -> ViewNode {
    let root = state.root();
    styled(&root, "tabs", &[("display", "flex"), ("gap", "8px")]);
    if state.orientation() == Orientation::Horizontal {
        set(&root, &[("flex-direction", "column")]);
    }
    mount(&root, children);
    ViewNode::Element(root)
}

/// The list of triggers, in the default variant.
#[allow(non_snake_case)]
#[component]
pub fn TabsList(state: TabsState, children: Vec<ViewNode>) -> ViewNode {
    let theme = current_theme();
    let list = state.list();
    // `rounded-lg`: var(--radius).
    let radius = format!("{}px", theme.radius);
    styled(
        &list,
        "tabs-list",
        &[
            ("display", "inline-flex"),
            ("align-items", "center"),
            ("justify-content", "center"),
            ("border-radius", &radius),
            ("padding", "3px"),
        ],
    );
    list.set_attribute("data-variant", "default")
        .expect("set data-variant");
    // w-fit and, when vertical, h-fit. Interim: Div's it-content
    // length is it-content(0px), which sizes the list to its padding.
    // The list is a flex item of the root, so the size it fits to is its
    // cross size there, and lign-self: flex-start gives the same box:
    // it-content on the cross axis, unstretched, at the start.
    set(&list, &[("align-self", "flex-start")]);
    set_color(&list, "color", theme.muted_foreground);
    set_color(&list, "background-color", theme.muted);
    match state.orientation() {
        Orientation::Horizontal => set(&list, &[("height", "36px")]),
        Orientation::Vertical => set(&list, &[("flex-direction", "column")]),
    }
    mount(&list, children);
    ViewNode::Element(list)
}

/// The trigger of the tab `value`, holding `children`.
#[allow(non_snake_case)]
#[component]
pub fn TabsTrigger(
    state: TabsState,
    value: &'static str,
    disabled: bool,
    children: Vec<ViewNode>,
) -> ViewNode {
    let theme = current_theme();
    let trigger = state.trigger(value, disabled);
    // `rounded-md`: calc(var(--radius) * 0.8).
    let radius = format!("{}px", theme.radius * 0.8);
    styled(
        &trigger,
        "tabs-trigger",
        &[
            // Preflight for buttons: `background-color: transparent`.
            ("background-color", "transparent"),
            ("position", "relative"),
            ("display", "inline-flex"),
            // `flex-1` (`flex: 1`); see [`FLEX_1`].
            FLEX_1[0],
            FLEX_1[1],
            FLEX_1[2],
            ("align-items", "center"),
            ("justify-content", "center"),
            ("gap", "6px"),
            ("border-radius", &radius),
            ("padding", "4px 8px"),
            // `text-sm`: 0.875rem with line-height calc(1.25 / 0.875).
            ("font-size", "14px"),
            ("line-height", "20px"),
            ("font-weight", "500"),
            ("white-space", "nowrap"),
        ],
    );
    trigger
        .set_native_appearance(false)
        .expect("authored button appearance");
    set_border(&trigger, "1");
    match state.orientation() {
        // h-[calc(100%-1px)], as a typed value: Div's height literal
        // accepts no calc().
        Orientation::Horizontal => set_typed(
            &trigger,
            StyleProperty::Height,
            RendererStyleValue::Height(Length::calc_percent_px(100.0, -1.0)),
        ),
        // The vertical list's height is its content's, so the percentage
        // has nothing to resolve against and CSS treats the height as
        // uto. Interim: Div resolves it against zero, so it is left
        // uto here.
        Orientation::Vertical => set(
            &trigger,
            &[("width", "100%"), ("justify-content", "flex-start")],
        ),
    }
    if disabled {
        set(&trigger, &[("pointer-events", "none"), ("opacity", "0.5")]);
    }

    let weak = trigger.downgrade();
    create_effect(move || {
        let active = state.is_selected(value);
        let Some(trigger) = weak.upgrade() else {
            return;
        };
        if active {
            // `shadow-sm`: 0 1px 3px 0 rgb(0 0 0 / 0.1),
            // 0 1px 2px -1px rgb(0 0 0 / 0.1)
            set_box_shadow(
                &trigger,
                vec![
                    black_shadow(0.0, 1.0, 3.0, 0.0, 0.1),
                    black_shadow(0.0, 1.0, 2.0, -1.0, 0.1),
                ],
            );
            set_color(&trigger, "color", theme.foreground);
            if theme.dark {
                set_border_color(&trigger, theme.input);
                set_color(&trigger, "background-color", with_opacity(theme.input, 30));
            } else {
                set_border_color(&trigger, "transparent");
                set_color(&trigger, "background-color", theme.background);
            }
        } else {
            trigger
                .remove_property(StyleProperty::BoxShadow)
                .expect("remove the shadow");
            if theme.dark {
                set_color(&trigger, "color", theme.muted_foreground);
            } else {
                set_color(&trigger, "color", with_opacity(theme.foreground, 60));
            }
            set_border_color(&trigger, "transparent");
            set(&trigger, &[("background-color", "transparent")]);
        }
    });

    mount(&trigger, children);
    ViewNode::Element(trigger)
}

/// The panel of the tab `value`, holding `children` while it is selected.
#[allow(non_snake_case)]
#[component]
pub fn TabsContent(state: TabsState, value: &'static str, children: Vec<ViewNode>) -> ViewNode {
    let content = state.content(value, children);
    styled(
        &content,
        "tabs-content",
        &[
            // `flex-1` (`flex: 1`); see [`FLEX_1`].
            FLEX_1[0],
            FLEX_1[1],
            FLEX_1[2],
            ("outline-style", "none"),
        ],
    );
    ViewNode::Element(content)
}
