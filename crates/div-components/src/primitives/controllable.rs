//! A value owned either by a primitive or by its caller, as Radix's
//! `useControllableState`.

use std::rc::Rc;

use div::prelude::{create_signal, Signal};

/// Called with the new value when a primitive changes it.
pub type OnChange<T> = Rc<dyn Fn(T)>;

/// A primitive's value: uncontrolled (the primitive holds it, seeded from a
/// default) or controlled (the caller holds it in a signal and updates it
/// from the change callback).
///
/// [`set`](Self::set) calls the change callback in both cases when the value
/// differs. In the controlled case it leaves the update to the caller, as
/// Radix does, so a caller that ignores the callback keeps the value.
pub struct Controllable<T: 'static> {
    value: Signal<T>,
    controlled: bool,
    on_change: Option<OnChange<T>>,
}

impl<T: 'static> Clone for Controllable<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            controlled: self.controlled,
            on_change: self.on_change.clone(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Controllable<T> {
    /// A value the primitive holds, starting at `default`.
    pub fn uncontrolled(default: T, on_change: Option<OnChange<T>>) -> Self {
        Self {
            value: create_signal(default),
            controlled: false,
            on_change,
        }
    }

    /// A value the caller holds in `value`.
    pub fn controlled(value: Signal<T>, on_change: Option<OnChange<T>>) -> Self {
        Self {
            value,
            controlled: true,
            on_change,
        }
    }

    /// The current value; inside an effect, the effect reruns when it
    /// changes.
    pub fn get(&self) -> T {
        self.value.get()
    }

    /// Request `next`: store it unless controlled, and report the change.
    pub fn set(&self, next: T) {
        if self.value.get() == next {
            return;
        }
        if !self.controlled {
            self.value.set(next.clone());
        }
        if let Some(on_change) = &self.on_change {
            on_change(next);
        }
    }

    /// Whether the caller holds the value.
    pub fn is_controlled(&self) -> bool {
        self.controlled
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn recorder() -> (Rc<RefCell<Vec<i32>>>, OnChange<i32>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        (seen, Rc::new(move |value| sink.borrow_mut().push(value)))
    }

    #[test]
    fn uncontrolled_stores_and_reports_changes_once() {
        let (seen, on_change) = recorder();
        let value = Controllable::uncontrolled(1, Some(on_change));
        value.set(1);
        value.set(2);
        value.set(2);
        assert_eq!(value.get(), 2);
        assert_eq!(*seen.borrow(), [2]);
    }

    #[test]
    fn controlled_reports_but_leaves_the_update_to_the_caller() {
        let (seen, on_change) = recorder();
        let signal = create_signal(1);
        let value = Controllable::controlled(signal, Some(on_change));
        value.set(2);
        assert_eq!(value.get(), 1);
        assert_eq!(*seen.borrow(), [2]);
        signal.set(2);
        assert_eq!(value.get(), 2);
        value.set(2);
        assert_eq!(*seen.borrow(), [2]);
    }
}
