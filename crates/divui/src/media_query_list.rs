//! `MediaQueryList` (RFC 0014, step 3): the handle
//! [`Document::match_media`] returns.
//!
//! The engine keeps the list and its cached result
//! ([`div_engine::Engine::match_media`]). This handle reads it, and
//! [`MediaQueryList::on_change`] listens for its `change` events, which
//! [`Document::begin_frame`] fires at HTML's "evaluate media queries and
//! report changes" step.

use crate::document::{Document, DocumentInner};
use crate::style::Error;
use div_engine::{EngineError, MediaQueryListId};
use std::rc::{Rc, Weak};

/// The web's `MediaQueryListEvent`: the `change` event of a
/// [`MediaQueryList`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQueryListEvent {
    media: String,
    matches: bool,
}

impl MediaQueryListEvent {
    /// The list's query, as [`MediaQueryList::media`].
    pub fn media(&self) -> &str {
        &self.media
    }

    /// Whether the list matched when the frame reported the change. It is
    /// the reported result, not a new evaluation.
    pub fn matches(&self) -> bool {
        self.matches
    }
}

type ChangeCallback = Rc<dyn Fn(&MediaQueryListEvent)>;

struct ChangeListener {
    id: u64,
    callback: ChangeCallback,
}

/// A list with `change` listeners. The document keeps it, as Chromium keeps
/// a list with listeners alive, so `match_media(q)?.on_change(..)` keeps
/// listening after the returned list is dropped.
struct ListenedList {
    list: MediaQueryList,
    listeners: Vec<ChangeListener>,
}

/// A document's `change` listeners, and whether a media input changed since
/// the last report.
#[derive(Default)]
pub(crate) struct MediaQueryListeners {
    /// In creation order, as the engine reports.
    listened: Vec<ListenedList>,
    /// Live lists of the document, with or without listeners.
    live: usize,
    report_pending: bool,
}

impl MediaQueryListeners {
    /// A media input changed. With live lists, the next frame must run, to
    /// report the change even when no style depends on it.
    pub(crate) fn note_input_change(&mut self) {
        if self.live > 0 {
            self.report_pending = true;
        }
    }

    #[cfg(any(
        all(feature = "linux", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub(crate) fn report_pending(&self) -> bool {
        self.report_pending
    }

    /// The listeners of `id` now, so listeners added during the dispatch
    /// are not called for it.
    fn snapshot(&self, id: MediaQueryListId) -> Option<(String, Vec<(u64, ChangeCallback)>)> {
        let entry = self
            .listened
            .iter()
            .find(|entry| entry.list.inner.id == id)?;
        Some((
            entry.list.inner.media.clone(),
            entry
                .listeners
                .iter()
                .map(|listener| (listener.id, listener.callback.clone()))
                .collect(),
        ))
    }

    /// Whether the listener `id` is still registered.
    fn contains(&self, id: u64) -> bool {
        self.listened
            .iter()
            .any(|entry| entry.listeners.iter().any(|listener| listener.id == id))
    }

    /// Remove the listener `id`. Returns what the caller drops after
    /// releasing the registry: the callback, and the list if no listener is
    /// left. `None` if no such listener is registered here.
    pub(crate) fn remove(&mut self, id: u64) -> Option<(ChangeCallback, Option<MediaQueryList>)> {
        let index = self
            .listened
            .iter()
            .position(|entry| entry.listeners.iter().any(|listener| listener.id == id))?;
        let entry = &mut self.listened[index];
        let position = entry
            .listeners
            .iter()
            .position(|listener| listener.id == id)?;
        let listener = entry.listeners.remove(position);
        let list = entry
            .listeners
            .is_empty()
            .then(|| self.listened.remove(index).list);
        Some((listener.callback, list))
    }
}

/// HTML's "evaluate media queries and report changes": fire `change` at
/// each list the engine reports, oldest first. Nothing is borrowed while a
/// listener runs, so listeners may use the document.
pub(crate) fn report_changes(document: &Document) -> Result<(), Error> {
    let registry = &document.inner.media_query_listeners;
    registry
        .try_borrow_mut()
        .map_err(|_| Error::ReentrantMutation)?
        .report_pending = false;
    let changes = document
        .with_engine_mut(|engine| Ok(engine.evaluate_media_queries_and_report_changes()))?;
    for (id, matches) in changes {
        let snapshot = registry
            .try_borrow()
            .map_err(|_| Error::ReentrantMutation)?
            .snapshot(id);
        let Some((media, listeners)) = snapshot else {
            continue;
        };
        let event = MediaQueryListEvent { media, matches };
        for (listener, callback) in listeners {
            // A listener removed by an earlier one is not called.
            let registered = registry
                .try_borrow()
                .map_err(|_| Error::ReentrantMutation)?
                .contains(listener);
            if registered {
                callback(&event);
            }
        }
    }
    Ok(())
}

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
        if let Ok(mut listeners) = inner.media_query_listeners.try_borrow_mut() {
            listeners.live = listeners.live.saturating_sub(1);
        }
        // The engine is only borrowed inside divui's own calls, which never
        // drop a handle, so the release cannot meet a borrowed engine.
        let _ =
            Document { inner }.with_engine_mut(|engine| engine.release_media_query_list(self.id));
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
        if let Ok(mut listeners) = document.inner.media_query_listeners.try_borrow_mut() {
            listeners.live += 1;
        }
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

    /// Listen for this list's `change` events, as the web's
    /// `addEventListener("change", callback)`. A frame
    /// ([`Document::begin_frame`]) fires one when the list's result changed
    /// since it was last reported or read, with the result in
    /// [`MediaQueryListEvent::matches`].
    ///
    /// Each call adds a listener. The handle removes exactly this listener;
    /// dropping it leaves the listener in place. While the list has a
    /// listener, its document keeps the list, so it keeps listening after
    /// every clone is dropped, as on the web.
    ///
    /// Fails with [`EngineError::StaleHandle`] once the document is gone.
    pub fn on_change(
        &self,
        callback: impl Fn(&MediaQueryListEvent) + 'static,
    ) -> Result<crate::ListenerHandle, Error> {
        let inner = self
            .inner
            .document
            .upgrade()
            .ok_or(Error::Engine(EngineError::StaleHandle))?;
        let id = crate::document::next_listener_id()?;
        let listener = ChangeListener {
            id,
            callback: Rc::new(callback),
        };
        {
            let mut listeners = inner
                .media_query_listeners
                .try_borrow_mut()
                .map_err(|_| Error::ReentrantMutation)?;
            let listened = &mut listeners.listened;
            match listened.iter_mut().find(|entry| entry.list == *self) {
                Some(entry) => entry.listeners.push(listener),
                None => {
                    let index =
                        listened.partition_point(|entry| entry.list.inner.id < self.inner.id);
                    listened.insert(
                        index,
                        ListenedList {
                            list: self.clone(),
                            listeners: vec![listener],
                        },
                    );
                }
            }
        }
        Ok(crate::ListenerHandle {
            document: Rc::downgrade(&inner),
            id,
        })
    }
}
