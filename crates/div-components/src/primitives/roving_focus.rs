//! Roving focus: one item of a group is in the tab order, and arrow keys
//! move focus between items. A port of Radix's `react-roving-focus` 1.2.1
//! (`RovingFocusGroup`, `RovingFocusGroupItem`); its source is the
//! specification.
//!
//! Radix moves focus for an arrow key in a `setTimeout`; here it moves
//! inside the `keydown` listener. Arrow keys on a group item have no
//! default action, so nothing runs in between and the result after the key
//! is the same.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use div::prelude::{
    create_effect, create_signal, current_document, on_cleanup, Document, Element, Event,
    MouseButton, Signal, WeakElement,
};

use super::collection::sort_in_tree_order;
use super::id::unique_id;
use super::keys::{Held, Key};
use crate::Orientation;

/// Reading direction, as the `dir` attribute.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    /// The `dir` attribute value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// How a group is set up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RovingFocusOptions {
    /// The arrow keys that move focus; both axes when `None`.
    pub orientation: Option<Orientation>,
    pub dir: Direction,
    /// Whether moving past the last item wraps to the first.
    pub looping: bool,
}

/// What an item contributes to its group.
pub struct ItemOptions {
    /// The item's tab stop id; a unique one when `None`.
    pub tab_stop_id: Option<String>,
    /// Whether the item takes focus (Radix's `focusable`).
    pub focusable: bool,
    /// Whether the item is the group's active one, which focus entering the
    /// group moves to first (Radix's `active`).
    pub active: Rc<dyn Fn() -> bool>,
}

struct Item {
    id: String,
    element: WeakElement,
    focusable: bool,
    active: Rc<dyn Fn() -> bool>,
}

struct Group {
    options: RovingFocusOptions,
    document: Document,
    current_tab_stop: Signal<Option<String>>,
    tabbing_back_out: Signal<bool>,
    focusable_count: Signal<usize>,
    click_focus: Cell<bool>,
    items: RefCell<Vec<Item>>,
}

/// A roving focus group. Clones share the group.
#[derive(Clone)]
pub struct RovingFocusGroup {
    group: Rc<Group>,
}

impl RovingFocusGroup {
    /// A group in the current document with no current tab stop, as
    /// Radix's default.
    pub fn new(options: RovingFocusOptions) -> Self {
        Self {
            group: Rc::new(Group {
                options,
                document: current_document(),
                current_tab_stop: create_signal(None),
                tabbing_back_out: create_signal(false),
                focusable_count: create_signal(0),
                click_focus: Cell::new(false),
                items: RefCell::new(Vec::new()),
            }),
        }
    }

    pub fn options(&self) -> RovingFocusOptions {
        self.group.options
    }

    /// The current tab stop's id.
    pub fn current_tab_stop(&self) -> Option<String> {
        self.group.current_tab_stop.get()
    }

    /// Make `root` the group's element: Radix's `RovingFocusGroupImpl`
    /// attributes and its `mousedown`, focus and blur handlers.
    pub fn attach_root(&self, root: &Element) {
        let group = &self.group;
        if let Some(orientation) = group.options.orientation {
            set_attribute(root, "data-orientation", orientation.name());
        }
        // Radix's `style={{ outline: "none" }}`.
        crate::css::set(root, &[("outline-style", "none")]);

        let tabindex_root = root.downgrade();
        let state = group.clone();
        create_effect(move || {
            let excluded = state.tabbing_back_out.get() || state.focusable_count.get() == 0;
            if let Some(root) = tabindex_root.upgrade() {
                set_attribute(&root, "tabindex", if excluded { "-1" } else { "0" });
            }
        });

        let state = group.clone();
        listen(root, "mousedown", move |_| state.click_focus.set(true));

        // React's `onFocus` and `onBlur` are `focusin` and `focusout`.
        let state = group.clone();
        let weak_root = root.downgrade();
        listen(root, "focusin", move |event| {
            let keyboard_focus = !state.click_focus.get();
            let on_root = weak_root
                .upgrade()
                .is_some_and(|root| is_target(event, &root));
            if on_root && keyboard_focus && !state.tabbing_back_out.get() {
                let items = state.focusable_items();
                let active = items.iter().find(|item| (item.active)());
                let current_id = state.current_tab_stop.get();
                let current = items
                    .iter()
                    .find(|item| Some(&item.id) == current_id.as_ref());
                let candidates: Vec<Element> = active
                    .into_iter()
                    .chain(current)
                    .chain(items.iter())
                    .filter_map(|item| item.element.upgrade())
                    .collect();
                focus_first(&state.document, &candidates);
            }
            state.click_focus.set(false);
        });

        let state = group.clone();
        listen(root, "focusout", move |_| state.tabbing_back_out.set(false));
    }

