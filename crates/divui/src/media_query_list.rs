//! `MediaQueryList` (RFC 0014, step 3): the handle
//! [`Document::match_media`] returns.
//!
//! The engine keeps the list and its cached result
//! ([`div_engine::Engine::match_media`]). This handle reads it. `change`
//! events are platform item 1.2's: its frame loop calls the engine's
//! "evaluate media queries and report changes" step.

use crate::document::{Document, DocumentInner};
use crate::style::Error;
use div_engine::{EngineError, MediaQueryListId};
use std::rc::{Rc, Weak};

/// A media query list from [`Document::match_media`], the web's
/// `MediaQueryList`.
///
/// Clones share one list and compare equal; two calls to `match_media`
/// make two lists, even for the same query. The list stays registered with
/// its document while any clone is alive and is released when the last one
/// drops. The handle does not keep the document alive.
#[derive(Clone)]
pub struct MediaQueryList {
    inner: Rc<Inner>,
}

struct Inner {
    document: Weak<DocumentInner>,
    id: MediaQueryListId,
    media: String,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(inner) = self.document.upgrade() else {
            return;
        };
        // The engine is only borrowed inside divui's own calls, which never
        // drop a handle, so the release cannot meet a borrowed engine.
        let _ = Document { inner }
            .with_engine_mut(|engine| engine.release_media_query_list(self.id));
    }
}

/// Shows the list's id and `media`. It never reads `matches`, which would
/// move the list's cached result.
impl std::fmt::Debug for MediaQueryList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaQueryList")
            .field("id", &self.inner.id)
            .field("media", &self.inner.media)
            .finish()
    }
}

/// The same list: clones of one [`Document::match_media`] result.
impl PartialEq for MediaQueryList {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for MediaQueryList {}

impl MediaQueryList {
    pub(crate) fn new(document: &Document, id: MediaQueryListId, media: String) -> Self {
        Self {
            inner: Rc::new(Inner {
                document: Rc::downgrade(&document.inner),
                id,
                media,
            }),
        }
    }

    /// The query, serialized as Chromium does: `"(min-width: 40em)"`,
    /// `"screen and (hover: hover)"`. Text that was not a valid query is
    /// `"not all"`.
    pub fn media(&self) -> String {
        self.inner.media.clone()
    }

    /// Whether the query matches the document's viewport, device scale
    /// factor, color scheme and reduced-motion preference now, without an
    /// update.
    ///
    /// The read is what the next frame's change report compares against, as
    /// in Chromium: after a color scheme change, reading `matches` before
    /// the next frame means no change event for this list. A viewport or
    /// reduced-motion change is reported whether or not it was read.
    ///
    /// Fails with [`EngineError::StaleHandle`] once the document is gone.
    pub fn matches(&self) -> Result<bool, Error> {
        let inner = self
            .inner
            .document
            .upgrade()
            .ok_or(Error::Engine(EngineError::StaleHandle))?;
        Document { inner }.with_engine_mut(|engine| engine.media_query_list_matches(self.inner.id))
    }
}
