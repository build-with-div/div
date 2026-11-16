//! Switch: a port of Radix's `react-switch` 1.4.1 (`Switch`, `SwitchThumb`);
//! its source is the specification.
//!
//! The root is a `button` with `role="switch"`, so the browser's button
//! behavior toggles it: a click, Space (on release) and Enter, which Div's
//! buttons also do. The parts set no visual style beyond what Radix sets;
//! `crate::switch` styles them as shadcn/ui does.
//!
//! Div has no context API yet (RFC 0003, request 1), so the root's state is a
//! [`SwitchState`] handle that both parts are built from. Radix's hidden form
//! input (`SwitchBubbleInput`), which it renders after the button inside a
//! `<form>`, is out of scope until forms (RFC 0003).

use std::rc::Rc;

use div::prelude::{create_effect, current_document, Element, Signal};

use super::controllable::{Controllable, OnChange};
use super::roving_focus::{listen, set_attribute};

/// How a [`SwitchState`] starts. Radix's `Switch` props.
#[derive(Clone)]
pub struct SwitchOptions {
    /// Whether the switch is on, held by the caller (controlled), or `None`.
    pub checked: Option<Signal<bool>>,
    /// Whether the switch starts on when uncontrolled.
    pub default_checked: bool,
    /// Called with the new state when the user toggles the switch.
    pub on_checked_change: Option<OnChange<bool>>,
    pub disabled: bool,
    /// Sets `aria-required="true"`; Radix leaves the attribute out unless
    /// `required` is given.
    pub required: bool,
    /// The button's `value` (`"on"` by default).
    pub value: String,
}

impl Default for SwitchOptions {
    fn default() -> Self {
        Self {
            checked: None,
            default_checked: false,
            on_checked_change: None,
            disabled: false,
            required: false,
            value: "on".to_owned(),
        }
    }
}

struct Switch {
    checked: Controllable<bool>,
    disabled: bool,
    required: bool,
    value: String,
}

/// The state of one switch, shared by its parts. Clones share it.
#[derive(Clone)]
pub struct SwitchState {
    switch: Rc<Switch>,
}

impl SwitchState {
    /// A new switch in the current document.
    pub fn new(options: SwitchOptions) -> Self {
        let checked = match options.checked {
            Some(signal) => Controllable::controlled(signal, options.on_checked_change),
            // Radix: `defaultProp: defaultChecked ?? false`.
            None => Controllable::uncontrolled(options.default_checked, options.on_checked_change),
        };
        Self {
            switch: Rc::new(Switch {
                checked,
                disabled: options.disabled,
                required: options.required,
                value: options.value,
            }),
        }
    }

    /// Whether the switch is on. Inside an effect, the effect reruns when it
    /// changes.
    pub fn is_checked(&self) -> bool {
        self.switch.checked.get()
    }

    /// Turn the switch over, as a click does.
    pub fn toggle(&self) {
        self.switch.checked.set(!self.switch.checked.get());
    }

    pub fn disabled(&self) -> bool {
        self.switch.disabled
    }

    /// Radix's `Switch` root (its `SwitchTrigger`): a `button` with
    /// `role="switch"` and `aria-checked` that a click toggles.
    pub fn root(&self) -> Element {
        let root = create("button");
        set_attribute(&root, "type", "button");
        set_attribute(&root, "role", "switch");
        if self.switch.required {
            set_attribute(&root, "aria-required", "true");
        }
        if self.switch.disabled {
            set_attribute(&root, "data-disabled", "");
            set_attribute(&root, "disabled", "");
        }
        set_attribute(&root, "value", &self.switch.value);

        let state = self.clone();
        let weak = root.downgrade();
        create_effect(move || {
            let checked = state.is_checked();
            if let Some(root) = weak.upgrade() {
                set_attribute(
                    &root,
                    "aria-checked",
                    if checked { "true" } else { "false" },
                );
                set_attribute(&root, "data-state", state_name(checked));
            }
        });

        // A disabled button receives no click in the browser; Div's does, so
        // the handler checks.
        let state = self.clone();
        listen(&root, "click", move |_| {
            if !state.switch.disabled {
                state.toggle();
            }
        });
        root
    }

    /// Radix's `SwitchThumb`: a `span` with the root's `data-state`.
    pub fn thumb(&self) -> Element {
        let thumb = create("span");
        if self.switch.disabled {
            set_attribute(&thumb, "data-disabled", "");
        }
        let state = self.clone();
        let weak = thumb.downgrade();
        create_effect(move || {
            let checked = state.is_checked();
            if let Some(thumb) = weak.upgrade() {
                set_attribute(&thumb, "data-state", state_name(checked));
            }
        });
        thumb
    }
}

fn create(tag: &str) -> Element {
    Element::create(&current_document(), tag).expect("create an element")
}

fn state_name(checked: bool) -> &'static str {
    if checked {
        "checked"
    } else {
        "unchecked"
    }
}