    /// Make `element` an item of the group, as Radix's
    /// `RovingFocusGroupItem`, and return its tab stop id. The item leaves
    /// the group when the reactive scope it was attached in is disposed.
    ///
    /// Listeners run in the order they were added, so a part that adds its
    /// own handlers first sees each event before the item, as with Radix's
    /// `asChild` composition.
    pub fn attach_item(&self, element: &Element, options: ItemOptions) -> String {
        let group = &self.group;
        let id = options.tab_stop_id.unwrap_or_else(|| unique_id("roving"));
        let focusable = options.focusable;
        group.items.borrow_mut().push(Item {
            id: id.clone(),
            element: element.downgrade(),
            focusable,
            active: options.active,
        });
        if focusable {
            group.focusable_count.update(|count| *count += 1);
        }
        // Scope cleanups run while the reactive runtime is borrowed, so this
        // one cannot touch signals: a removed item leaves the registry, but
        // the root keeps counting it toward Radix's `focusableItemsCount`.
        // That count only decides whether the root is a tab stop when the
        // group has no focusable item at all.
        let state = group.clone();
        let cleanup_id = id.clone();
        on_cleanup(move || {
            state
                .items
                .borrow_mut()
                .retain(|item| item.id != cleanup_id);
        });

        // Radix's `Collection.ItemSlot` marks each item.
        set_attribute(element, "data-radix-collection-item", "");
        if let Some(orientation) = group.options.orientation {
            set_attribute(element, "data-orientation", orientation.name());
        }

        let state = group.clone();
        let weak = element.downgrade();
        let tab_stop = id.clone();
        create_effect(move || {
            let current = state.current_tab_stop.get();
            if let Some(element) = weak.upgrade() {
                let tabindex = if current.as_deref() == Some(&tab_stop) {
                    "0"
                } else {
                    "-1"
                };
                set_attribute(&element, "tabindex", tabindex);
            }
        });

        let state = group.clone();
        let item_id = id.clone();
        listen(element, "mousedown", move |event| {
            if focusable {
                state.current_tab_stop.set(Some(item_id.clone()));
            } else {
                event.prevent_default();
            }
        });

        let state = group.clone();
        let item_id = id.clone();
        listen(element, "focusin", move |_| {
            state.current_tab_stop.set(Some(item_id.clone()));
        });

        let state = group.clone();
        let weak = element.downgrade();
        listen(element, "keydown", move |event| {
            let Some(key) = Key::of(event) else {
                return;
            };
            let held = Held::of(event);
            if key == Key::Tab && held.shift {
                state.tabbing_back_out.set(true);
                return;
            }
            let Some(element) = weak.upgrade() else {
                return;
            };
            if !is_target(event, &element) {
                return;
            }
            let Some(intent) = focus_intent(key, state.options.orientation, state.options.dir)
            else {
                return;
            };
            if held.any() {
                return;
            }
            event.prevent_default();
            let mut candidates: Vec<Element> = state
                .focusable_items()
                .iter()
                .filter_map(|item| item.element.upgrade())
                .collect();
            match intent {
                FocusIntent::First => {}
                FocusIntent::Last => candidates.reverse(),
                FocusIntent::Previous | FocusIntent::Next => {
                    if intent == FocusIntent::Previous {
                        candidates.reverse();
                    }
                    // Radix's `indexOf`: -1 when the item is not focusable.
                    let start = candidates
                        .iter()
                        .position(|candidate| same(candidate, &element))
                        .map_or(0, |index| index + 1);
                    candidates = if state.options.looping {
                        let len = candidates.len();
                        (0..len)
                            .map(|offset| candidates[(start + offset) % len].clone())
                            .collect()
                    } else {
                        candidates.split_off(start.min(candidates.len()))
                    };
                }
            }
            focus_first(&state.document, &candidates);
        });

        id
    }

    /// Set the current tab stop, as an item receiving focus does.
    pub fn focus_item(&self, id: &str) {
        self.group.current_tab_stop.set(Some(id.to_owned()));
    }
}

impl Group {
    /// The focusable items, in tree order.
    fn focusable_items(&self) -> Vec<FocusableItem> {
        let mut items: Vec<FocusableItem> = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.focusable)
            .filter_map(|item| {
                item.element.upgrade().map(|_| FocusableItem {
                    id: item.id.clone(),
                    element: item.element.clone(),
                    active: item.active.clone(),
                })
            })
            .collect();
        sort_in_tree_order(&mut items, |item| {
            item.element.upgrade().expect("upgraded above")
        });
        items
    }
}

