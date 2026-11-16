//! `ResizeObserver` (RFC 0007, section 3).
//!
//! Observers, their targets and each target's last reported size live here.
//! The engine measures a target's box after layout
//! ([`div_engine::Engine::resize_observation`]); this module gathers the
//! targets whose observed size changed and delivers them between layouts of
//! [`Document::begin_frame`], as the HTML "update the rendering" steps and
//! the Resize Observer specification describe.

use crate::document::{Document, DocumentInner};
use crate::element::Element;
use crate::style::Rect;
use div_engine::{LogicalBoxSize, ResizeObservation};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Which box of a target an observation watches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ResizeObserverBox {
    /// The content box (the web's default).
    #[default]
    ContentBox,
    /// The border box.
    BorderBox,
    /// The content box in whole device pixels, snapped as it is painted.
    DevicePixelContentBox,
}

/// A size in the target's writing mode: `inline_size` is the width in a
/// horizontal writing mode and the height in a vertical one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResizeObserverSize {
    pub inline_size: f64,
    pub block_size: f64,
}

impl From<LogicalBoxSize> for ResizeObserverSize {
    fn from(size: LogicalBoxSize) -> Self {
        Self {
            inline_size: size.inline_size,
            block_size: size.block_size,
        }
    }
}

/// One target's sizes, delivered to a [`ResizeObserver`]'s callback.
///
/// Every entry carries all three sizes, whichever box was observed, as
/// Chromium's `ResizeObserverEntry` does. Where the web has arrays there is
/// one size: Chromium 147 reports one size per entry, stitched across a
/// box's fragments, and so does Div.
#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target: Element,
    /// The content box: `x` and `y` are the left and top padding.
    pub content_rect: Rect,
    pub border_box_size: ResizeObserverSize,
    pub content_box_size: ResizeObserverSize,
    pub device_pixel_content_box_size: ResizeObserverSize,
}

impl ResizeObserverEntry {
    fn new(target: Element, observation: &ResizeObservation) -> Self {
        Self {
            target,
            content_rect: observation.content_rect,
            border_box_size: observation.border_box.into(),
            content_box_size: observation.content_box.into(),
            device_pixel_content_box_size: observation.device_pixel_content_box.into(),
        }
    }
}

type Callback = dyn Fn(&[ResizeObserverEntry], &ResizeObserver);

struct Observation {
    target: Element,
    observed_box: ResizeObserverBox,
    /// `None` until the first delivery: the specification's initial
    /// last-reported size is -1 × -1, which no box has, so every target is
    /// delivered once, even one without a box.
    last_reported: Option<ResizeObserverSize>,
}

pub(crate) struct ObserverInner {
    document: Weak<DocumentInner>,
    callback: Box<Callback>,
    observations: RefCell<Vec<Observation>>,
    /// Indices into `observations` gathered for the current delivery.
    active: RefCell<Vec<(usize, ResizeObservation)>>,
    skipped: Cell<bool>,
}

/// Observes changes to the size of elements' boxes.
///
/// The callback runs during [`Document::begin_frame`], after style and
/// layout, with an entry for each observed target whose size in the observed
/// box changed since it was last delivered; every target is delivered once
/// when it is first observed. The callback runs with the document current
/// and untracked: it does not subscribe to the signals it reads. If it
/// changes sizes, the document lays out again and delivers the targets that
/// are deeper in the tree than the shallowest one just delivered; the rest
/// wait for the next frame, and the document reports
/// "ResizeObserver loop completed with undelivered notifications." through
/// [`Document::on_error`].
///
/// `ResizeObserver` is a handle: clones share one observer. When the last
/// handle is dropped, the observer stops observing.
///
/// ```no_run
/// use div::prelude::*;
///
/// let document = current_document();
/// let panel = document.element_by_id("panel").unwrap().unwrap();
/// let width = create_signal(0.0);
/// let observer = ResizeObserver::new(&document, move |entries, _| {
///     for entry in entries {
///         width.set(entry.content_box_size.inline_size);
///     }
/// });
/// observer.observe(&panel, ResizeObserverBox::ContentBox);
/// on_cleanup(move || observer.disconnect());
/// ```
#[derive(Clone)]
pub struct ResizeObserver {
    inner: Rc<ObserverInner>,
}

