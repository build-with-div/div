//! View node types and the [`IntoView`] trait.
//!
//! These types form the intermediate representation between the `view!`
//! macro output and the live DOM.

use crate::element::Element;

/// A renderable node in the UI tree.
pub enum ViewNode {
    /// A DOM element (possibly with children already appended).
    Element(Element),
    /// A static text string.
    Text(String),
    /// Multiple sibling nodes.
    Fragment(Vec<ViewNode>),
    /// Nothing — used for conditional rendering.
    Empty,
    /// Deferred mount action — a closure that runs with the parent element
    /// during [`mount_view`]. Used for dynamic text nodes that need a parent
    /// to create the text child on.
    MountFn(Box<dyn FnOnce(&Element)>),
}

/// Trait for anything that can be converted into a [`ViewNode`].
pub trait IntoView {
    /// Convert this value into a renderable view node.
    fn into_view(self) -> ViewNode;
}

impl IntoView for ViewNode {
    fn into_view(self) -> ViewNode {
        self
    }
}

impl IntoView for &str {
    fn into_view(self) -> ViewNode {
        ViewNode::Text(self.to_string())
    }
}

impl IntoView for String {
    fn into_view(self) -> ViewNode {
        ViewNode::Text(self)
    }
}

impl IntoView for Element {
    fn into_view(self) -> ViewNode {
        ViewNode::Element(self)
    }
}

impl<T: IntoView> IntoView for Vec<T> {
    fn into_view(self) -> ViewNode {
        ViewNode::Fragment(self.into_iter().map(IntoView::into_view).collect())
    }
}

impl<T: IntoView> IntoView for Option<T> {
    fn into_view(self) -> ViewNode {
        match self {
            Some(v) => v.into_view(),
            None => ViewNode::Empty,
        }
    }
}

/// Attach an `on:` listener written on a component to the view it returned.
///
/// `view!` calls this for `<Component on:event=handler>`. The listener goes
/// on the component's root element, or on each top-level element of a
/// fragment, so events from inside the component bubble to it as they do to
/// a listener on that element. (`Show`, `For` and `DynChild` render a
/// `display: contents` container, which receives it.) A view with no element
/// to receive it (only text, nothing, or dynamic text) panics naming the
/// component, rather than dropping the listener.
#[doc(hidden)]
pub fn __forward_listener(
    view: ViewNode,
    component: &str,
    event_type: &str,
    handler: impl Fn(&crate::Event) + 'static,
) -> ViewNode {
    fn roots<'a>(view: &'a ViewNode, found: &mut Vec<&'a Element>) {
        match view {
            ViewNode::Element(element) => found.push(element),
            ViewNode::Fragment(nodes) => nodes.iter().for_each(|node| roots(node, found)),
            ViewNode::Text(_) | ViewNode::Empty | ViewNode::MountFn(_) => {}
        }
    }
    let mut elements = Vec::new();
    roots(&view, &mut elements);
    assert!(
        !elements.is_empty(),
        "`<{component} on:{event_type}>`: `{component}` returns no element to          attach the listener to; give it a handler prop instead"
    );
    let handler = std::rc::Rc::new(handler);
    for element in elements {
        let handler = handler.clone();
        element
            .on(event_type, move |event| handler(event))
            .expect("forward a component listener");
    }
    view
}

/// Mount a [`ViewNode`] as a child of `parent`.
///
/// Elements are appended and then "forgotten" (ownership transfers to the
/// DOM tree). Text nodes are appended as proper DOM `Text` nodes.
pub fn mount_view(parent: &Element, view: ViewNode) {
    match view {
        ViewNode::Element(el) => {
            parent.append_child(&el).expect("mount element");
        }
        ViewNode::Text(text) => {
            parent.append_text_node(&text).expect("mount text");
        }
        ViewNode::Fragment(nodes) => {
            for node in nodes {
                mount_view(parent, node);
            }
        }
        ViewNode::Empty => {}
        ViewNode::MountFn(f) => f(parent),
    }
}
