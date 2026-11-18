//! Checkbox: a port of Radix's `react-checkbox` 1.4.1 (`Checkbox`,
//! `CheckboxIndicator`); its source is the specification.
//!
//! The root is a `button` with `role="checkbox"`, so a click and Space (on
//! release) toggle it through the browser's button activation. Unlike the
//! switch, Radix cancels Enter's `keydown`, so Enter does not toggle a
//! checkbox. An indeterminate checkbox becomes checked when toggled. The
//! indicator exists only while the checkbox is checked or indeterminate
//! (Radix's `Presence`, without `forceMount`). The parts set no visual
//! style beyond what Radix sets; `crate::checkbox` styles them as shadcn/ui
//! does.
//!
//! Div has no context API yet (RFC 0003, request 1), so the root's state is
//! a [`CheckboxState`] handle that both parts are built from. Radix's hidden
//! form input (`CheckboxBubbleInput`), which it renders after the button
//! inside a `<form>`, is out of scope until forms (RFC 0003).

use std::cell::RefCell;
use std::rc::Rc;

use div::prelude::{create_effect, current_document, with_document, Element, Signal};

use super::controllable::{Controllable, OnChange};
use super::keys::Key;
use super::roving_focus::{listen, set_attribute};
use crate::css::set;

/// Radix's `CheckedState`: `boolean | 'indeterminate'`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckedState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

impl CheckedState {
    /// Radix's `getState`: the `data-state` value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Checked => "checked",
            Self::Indeterminate => "indeterminate",
        }
    }

    /// The `aria-checked` value: `"mixed"` when indeterminate.
    pub fn aria(self) -> &'static str {
        match self {
            Self::Unchecked => "false",
            Self::Checked => "true",
            Self::Indeterminate => "mixed",
        }
    }

    /// The state a click requests: indeterminate becomes checked, the others
    /// turn over.
    pub fn toggled(self) -> Self {
        match self {
            Self::Unchecked | Self::Indeterminate => Self::Checked,
            Self::Checked => Self::Unchecked,
        }
    }
}

impl From<bool> for CheckedState {
    fn from(checked: bool) -> Self {
        if checked {
            Self::Checked
        } else {
            Self::Unchecked
        }
    }
}

/// How a [`CheckboxState`] starts. Radix's `Checkbox` props.
#[derive(Clone)]
pub struct CheckboxOptions {
    /// The state, held by the caller (controlled), or `None`.
    pub checked: Option<Signal<CheckedState>>,
    /// The state the checkbox starts in when uncontrolled.
    pub default_checked: CheckedState,
    /// Called with the new state when the user toggles the checkbox.
    pub on_checked_change: Option<OnChange<CheckedState>>,
    pub disabled: bool,
    /// Sets `aria-required="true"`; Radix leaves the attribute out unless
    /// `required` is given.
    pub required: bool,
    /// The button's `value` (`"on"` by default).
    pub value: String,
}

impl Default for CheckboxOptions {
    fn default() -> Self {
        Self {
            checked: None,
            default_checked: CheckedState::Unchecked,
            on_checked_change: None,
            disabled: false,
            required: false,
            value: "on".to_owned(),
        }
    }
}

struct Checkbox {
    checked: Controllable<CheckedState>,
    disabled: bool,
    required: bool,
    value: String,
}

/// The state of one checkbox, shared by its parts. Clones share it.
#[derive(Clone)]
pub struct CheckboxState {
    checkbox: Rc<Checkbox>,
}

impl CheckboxState {
    /// A new checkbox in the current document.
    pub fn new(options: CheckboxOptions) -> Self {
        let checked = match options.checked {
            Some(signal) => Controllable::controlled(signal, options.on_checked_change),
            // Radix: `defaultProp: defaultChecked ?? false`.
            None => Controllable::uncontrolled(options.default_checked, options.on_checked_change),
        };
        Self {
            checkbox: Rc::new(Checkbox {
                checked,
                disabled: options.disabled,
                required: options.required,
                value: options.value,
            }),
        }
    }

