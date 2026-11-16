//! Errors a document reports instead of returning or panicking, the
//! framework's equivalent of the web's `error` event on `window`.

use crate::document::Document;
use std::rc::Rc;

/// An error a document reported while it ran a frame, for example
/// "ResizeObserver loop completed with undelivered notifications.".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedError {
    pub message: String,
}

impl std::fmt::Display for ReportedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

type Handler = Rc<dyn Fn(&ReportedError)>;

#[derive(Default)]
pub(crate) struct ErrorHandlers {
    handlers: Vec<Handler>,
}

impl Document {
    /// Call `handler` with each error the document reports from now on.
    /// Handlers run in the order they were added. While no handler is
    /// added, reported errors go to standard error, as Chromium's go to the
    /// console.
    pub fn on_error(&self, handler: impl Fn(&ReportedError) + 'static) {
        self.inner
            .error_handlers
            .borrow_mut()
            .handlers
            .push(Rc::new(handler));
    }

    pub(crate) fn report_error(&self, message: &str) {
        let error = ReportedError {
            message: message.to_owned(),
        };
        // Handlers may add handlers; run the ones present now.
        let handlers = self.inner.error_handlers.borrow().handlers.clone();
        if handlers.is_empty() {
            eprintln!("div: {error}");
            return;
        }
        for handler in handlers {
            crate::context::with_document(self, || crate::runtime::untracked(|| handler(&error)));
        }
    }
}