impl std::fmt::Debug for ResizeObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResizeObserver")
            .field("targets", &self.inner.observations.borrow().len())
            .finish()
    }
}

impl PartialEq for ResizeObserver {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ResizeObserver {}

impl ResizeObserver {
    /// Create an observer for targets in `document`.
    pub fn new(
        document: &Document,
        callback: impl Fn(&[ResizeObserverEntry], &ResizeObserver) + 'static,
    ) -> Self {
        let inner = Rc::new(ObserverInner {
            document: Rc::downgrade(&document.inner),
            callback: Box::new(callback),
            observations: RefCell::new(Vec::new()),
            active: RefCell::new(Vec::new()),
            skipped: Cell::new(false),
        });
        document
            .inner
            .resize_observers
            .borrow_mut()
            .observers
            .push(Rc::downgrade(&inner));
        Self { inner }
    }

    /// Observe `target`'s `observed_box`. Observing a target again with
    /// another box replaces its observation, which is then delivered afresh;
    /// with the same box it does nothing.
    ///
    /// The document's root ([`Document::body`]) is both the viewport and the
    /// root element, and is observed as the browser's `<html>` is: the
    /// viewport's scrollbars are outside its box. Its block size is the
    /// viewport's, though, where `<html>`'s `auto` height is its content's
    /// (RFC 0007, section 3).
    pub fn observe(&self, target: &Element, observed_box: ResizeObserverBox) {
        let mut observations = self.inner.observations.borrow_mut();
        if let Some(index) = observations
            .iter()
            .position(|observation| observation.target.handle == target.handle)
        {
            if observations[index].observed_box == observed_box {
                return;
            }
            observations.remove(index);
            self.inner.active.borrow_mut().clear();
        }
        observations.push(Observation {
            target: target.clone(),
            observed_box,
            last_reported: None,
        });
        drop(observations);
        if let Some(document) = self.inner.document.upgrade() {
            document.resize_observers.borrow().wants_frame.set(true);
        }
    }

    /// Stop observing `target`. Unobserving a target that is not observed
    /// does nothing.
    pub fn unobserve(&self, target: &Element) {
        let mut observations = self.inner.observations.borrow_mut();
        if let Some(index) = observations
            .iter()
            .position(|observation| observation.target.handle == target.handle)
        {
            observations.remove(index);
            self.inner.active.borrow_mut().clear();
        }
    }

    /// Stop observing every target.
    pub fn disconnect(&self) {
        self.inner.observations.borrow_mut().clear();
        self.inner.active.borrow_mut().clear();
        self.inner.skipped.set(false);
    }

    /// Gather this observer's targets whose observed size changed: those
    /// deeper than `deeper_than` become active, the rest are skipped. Returns
    /// the shallowest active depth.
    fn gather(&self, deeper_than: u32) -> Option<u32> {
        let mut active = Vec::new();
        let mut skipped = false;
        let mut shallowest = None::<u32>;
        let observations = self.inner.observations.borrow();
        for (index, observation) in observations.iter().enumerate() {
            let Ok(measured) = observation
                .target
                .document
                .with_engine_mut(|engine| engine.resize_observation(observation.target.handle))
            else {
                // A target whose node is gone has no size to report.
                continue;
            };
            let size: ResizeObserverSize = match observation.observed_box {
                ResizeObserverBox::ContentBox => measured.content_box,
                ResizeObserverBox::BorderBox => measured.border_box,
                ResizeObserverBox::DevicePixelContentBox => measured.device_pixel_content_box,
            }
            .into();
            if observation.last_reported == Some(size) {
                continue;
            }
            if measured.depth > deeper_than {
                shallowest = Some(shallowest.map_or(measured.depth, |d| d.min(measured.depth)));
                active.push((index, measured));
            } else {
                skipped = true;
            }
        }
        *self.inner.active.borrow_mut() = active;
        self.inner.skipped.set(skipped);
        shallowest
    }