    /// The current state. Inside an effect, the effect reruns when it
    /// changes.
    pub fn checked(&self) -> CheckedState {
        self.checkbox.checked.get()
    }

    /// Toggle the checkbox, as a click does.
    pub fn toggle(&self) {
        self.checkbox.checked.set(self.checked().toggled());
    }

    pub fn disabled(&self) -> bool {
        self.checkbox.disabled
    }

    /// Radix's `Checkbox` root (its `CheckboxTrigger`): a `button` with
    /// `role="checkbox"` and `aria-checked` that a click toggles and whose
    /// Enter `keydown` is cancelled.
    pub fn root(&self) -> Element {
        let root = create("button");
        set_attribute(&root, "type", "button");
        set_attribute(&root, "role", "checkbox");
        if self.checkbox.required {
            set_attribute(&root, "aria-required", "true");
        }
        if self.checkbox.disabled {
            set_attribute(&root, "data-disabled", "");
            set_attribute(&root, "disabled", "");
        }
        set_attribute(&root, "value", &self.checkbox.value);

        let state = self.clone();
        let weak = root.downgrade();
        create_effect(move || {
            let checked = state.checked();
            if let Some(root) = weak.upgrade() {
                set_attribute(&root, "aria-checked", checked.aria());
                set_attribute(&root, "data-state", checked.name());
            }
        });

        // Radix: `if (event.key === "Enter") event.preventDefault()`. A
        // checkbox is toggled by Space, as a native one is.
        listen(&root, "keydown", |event| {
            if Key::of(event) == Some(Key::Enter) {
                event.prevent_default();
            }
        });

        // A disabled button receives no click in the browser; Div's does, so
        // the handler checks.
        let state = self.clone();
        listen(&root, "click", move |_| {
            if !state.checkbox.disabled {
                state.toggle();
            }
        });
        root
    }

    /// Radix's `CheckboxIndicator`: a `span` with the root's `data-state`
    /// and `pointer-events: none`, present only while the checkbox is
    /// checked or indeterminate. `parent` receives it when it appears, with
    /// `build` called on it first to add its content and style; it is
    /// removed when the checkbox is unchecked.
    pub fn indicator(&self, parent: &Element, build: impl Fn(&Element) + 'static) {
        let state = self.clone();
        let weak_parent = parent.downgrade();
        let disabled = self.checkbox.disabled;
        // The effect reruns from event handlers, outside the view's render
        // context; the indicator is built in the document it was declared in.
        let document = current_document();
        let current: RefCell<Option<Element>> = RefCell::new(None);
        create_effect(move || {
            let checked = state.checked();
            let Some(parent) = weak_parent.upgrade() else {
                return;
            };
            let present = checked != CheckedState::Unchecked;
            let mut current = current.borrow_mut();
            match (present, current.as_ref()) {
                (true, Some(indicator)) => {
                    set_attribute(indicator, "data-state", checked.name());
                }
                (true, None) => with_document(&document, || {
                    let indicator = create("span");
                    set_attribute(&indicator, "data-state", checked.name());
                    if disabled {
                        set_attribute(&indicator, "data-disabled", "");
                    }
                    // Radix: `style: { pointerEvents: "none", ...props.style }`.
                    set(&indicator, &[("pointer-events", "none")]);
                    build(&indicator);
                    parent
                        .append_child(&indicator)
                        .expect("attach the indicator");
                    *current = Some(indicator);
                }),
                (false, Some(indicator)) => {
                    indicator.remove().expect("detach the indicator");
                    *current = None;
                }
                (false, None) => {}
            }
        });
    }
}

fn create(tag: &str) -> Element {
    Element::create(&current_document(), tag).expect("create an element")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeterminate_toggles_to_checked() {
        assert_eq!(CheckedState::Indeterminate.toggled(), CheckedState::Checked);
        assert_eq!(CheckedState::Checked.toggled(), CheckedState::Unchecked);
        assert_eq!(CheckedState::Unchecked.toggled(), CheckedState::Checked);
        assert_eq!(CheckedState::Indeterminate.aria(), "mixed");
    }
}
