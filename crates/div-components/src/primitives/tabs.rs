//! Tabs: a port of Radix's `react-tabs` 1.2.1 (`Tabs`, `TabsList`,
//! `TabsTrigger`, `TabsContent`); its source is the specification.
//!
//! Div has no context API yet (RFC 0003, request 1), so the root's state is
//! a [`TabsState`] handle that each part is built from. The parts set no
//! visual style beyond what Radix sets; `crate::tabs` styles them as
//! shadcn/ui does.
//!
//! Two interims stand in for engine features:
//!
//! - Div has no user-agent rule for the `hidden` attribute, so an inactive
//!   panel also gets an inline `display: none`.
//! - Radix unmounts an inactive panel's children and mounts them again when
//!   it becomes active. Here the panel's element children are detached and
//!   attached again, so they keep their state; text directly inside the
//!   panel stays (hidden with it), because text nodes cannot be moved
//!   through the public API.

use std::cell::RefCell;
use std::rc::Rc;

use div::prelude::{create_effect, current_document, mount_view, Element, Signal, ViewNode};
use div::typed_style::{Display, StyleProperty};

use super::controllable::{Controllable, OnChange};
use super::id::unique_id;
use super::keys::Key;
use super::roving_focus::{
    is_primary_press, is_target, listen, set_attribute, Direction, ItemOptions, RovingFocusGroup,
    RovingFocusOptions,
};
use crate::Orientation;

/// When a focused trigger selects its tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivationMode {
    /// On receiving focus (Radix's default).
    #[default]
    Automatic,
    /// Only on a click, Enter or Space.
    Manual,
}

/// How a [`TabsState`] starts. Radix's `Tabs` and `TabsList` props.
#[derive(Clone)]
pub struct TabsOptions {
    /// The selected tab, held by the caller (controlled), or `None`.
    pub value: Option<Signal<String>>,
    /// The tab selected first when uncontrolled; none when `None`.
    pub default_value: Option<String>,
    /// Called with a tab's value when the user selects it.
    pub on_value_change: Option<OnChange<String>>,
    pub orientation: Orientation,
    pub dir: Direction,
    pub activation_mode: ActivationMode,
    /// Whether arrow keys wrap around the list (`TabsList`'s `loop`, true by
    /// default).
    pub looping: bool,
}

impl Default for TabsOptions {
    fn default() -> Self {
        Self {
            value: None,
            default_value: None,
            on_value_change: None,
            orientation: Orientation::Horizontal,
            dir: Direction::Ltr,
            activation_mode: ActivationMode::Automatic,
            looping: true,
        }
    }
}

struct Tabs {
    value: Controllable<String>,
    base_id: String,
    orientation: Orientation,
    dir: Direction,
    activation_mode: ActivationMode,
    roving: RovingFocusGroup,
}

/// The state of one tabs root, shared by its parts. Clones share it.
#[derive(Clone)]
pub struct TabsState {
    tabs: Rc<Tabs>,
}

impl TabsState {
    /// New tabs in the current document.
    pub fn new(options: TabsOptions) -> Self {
        let value = match options.value {
            Some(signal) => Controllable::controlled(signal, options.on_value_change),
            // Radix: `defaultProp: defaultValue ?? ""`.
            None => Controllable::uncontrolled(
                options.default_value.unwrap_or_default(),
                options.on_value_change,
            ),
        };
        Self {
            tabs: Rc::new(Tabs {
                value,
                base_id: unique_id("tabs"),
                orientation: options.orientation,
                dir: options.dir,
                activation_mode: options.activation_mode,
                roving: RovingFocusGroup::new(RovingFocusOptions {
                    orientation: Some(options.orientation),
                    dir: options.dir,
                    looping: options.looping,
                }),
            }),
        }
    }

    /// The selected tab's value. Inside an effect, the effect reruns when
    /// the selection changes.
    pub fn value(&self) -> String {
        self.tabs.value.get()
    }

    /// Whether `value` is the selected tab.
    pub fn is_selected(&self, value: &str) -> bool {
        self.tabs.value.get() == value
    }

    /// Select the tab `value`, as the user does.
    pub fn select(&self, value: &str) {
        self.tabs.value.set(value.to_owned());
    }

    pub fn orientation(&self) -> Orientation {
        self.tabs.orientation
    }

    pub fn dir(&self) -> Direction {
        self.tabs.dir
    }

    fn trigger_id(&self, value: &str) -> String {
        format!("{}-trigger-{value}", self.tabs.base_id)
    }

    fn content_id(&self, value: &str) -> String {
        format!("{}-content-{value}", self.tabs.base_id)
    }

    /// Radix's `Tabs` root: a `div` with `dir` and `data-orientation`.
    pub fn root(&self) -> Element {
        let root = create("div");
        set_attribute(&root, "dir", self.tabs.dir.name());
        set_attribute(&root, "data-orientation", self.tabs.orientation.name());
        root
    }

