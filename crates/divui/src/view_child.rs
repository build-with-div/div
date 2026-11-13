//! `{expr}` children in `view!`: text or a view, chosen by the expression's
//! type.
//!
//! `view!` evaluates each `{expr}` in a `move` closure as a [`Value`], and
//! method resolution picks an impl, most specific first (autoref
//! specialization):
//!
//! - The value: `ToString` makes [`Text`]; otherwise `IntoView` makes
//!   [`View`]. Text wins, so a `String` stays text. Anything else is a
//!   compile error naming the type.
//! - The child: text is re-run in an effect, as before, so its closure must
//!   be `Fn + 'static` as before. A view is built once and mounted.
//!
//! How the closure reads the expression depends on its shape, because a
//! closure's `Fn`/`FnOnce` kind is settled after method resolution and so
//! cannot choose between impls:
//!
//! - A variable (`{label}`, `{children}`) moves into a [`Value`] when the
//!   child is built, as the old effect closure did, and the closure borrows
//!   that. Text reads it in place; a view takes it out once.
//! - Another place (`{props.title}`, `{self.name}`) is borrowed, as before,
//!   so it shows as text. To show a view stored in a field, move it out:
//!   `{props.content.into_view()}`.
//! - Any other expression is evaluated by value on each run.

use std::cell::Cell;

use crate::{Element, IntoView, ViewNode};

/// A `{expr}` value that is text.
pub struct Text(pub String);

/// A `{expr}` value that is a view.
pub struct View(pub ViewNode);

/// A `{expr}` value, held until it is shown.
pub struct Value<T>(Cell<Option<T>>);

impl<T> Value<T> {
    pub fn new(value: T) -> Self {
        Self(Cell::new(Some(value)))
    }

    fn take(&self) -> T {
        self.0.take().expect("view! child value taken twice")
    }
}

/// A `{expr}` child: the closure that evaluates it.
pub struct Child<F>(Cell<Option<F>>);

impl<F> Child<F> {
    pub fn new(evaluate: F) -> Self {
        Self(Cell::new(Some(evaluate)))
    }

    fn take(&self) -> F {
        self.0.take().expect("view! child built twice")
    }
}

/// Bring the dispatch traits into scope for the generated code.
pub mod traits {
    pub use super::{
        __ChildText as _, __ChildView as _, __ValueNotAChild as _, __ValueText as _,
        __ValueView as _,
    };
}

// ─── The value ─────────────────────────────────────────────────

pub trait __ValueText {
    fn __child_value(&self) -> Text;
}

impl<T: ToString> __ValueText for &&Value<T> {
    fn __child_value(&self) -> Text {
        // Read in place: a variable's value is shown again on each run.
        let value = self.take();
        let text = value.to_string();
        self.0.set(Some(value));
        Text(text)
    }
}

pub trait __ValueView {
    fn __child_value(&self) -> View;
}

impl<T: IntoView> __ValueView for &Value<T> {
    fn __child_value(&self) -> View {
        View(self.take().into_view())
    }
}

/// Neither text nor a view.
#[diagnostic::on_unimplemented(
    message = "`{Self}` in a `view!` `{{..}}` child is neither text (`ToString`) nor a view \
               (`IntoView`)",
    label = "neither text nor a view"
)]
pub trait ChildExpr {}

pub trait __ValueNotAChild {
    type Inner;
    fn __child_value(&self) -> Text
    where
        Self::Inner: ChildExpr;
}

impl<T> __ValueNotAChild for Value<T> {
    type Inner = T;
    fn __child_value(&self) -> Text
    where
        T: ChildExpr,
    {
        unreachable!("no type implements ChildExpr")
    }
}

// ─── The child ─────────────────────────────────────────────────

pub trait __ChildText {
    fn __into_view(&self) -> ViewNode;
}

impl<F: Fn() -> Text + 'static> __ChildText for &Child<F> {
    fn __into_view(&self) -> ViewNode {
        let evaluate = self.take();
        ViewNode::MountFn(Box::new(move |parent: &Element| {
            let text_node = parent
                .create_text_child(&evaluate().0)
                .expect("create dynamic text node");
            let text_weak = text_node.downgrade();
            // Transfer ownership to scope cleanup so the text node is
            // removed from the DOM when the enclosing scope is disposed.
            crate::on_cleanup(move || drop(text_node));
            crate::create_effect(move || {
                if let Some(node) = text_weak.upgrade() {
                    node.set_data(&evaluate().0).expect("update dynamic text");
                }
            });
        }))
    }
}

pub trait __ChildView {
    fn __into_view(&self) -> ViewNode;
}

impl<F: FnOnce() -> View> __ChildView for Child<F> {
    fn __into_view(&self) -> ViewNode {
        self.take()().0
    }
}
