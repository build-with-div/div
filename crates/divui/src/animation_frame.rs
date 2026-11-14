//! `requestAnimationFrame`: callbacks a document runs once, at the start of
//! its next frame (RFC 0007, section 2).

use crate::Document;
use std::collections::BTreeMap;
use std::num::NonZeroU64;

/// A callback requested with [`Document::request_animation_frame`], for
/// [`Document::cancel_animation_frame`]. Handles belong to the document that
/// returned them, as the web's request ids do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationFrameHandle(NonZeroU64);

/// The document's map of animation frame callbacks, in request order.
#[derive(Default)]
pub(crate) struct AnimationFrameCallbacks {
    last: u64,
    callbacks: BTreeMap<u64, Box<dyn FnOnce(f64)>>,
}

impl Document {
    /// Run `callback` once at the start of the document's next frame, before
    /// style and layout, as the web's `requestAnimationFrame` does.
    ///
    /// The callback receives the frame's time in milliseconds, the time the
    /// frame passes to [`begin_frame`](Self::begin_frame), which is also the
    /// document's animation time. Every callback of one frame receives the
    /// same time, and they run in the order they were requested. A callback
    /// requested while callbacks run waits for the next frame, so a callback
    /// that requests itself again runs once per frame.
    ///
    /// The callback runs with this document current
    /// ([`current_document`](crate::current_document)) and does not subscribe
    /// to the signals it reads. A pending callback makes the app's window
    /// want a frame, so the platform draws one even when nothing else
    /// changed. A minimized window draws no frames, so its callbacks wait
    /// until it is shown.
    ///
    /// ```no_run
    /// # use div::prelude::*;
    /// let document = current_document();
    /// let x = create_signal(0.0);
    /// let frame = document.request_animation_frame(move |time_ms| x.set(time_ms / 10.0));
    /// on_cleanup(move || document.cancel_animation_frame(frame));
    /// ```
    pub fn request_animation_frame(
        &self,
        callback: impl FnOnce(f64) + 'static,
    ) -> AnimationFrameHandle {
        let mut frames = self.inner.animation_frames.borrow_mut();
        let id = frames.last + 1;
        frames.last = id;
        frames.callbacks.insert(id, Box::new(callback));
        AnimationFrameHandle(NonZeroU64::new(id).expect("ids start at 1"))
    }

    /// Cancel a callback requested with
    /// [`request_animation_frame`](Self::request_animation_frame) that has
    /// not run yet, including one that is due later in the frame now
    /// running. A handle that already ran, was cancelled or came from
    /// another document is ignored.
    pub fn cancel_animation_frame(&self, handle: AnimationFrameHandle) {
        self.inner
            .animation_frames
            .borrow_mut()
            .callbacks
            .remove(&handle.0.get());
    }

    /// Whether a callback waits for the next frame.
    pub fn has_animation_frame_callbacks(&self) -> bool {
        !self.inner.animation_frames.borrow().callbacks.is_empty()
    }

    /// Run the callbacks requested before this call, in request order, with
    /// the frame's time. No borrow is held while a callback runs, so it may
    /// request or cancel callbacks.
    pub(crate) fn run_animation_frame_callbacks(&self, time_ms: f64) {
        let last = self.inner.animation_frames.borrow().last;
        loop {
            let next = {
                let mut frames = self.inner.animation_frames.borrow_mut();
                match frames.callbacks.first_entry() {
                    Some(entry) if *entry.key() <= last => Some(entry.remove()),
                    _ => None,
                }
            };
            let Some(callback) = next else {
                break;
            };
            crate::context::with_document(self, || crate::runtime::untracked(|| callback(time_ms)));
        }
    }
}
