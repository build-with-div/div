//! Mutable text-node handles.
//!
//! A [`TextNode`] is a handle, as an [`Element`](crate::Element) is: the
//! document owns the node, and dropping the handle never removes it. Remove
//! a node with [`TextNode::remove`].

use crate::document::{Document, DocumentInner};
use crate::style::Error;
use div_engine::{NodeHandle, WeakNode};
use std::rc::{Rc, Weak};

pub struct TextNode {
    pub(crate) document: Document,
    pub(crate) handle: NodeHandle,
}

#[derive(Clone)]
pub struct WeakTextNode {
    document: Weak<DocumentInner>,
    handle: WeakNode,
}

impl WeakTextNode {
    pub fn upgrade(&self) -> Option<TextNode> {
        let inner = self.document.upgrade()?;
        let document = Document { inner };
        let handle = document
            .with_engine(|engine| self.handle.upgrade(engine))
            .ok()?
            .ok()?;
        Some(TextNode::from_handle(document, handle))
    }
}

impl TextNode {
    pub(crate) fn from_handle(document: Document, handle: NodeHandle) -> Self {
        Self { document, handle }
    }

    pub fn downgrade(&self) -> WeakTextNode {
        WeakTextNode {
            document: Rc::downgrade(&self.document.inner),
            handle: self.handle.downgrade(),
        }
    }

    pub fn set_data(&self, data: &str) -> Result<(), Error> {
        self.document
            .with_engine_mut(|engine| engine.set_text(self.handle, data.to_owned()))
    }

    pub fn data(&self) -> Result<String, Error> {
        Ok(self
            .document
            .with_engine(|engine| engine.text_data(self.handle).map(str::to_owned))??)
    }

    pub fn parent(&self) -> Result<Option<crate::Element>, Error> {
        let parent = self
            .document
            .with_engine(|engine| engine.parent(self.handle))??;
        Ok(parent.map(|handle| crate::Element::from_handle(self.document.clone(), handle)))
    }

    /// Detach this text node while keeping its handle and data for later
    /// reattachment. The document owns it until it is removed.
    pub fn detach(&mut self) -> Result<(), Error> {
        self.document
            .with_engine_mut(|engine| engine.detach(self.handle))
    }

    /// Remove this text node from the document and invalidate its handles.
    pub fn remove(self) -> Result<(), Error> {
        self.document.remove_node(self.handle)
    }
}