    /// Radix's `TabsList`: a `tablist` that is a looping roving focus group.
    pub fn list(&self) -> Element {
        let list = create("div");
        set_attribute(&list, "role", "tablist");
        set_attribute(&list, "aria-orientation", self.tabs.orientation.name());
        self.tabs.roving.attach_root(&list);
        list
    }

    /// Radix's `TabsTrigger` for the tab `value`: a `button` with
    /// `role="tab"` that is an item of the list's roving focus group.
    pub fn trigger(&self, value: &str, disabled: bool) -> Element {
        let trigger = create("button");
        set_attribute(&trigger, "type", "button");
        set_attribute(&trigger, "role", "tab");
        set_attribute(&trigger, "aria-controls", &self.content_id(value));
        if disabled {
            set_attribute(&trigger, "data-disabled", "");
            set_attribute(&trigger, "disabled", "");
        }
        trigger
            .set_id(&self.trigger_id(value))
            .expect("set the trigger id");

        let state = self.clone();
        let weak = trigger.downgrade();
        let tab = value.to_owned();
        create_effect(move || {
            let selected = state.is_selected(&tab);
            if let Some(trigger) = weak.upgrade() {
                set_attribute(&trigger, "aria-selected", bool_name(selected));
                set_attribute(&trigger, "data-state", state_name(selected));
            }
        });

        // The trigger's own handlers come before the roving item's, as
        // Radix's `asChild` composition runs the child's handler first.
        let state = self.clone();
        let weak = trigger.downgrade();
        let tab = value.to_owned();
        listen(&trigger, "mousedown", move |event| {
            if !disabled && is_primary_press(event) {
                if let Some(trigger) = weak.upgrade() {
                    let _ = trigger.focus();
                }
                state.select(&tab);
            } else {
                event.prevent_default();
            }
        });

        let state = self.clone();
        let weak = trigger.downgrade();
        let tab = value.to_owned();
        listen(&trigger, "keydown", move |event| {
            let on_trigger = weak
                .upgrade()
                .is_some_and(|trigger| is_target(event, &trigger));
            if disabled || !on_trigger {
                return;
            }
            if matches!(Key::of(event), Some(Key::Space | Key::Enter)) {
                state.select(&tab);
            }
        });

        let state = self.clone();
        let tab = value.to_owned();
        listen(&trigger, "focusin", move |_| {
            let automatic = state.tabs.activation_mode != ActivationMode::Manual;
            if !state.is_selected(&tab) && !disabled && automatic {
                state.select(&tab);
            }
        });

        let state = self.clone();
        let tab = value.to_owned();
        self.tabs.roving.attach_item(
            &trigger,
            ItemOptions {
                tab_stop_id: None,
                focusable: !disabled,
                active: Rc::new(move || state.is_selected(&tab)),
            },
        );
        trigger
    }

    /// Radix's `TabsContent` for the tab `value`: a `tabpanel` holding
    /// `children` while its tab is selected.
    pub fn content(&self, value: &str, children: Vec<ViewNode>) -> Element {
        let panel = create("div");
        set_attribute(&panel, "data-orientation", self.tabs.orientation.name());
        set_attribute(&panel, "role", "tabpanel");
        set_attribute(&panel, "aria-labelledby", &self.trigger_id(value));
        panel
            .set_id(&self.content_id(value))
            .expect("set the panel id");
        set_attribute(&panel, "tabindex", "0");
        for child in children {
            mount_view(&panel, child);
        }

        let state = self.clone();
        let weak = panel.downgrade();
        let tab = value.to_owned();
        let detached: RefCell<Vec<Element>> = RefCell::new(Vec::new());
        create_effect(move || {
            let selected = state.is_selected(&tab);
            let Some(panel) = weak.upgrade() else {
                return;
            };
            set_attribute(&panel, "data-state", state_name(selected));
            if selected {
                panel.remove_attribute("hidden").expect("show the panel");
                panel
                    .remove_property(StyleProperty::Display)
                    .expect("show the panel");
                for child in detached.borrow_mut().drain(..) {
                    panel
                        .append_child(&child)
                        .expect("attach the panel's children");
                }
            } else {
                set_attribute(&panel, "hidden", "");
                panel
                    .set_property(StyleProperty::Display, Display::None.into())
                    .expect("hide the panel");
                let mut children = Vec::new();
                let mut child = panel.first_child().expect("the document answers");
                while let Some(current) = child {
                    child = current.next_sibling().expect("the document answers");
                    children.push(current);
                }
                for current in children {
                    current.detach().expect("detach the panel's children");
                    detached.borrow_mut().push(current);
                }
            }
        });
        panel
    }
}

fn create(tag: &str) -> Element {
    Element::create(&current_document(), tag).expect("create an element")
}

fn bool_name(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn state_name(selected: bool) -> &'static str {
    if selected {
        "active"
    } else {
        "inactive"
    }
}