    /// Deliver the gathered targets to the callback, recording their sizes.
    fn deliver(&self) {
        let active = std::mem::take(&mut *self.inner.active.borrow_mut());
        if active.is_empty() {
            return;
        }
        let entries = {
            let mut observations = self.inner.observations.borrow_mut();
            let mut entries = Vec::with_capacity(active.len());
            for (index, measured) in &active {
                let Some(observation) = observations.get_mut(*index) else {
                    continue;
                };
                observation.last_reported = Some(
                    match observation.observed_box {
                        ResizeObserverBox::ContentBox => measured.content_box,
                        ResizeObserverBox::BorderBox => measured.border_box,
                        ResizeObserverBox::DevicePixelContentBox => {
                            measured.device_pixel_content_box
                        }
                    }
                    .into(),
                );
                entries.push(ResizeObserverEntry::new(
                    observation.target.clone(),
                    measured,
                ));
            }
            entries
        };
        if entries.is_empty() {
            return;
        }
        (self.inner.callback)(&entries, self);
    }
}

/// A document's observers, in creation order.
#[derive(Default)]
pub(crate) struct ResizeObservers {
    observers: Vec<Weak<ObserverInner>>,
    /// Set when an observation was added or left undelivered, so the window
    /// runs a frame to deliver it.
    wants_frame: Cell<bool>,
}

const LOOP_ERROR: &str = "ResizeObserver loop completed with undelivered notifications.";

impl Document {
    /// Whether a resize observation waits for a frame: a target was observed
    /// and not yet delivered, or the last frame left one undelivered.
    pub fn has_pending_resize_observations(&self) -> bool {
        self.inner.resize_observers.borrow().wants_frame.get()
    }

    fn live_resize_observers(&self) -> Vec<ResizeObserver> {
        let mut registry = self.inner.resize_observers.borrow_mut();
        registry
            .observers
            .retain(|observer| observer.strong_count() > 0);
        registry
            .observers
            .iter()
            .filter_map(Weak::upgrade)
            .map(|inner| ResizeObserver { inner })
            .collect()
    }

    /// The resize observation steps of "update the rendering": gather at
    /// depth 0, then deliver, lay out and gather deeper until nothing is
    /// active, and report the loop error if targets were skipped.
    pub(crate) fn broadcast_resize_observations(&self) -> Result<(), crate::Error> {
        self.inner.resize_observers.borrow().wants_frame.set(false);
        if self.inner.resize_observers.borrow().observers.is_empty() {
            return Ok(());
        }
        let mut depth = 0;
        loop {
            let observers = self.live_resize_observers();
            let mut shallowest = None::<u32>;
            for observer in &observers {
                if let Some(found) = observer.gather(depth) {
                    shallowest = Some(shallowest.map_or(found, |d| d.min(found)));
                }
            }
            let Some(shallowest) = shallowest else {
                if observers
                    .iter()
                    .any(|observer| observer.inner.skipped.get())
                {
                    for observer in &observers {
                        observer.inner.skipped.set(false);
                    }
                    // The skipped targets keep their last reported size, so
                    // the next frame delivers them.
                    self.inner.resize_observers.borrow().wants_frame.set(true);
                    self.report_error(LOOP_ERROR);
                }
                return Ok(());
            };
            for observer in &observers {
                crate::context::with_document(self, || {
                    crate::runtime::untracked(|| observer.deliver())
                });
            }
            depth = shallowest;
            self.update_all()?;
        }
    }
}