struct FocusableItem {
    id: String,
    element: WeakElement,
    active: Rc<dyn Fn() -> bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusIntent {
    First,
    Last,
    Previous,
    Next,
}

/// Radix's `getFocusIntent`: the move a key asks for, if any.
fn focus_intent(key: Key, orientation: Option<Orientation>, dir: Direction) -> Option<FocusIntent> {
    // `getDirectionAwareKey`: in right-to-left text the horizontal arrows swap.
    let key = match (dir, key) {
        (Direction::Rtl, Key::ArrowLeft) => Key::ArrowRight,
        (Direction::Rtl, Key::ArrowRight) => Key::ArrowLeft,
        (_, key) => key,
    };
    match (orientation, key) {
        (Some(Orientation::Vertical), Key::ArrowLeft | Key::ArrowRight) => None,
        (Some(Orientation::Horizontal), Key::ArrowUp | Key::ArrowDown) => None,
        (_, Key::ArrowLeft | Key::ArrowUp) => Some(FocusIntent::Previous),
        (_, Key::ArrowRight | Key::ArrowDown) => Some(FocusIntent::Next),
        (_, Key::PageUp | Key::Home) => Some(FocusIntent::First),
        (_, Key::PageDown | Key::End) => Some(FocusIntent::Last),
        _ => None,
    }
}

/// Radix's `focusFirst`: focus the first candidate that takes focus, unless
/// focus is already on the one reached first.
pub(crate) fn focus_first(document: &Document, candidates: &[Element]) {
    let previous = document.focused_element().expect("the document answers");
    for candidate in candidates {
        if previous
            .as_ref()
            .is_some_and(|previous| same(previous, candidate))
        {
            return;
        }
        // An element that cannot take focus is skipped, as `focus()` is a
        // no-op on it in the DOM.
        let _ = candidate.focus();
        let now = document.focused_element().expect("the document answers");
        let changed = match (&previous, &now) {
            (Some(previous), Some(now)) => !same(previous, now),
            (None, None) => false,
            _ => true,
        };
        if changed {
            return;
        }
    }
}

pub(crate) fn same(left: &Element, right: &Element) -> bool {
    left.is_same_node(right).expect("the document answers")
}

/// Whether `element` is the event's target (`event.target ===
/// event.currentTarget` for a listener on `element`).
pub(crate) fn is_target(event: &Event, element: &Element) -> bool {
    event.target().is_some_and(|target| same(&target, element))
}

/// Whether a `mousedown` is the primary button without Ctrl, which Radix's
/// tab trigger selects on.
pub(crate) fn is_primary_press(event: &Event) -> bool {
    event.mouse_button == MouseButton::Left as i32 && !Held::of(event).ctrl
}

pub(crate) fn set_attribute(element: &Element, name: &str, value: &str) {
    element
        .set_attribute(name, value)
        .unwrap_or_else(|error| panic!("set {name}: {error}"));
}

pub(crate) fn listen(element: &Element, event_type: &str, callback: impl Fn(&Event) + 'static) {
    element
        .on(event_type, callback)
        .unwrap_or_else(|error| panic!("listen for {event_type}: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_intent_follows_orientation_and_direction() {
        let horizontal = Some(Orientation::Horizontal);
        let vertical = Some(Orientation::Vertical);
        assert_eq!(
            focus_intent(Key::ArrowRight, horizontal, Direction::Ltr),
            Some(FocusIntent::Next)
        );
        assert_eq!(
            focus_intent(Key::ArrowRight, horizontal, Direction::Rtl),
            Some(FocusIntent::Previous)
        );
        assert_eq!(
            focus_intent(Key::ArrowDown, horizontal, Direction::Ltr),
            None
        );
        assert_eq!(
            focus_intent(Key::ArrowRight, vertical, Direction::Ltr),
            None
        );
        assert_eq!(
            focus_intent(Key::ArrowUp, vertical, Direction::Ltr),
            Some(FocusIntent::Previous)
        );
        assert_eq!(
            focus_intent(Key::ArrowDown, None, Direction::Ltr),
            Some(FocusIntent::Next)
        );
        assert_eq!(
            focus_intent(Key::PageUp, horizontal, Direction::Ltr),
            Some(FocusIntent::First)
        );
        assert_eq!(
            focus_intent(Key::End, vertical, Direction::Rtl),
            Some(FocusIntent::Last)
        );
        assert_eq!(focus_intent(Key::Enter, None, Direction::Ltr), None);
    }
}
