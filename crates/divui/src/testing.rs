//! Headless UI tests: mount a view, find elements, send pointer, wheel and
//! key input, wait for the work it starts, and compare the rendered pixels
//! with accepted snapshots.
//!
//! Enable the `testing` feature for tests only:
//!
//! ```toml
//! [dev-dependencies]
//! divui = { ..., features = ["testing"] }
//! ```
//!
//! ```no_run
//! use div::prelude::*;
//! use div::testing::TestApp;
//!
//! let count = create_signal(0);
//! let mut app = TestApp::new(LogicalSize::new(200.0, 100.0)).mount(move || view! {
//!     <button on:click={move |_| count.set(count.get() + 1)}>"Add"</button>
//! });
//! app.click(&app.by_text("Add"));
//! assert_eq!(count.get(), 1);
//! div::assert_snapshot!(app, "after-one-click");
//! ```
//!
//! # Input and waiting
//!
//! Input returns once the event is dispatched and the UI work it made
//! ready has run, as a browser returns to the page after an event and its
//! microtasks. It does not wait for timers or background work, so a test
//! can act while a task is still running, and waits explicitly, as with
//! Testing Library's `waitFor`:
//!
//! ```no_run
//! # use div::prelude::*;
//! # use div::testing::TestApp;
//! # let mut app = TestApp::new(LogicalSize::new(200.0, 100.0)).mount(|| view! { <p>"Fetch"</p> });
//! app.click(&app.by_text("Fetch"));
//! app.click(&app.by_text("Cancel")); // the fetch is still running
//! app.wait_for(|app| app.find_text("Cancelled").is_some());
//! let row = app.find_by_text("Loaded"); // waits, then panics if absent
//! ```
//!
//! [`wait_for`](TestApp::wait_for) runs UI tasks, timers and frames until
//! its condition holds and panics after [`DEFAULT_WAIT_TIMEOUT`].
//!
//! Helpers panic with a description of what was missing, as test
//! assertions do.
//!
//! [`assert_snapshot!`](crate::assert_snapshot) compares exact RGBA with
//! `tests/snapshots/<name>.<os>.png` in the calling crate. Snapshots are
//! per operating system because system fonts differ. A missing snapshot
//! fails unless `DIV_UPDATE_SNAPSHOTS=1`, which writes or replaces it, so
//! CI never accepts a new image silently. A mismatch writes the rendering
//! next to the snapshot as `<name>.<os>.actual.png`.

use crate::app::{HeadlessApp, LogicalSize};
use crate::element::Element;
use crate::events::{KeyEventType, Modifiers, MouseButton};
use crate::pointer::{Point, PointerId, PointerInput, PointerInputKind};
use crate::view_node::IntoView;
use crate::Document;
use div_engine::ViewportMetrics;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The environment variable that lets snapshot assertions write snapshots.
pub const UPDATE_SNAPSHOTS: &str = "DIV_UPDATE_SNAPSHOTS";

/// How long [`TestApp::wait_for`], [`TestApp::find_by_text`] and
/// [`TestApp::find_by_id`] wait before they panic: 1 s, as Testing
/// Library's `waitFor`.
pub const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(1);

/// The interval between the frames [`TestApp::wait_for`] runs while it
/// waits: a 60 Hz display's.
const WAIT_FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// How long [`TestApp::run_until_idle`] runs before it panics.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// The touch contact the single-touch helpers use, as a first finger.
pub const FIRST_TOUCH: PointerId = PointerId(2);

/// A headless app under test.
pub struct TestApp {
    app: HeadlessApp,
    reported_errors: std::rc::Rc<std::cell::RefCell<Vec<crate::ReportedError>>>,
}

impl TestApp {
    /// An app with a viewport of `size` CSS pixels at scale 1.
    pub fn new(size: LogicalSize) -> Self {
        let viewport = ViewportMetrics::from_logical_size(size.width, size.height, 1.0)
            .expect("a positive, finite test viewport");
        let app = HeadlessApp::new(viewport).expect("a headless document");
        let reported_errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = reported_errors.clone();
        app.document()
            .on_error(move |error| collected.borrow_mut().push(error.clone()));
        Self {
            app,
            reported_errors,
        }
    }

    /// Mount `view` as the app's root and run the tasks it starts, to the
    /// end ([`run_until_idle`](Self::run_until_idle)).
    pub fn mount<V: IntoView>(mut self, view: impl FnOnce() -> V) -> Self {
        self.app.mount(view).expect("the view mounts");
        self.run_until_idle();
        self
    }

    pub fn document(&self) -> &Document {
        self.app.document()
    }

    pub fn headless(&mut self) -> &mut HeadlessApp {
        &mut self.app
    }

    /// The element with id `id`.
    pub fn by_id(&self, id: &str) -> Element {
        self.find_id(id)
            .unwrap_or_else(|| panic!("no element has id `{id}`"))
    }

    /// Like [`by_id`](Self::by_id), but `None` when nothing matches.
    pub fn find_id(&self, id: &str) -> Option<Element> {
        self.document()
            .element_by_id(id)
            .expect("the document answers")
    }

    /// The innermost element whose text, with surrounding white space
    /// trimmed, is `text`.
    pub fn by_text(&self, text: &str) -> Element {
        self.find_text(text)
            .unwrap_or_else(|| panic!("no element has the text `{text}`"))
    }

    /// Like [`by_text`](Self::by_text), but `None` when nothing matches.
    pub fn find_text(&self, text: &str) -> Option<Element> {
        innermost_with_text(&self.document().body(), text.trim())
    }

    /// Wait for an element with the text `text`, as
    /// [`wait_for`](Self::wait_for) does, and return it. Panics after
    /// [`DEFAULT_WAIT_TIMEOUT`] when none appears.
    pub fn find_by_text(&mut self, text: &str) -> Element {
        let mut found = None;
        let appeared = self.poll_until(DEFAULT_WAIT_TIMEOUT, |app| {
            found = app.find_text(text);
            found.is_some()
        });
        match found {
            Some(element) if appeared => element,
            _ => panic!(
                "no element has the text `{text}` after {:?}",
                DEFAULT_WAIT_TIMEOUT
            ),
        }
    }

    /// Wait for the element with id `id`, as [`wait_for`](Self::wait_for)
    /// does, and return it. Panics after [`DEFAULT_WAIT_TIMEOUT`] when none
    /// appears.
    pub fn find_by_id(&mut self, id: &str) -> Element {
        let mut found = None;
        let appeared = self.poll_until(DEFAULT_WAIT_TIMEOUT, |app| {
            found = app.find_id(id);
            found.is_some()
        });
        match found {
            Some(element) if appeared => element,
            _ => panic!("no element has id `{id}` after {:?}", DEFAULT_WAIT_TIMEOUT),
        }
    }

    /// Run UI tasks, timers and frames until `condition` holds. Panics
    /// after [`DEFAULT_WAIT_TIMEOUT`].
    ///
    /// The condition is checked first, then after each round of work. While
    /// it waits, a frame ([`advance_frame`](Self::advance_frame)) runs
    /// every 16 ms of real time, by the time that passed, so animations,
    /// animation frame callbacks and scroll events advance with the timers.
    pub fn wait_for(&mut self, condition: impl FnMut(&TestApp) -> bool) {
        self.wait_for_with(DEFAULT_WAIT_TIMEOUT, condition);
    }

    /// [`wait_for`](Self::wait_for) with another timeout.
    pub fn wait_for_with(&mut self, timeout: Duration, condition: impl FnMut(&TestApp) -> bool) {
        if !self.poll_until(timeout, condition) {
            panic!("the condition did not hold within {timeout:?}");
        }
    }

    /// Whether `condition` held within `timeout`.
    fn poll_until(
        &mut self,
        timeout: Duration,
        mut condition: impl FnMut(&TestApp) -> bool,
    ) -> bool {
        let start = Instant::now();
        let mut last_frame = start;
        loop {
            self.run_ready_tasks();
            if condition(self) {
                return true;
            }
            let now = Instant::now();
            let elapsed = now - start;
            if elapsed >= timeout {
                return false;
            }
            let next_frame = WAIT_FRAME_INTERVAL.saturating_sub(now - last_frame);
            crate::task::wait_for_work(next_frame.min(timeout - elapsed));
            let now = Instant::now();
            if now - last_frame >= WAIT_FRAME_INTERVAL {
                self.app
                    .advance_frame((now - last_frame).as_secs_f64() * 1000.0)
                    .expect("the frame runs");
                last_frame = now;
            }
        }
    }

    /// The centre of `element`'s border box, in viewport coordinates.
    pub fn center_of(&self, element: &Element) -> Point {
        let rect = element
            .bounding_rect()
            .expect("the document answers")
            .unwrap_or_else(|| panic!("the element has no box"));
        Point::new(
            f64::from(rect.x + rect.width / 2.0),
            f64::from(rect.y + rect.height / 2.0),
        )
    }

    /// Send one pointer input, then run the UI work it made ready.
    pub fn pointer(&mut self, input: PointerInput) {
        self.send(input);
        self.run_ready_tasks();
    }

    /// Move the mouse to `at` while holding `modifiers`.
    pub fn pointer_move(&mut self, at: Point, modifiers: Modifiers) {
        self.pointer(mouse(PointerInputKind::Move, at).with_modifiers(modifiers));
    }

    /// Press the mouse's `button` at `at` while holding `modifiers`.
    pub fn pointer_down(&mut self, at: Point, button: MouseButton, modifiers: Modifiers) {
        self.pointer(
            mouse(PointerInputKind::Down, at)
                .with_button(button)
                .with_modifiers(modifiers),
        );
    }

    /// Release the mouse's `button` at `at` while holding `modifiers`.
    pub fn pointer_up(&mut self, at: Point, button: MouseButton, modifiers: Modifiers) {
        self.pointer(
            mouse(PointerInputKind::Up, at)
                .with_button(button)
                .with_modifiers(modifiers),
        );
    }

    /// Move the mouse to the centre of `element`, then press and release
    /// the primary button there, as a user would. The click reaches
    /// whatever is drawn there.
    ///
    /// The click's `detail` is 1 however soon after another click it comes,
    /// as with Playwright's and Puppeteer's `click`: the helpers give each
    /// press its count, so a test does not depend on the double-click time.
    /// [`pointer_down`](Self::pointer_down) and
    /// [`pointer_up`](Self::pointer_up) leave the count to the document,
    /// which counts as the platform does.
    pub fn click(&mut self, element: &Element) {
        self.click_with(element, MouseButton::Main, 1);
    }

    /// [`click`](Self::click) with the secondary button: `auxclick`, then
    /// `contextmenu`.
    pub fn right_click(&mut self, element: &Element) {
        self.click_with(element, MouseButton::Secondary, 1);
    }

    /// Double-click the centre of `element`: two press and release pairs
    /// whose `detail` is 1, then 2, so the second `click` is followed by
    /// `dblclick`.
    pub fn double_click(&mut self, element: &Element) {
        self.click_with(element, MouseButton::Main, 2);
    }

    /// Triple-click the centre of `element`: `detail` 1, 2, then 3, and one
    /// `dblclick`, after the second `click`.
    pub fn triple_click(&mut self, element: &Element) {
        self.click_with(element, MouseButton::Main, 3);
    }

    fn click_with(&mut self, element: &Element, button: MouseButton, count: u32) {
        let at = self.center_of(element);
        self.send(mouse(PointerInputKind::Move, at));
        for n in 1..=count {
            self.send(
                mouse(PointerInputKind::Down, at)
                    .with_button(button)
                    .with_click_count(n),
            );
            self.send(
                mouse(PointerInputKind::Up, at)
                    .with_button(button)
                    .with_click_count(n),
            );
        }
        self.run_ready_tasks();
    }

    /// Drag with the primary button from `from` to `to`: move to `from`,
    /// press, move in `steps` equal steps (at least one) and release at
    /// `to`.
    pub fn drag(&mut self, from: Point, to: Point, steps: usize) {
        self.send(mouse(PointerInputKind::Move, from));
        self.send(mouse(PointerInputKind::Down, from));
        for at in path(from, to, steps) {
            self.send(mouse(PointerInputKind::Move, at));
        }
        self.send(mouse(PointerInputKind::Up, to));
        self.run_ready_tasks();
    }

    /// Turn the wheel over `at` by `delta` CSS pixels; positive scrolls
    /// right or down. The `scroll` events follow at the next frame, as in
    /// Chromium ([`advance_frame`](Self::advance_frame),
    /// [`wait_for`](Self::wait_for) or [`run_until_idle`](Self::run_until_idle)).
    pub fn wheel(&mut self, at: Point, delta: Point) {
        self.document()
            .clone()
            .dispatch_wheel_event(
                at.x as f32,
                at.y as f32,
                delta.x as f32,
                delta.y as f32,
                Modifiers::NONE,
            )
            .expect("the wheel event dispatches");
        self.run_ready_tasks();
    }

    /// Start touch contact `id` at `at`.
    pub fn touch_down(&mut self, id: PointerId, at: Point) {
        self.pointer(touch(id, PointerInputKind::Down, at));
    }

    /// Move touch contact `id` to `at`.
    pub fn touch_move(&mut self, id: PointerId, at: Point) {
        self.pointer(touch(id, PointerInputKind::Move, at));
    }

    /// End touch contact `id` at `at`.
    pub fn touch_up(&mut self, id: PointerId, at: Point) {
        self.pointer(touch(id, PointerInputKind::Up, at));
    }

    /// Cancel touch contact `id`, as the platform does when it takes the
    /// gesture.
    pub fn touch_cancel(&mut self, id: PointerId, at: Point) {
        self.pointer(touch(id, PointerInputKind::Cancel, at));
    }

    /// Tap the centre of `element` with [`FIRST_TOUCH`]: its compatibility
    /// mouse events, then `click`, whose `detail` is 1 however soon after
    /// another tap it comes, as with [`click`](Self::click).
    pub fn tap(&mut self, element: &Element) {
        let at = self.center_of(element);
        self.send(touch(FIRST_TOUCH, PointerInputKind::Down, at));
        self.send(touch(FIRST_TOUCH, PointerInputKind::Up, at).with_click_count(1));
        self.run_ready_tasks();
    }

    /// [`drag`](Self::drag) with [`FIRST_TOUCH`].
    pub fn touch_drag(&mut self, from: Point, to: Point, steps: usize) {
        self.send(touch(FIRST_TOUCH, PointerInputKind::Down, from));
        for at in path(from, to, steps) {
            self.send(touch(FIRST_TOUCH, PointerInputKind::Move, at));
        }
        self.send(touch(FIRST_TOUCH, PointerInputKind::Up, to));
        self.run_ready_tasks();
    }

    fn send(&self, input: PointerInput) {
        self.document()
            .clone()
            .dispatch_pointer_input(input)
            .expect("the pointer input dispatches");
    }

    /// Type `text` into the focused element, one key per character.
    pub fn type_text(&mut self, text: &str) {
        let document = self.document().clone();
        let mut buffer = [0; 4];
        for character in text.chars() {
            let text = &*character.encode_utf8(&mut buffer);
            let code = key_code_of(character);
            document
                .dispatch_key_input(
                    KeyEventType::Down,
                    code,
                    Some(text),
                    Some(text),
                    Modifiers::NONE,
                )
                .expect("the key dispatches");
            document
                .dispatch_key_input(KeyEventType::Up, code, Some(text), None, Modifiers::NONE)
                .expect("the key dispatches");
        }
        self.run_ready_tasks();
    }

    /// Press and release `key` with no modifiers.
    pub fn press(&mut self, key: Key) {
        self.press_with(Modifiers::NONE, key);
    }

    /// Press and release `key` while holding `modifiers`.
    pub fn press_with(&mut self, modifiers: Modifiers, key: Key) {
        let document = self.document().clone();
        document
            .dispatch_key(KeyEventType::Down, key, modifiers, false)
            .expect("the key dispatches");
        document
            .dispatch_key(KeyEventType::Up, key, modifiers, false)
            .expect("the key dispatches");
        self.run_ready_tasks();
    }

    /// Run the UI tasks that are ready now, without waiting for timers or
    /// background work. Input runs this after its events.
    pub fn run_ready_tasks(&mut self) {
        self.app.run_ready_tasks().expect("the ready tasks run");
    }

    /// Run UI tasks, the timers and background work they await, and the
    /// pending `scroll` events, until all are done. Panics after 30 s of
    /// unfinished work.
    pub fn run_until_idle(&mut self) {
        let deadline = Instant::now() + IDLE_TIMEOUT;
        loop {
            self.app.run_until_idle().expect("tasks finish within 30 s");
            let document = self.document().clone();
            if !document.has_pending_events().expect("the document answers") {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "events were still pending after {IDLE_TIMEOUT:?}"
            );
            document
                .dispatch_pending_events()
                .expect("the pending events dispatch");
        }
    }

    /// Run one frame `delta_ms` milliseconds after the last: the pending
    /// `scroll` events, animations, the callbacks of
    /// [`Document::request_animation_frame`](crate::Document::request_animation_frame),
    /// style and layout, and the [`ResizeObserver`](crate::ResizeObserver)
    /// callbacks. Then run the UI tasks the frame made ready. The app
    /// starts at time 0.
    pub fn advance_frame(&mut self, delta_ms: f64) {
        self.app.advance_frame(delta_ms).expect("the frame runs");
        self.run_ready_tasks();
    }

    /// Run `count` frames, each `delta_ms` after the one before.
    pub fn advance_frames(&mut self, count: usize, delta_ms: f64) {
        for _ in 0..count {
            self.advance_frame(delta_ms);
        }
    }

    /// The errors the document reported ([`Document::on_error`](crate::Document::on_error))
    /// since the last call, oldest first. A test app collects them instead
    /// of printing them.
    pub fn take_reported_errors(&mut self) -> Vec<crate::ReportedError> {
        std::mem::take(&mut *self.reported_errors.borrow_mut())
    }

    /// The current frame as PNG bytes, at the time of the last frame
    /// ([`advance_frame`](Self::advance_frame)).
    pub fn render_png(&self) -> Vec<u8> {
        let time_ms = self.document().get_time().expect("the document answers");
        self.app.render_png_at(time_ms).expect("the frame renders")
    }

    /// Compare the current frame with snapshot `name` in `directory`. Used
    /// by [`assert_snapshot!`](crate::assert_snapshot), which passes the
    /// calling crate's `tests/snapshots`.
    pub fn check_snapshot(
        &self,
        directory: impl AsRef<Path>,
        name: &str,
    ) -> Result<Snapshot, String> {
        let update = std::env::var_os(UPDATE_SNAPSHOTS).is_some_and(|value| !value.is_empty());
        compare_snapshot(directory.as_ref(), name, &self.render_png(), update)
    }
}

fn mouse(kind: PointerInputKind, at: Point) -> PointerInput {
    PointerInput::mouse(kind, at.x as f32, at.y as f32)
}

fn touch(id: PointerId, kind: PointerInputKind, at: Point) -> PointerInput {
    PointerInput::touch(id, kind, at.x as f32, at.y as f32)
}

/// The points of `steps` equal steps from `from` to `to`, ending at `to`.
fn path(from: Point, to: Point, steps: usize) -> impl Iterator<Item = Point> {
    let steps = steps.max(1);
    (1..=steps).map(move |step| {
        let t = step as f64 / steps as f64;
        Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
    })
}

/// What a snapshot comparison did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Snapshot {
    /// The frame matched the accepted snapshot exactly.
    Matched,
    /// The frame was written as the snapshot, because updates were allowed.
    Written(PathBuf),
}

/// Compare the frame with `directory/name.<os>.png`.
#[doc(hidden)]
pub fn compare_snapshot(
    directory: &Path,
    name: &str,
    png: &[u8],
    update: bool,
) -> Result<Snapshot, String> {
    let os = std::env::consts::OS;
    let accepted = directory.join(format!("{name}.{os}.png"));
    let actual = directory.join(format!("{name}.{os}.actual.png"));
    let frame = decode(png).map_err(|error| format!("the frame is not a PNG: {error}"))?;
    let write = |path: &Path| {
        std::fs::create_dir_all(directory)
            .and_then(|()| std::fs::write(path, png))
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    };
    let difference = match std::fs::read(&accepted) {
        Ok(bytes) => match decode(&bytes) {
            Ok(expected) => difference(&expected, &frame),
            Err(error) => Some(format!("the snapshot is not a readable PNG: {error}")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Some("there is no accepted snapshot".to_owned())
        }
        Err(error) => return Err(format!("could not read {}: {error}", accepted.display())),
    };
    match difference {
        None => {
            let _ = std::fs::remove_file(&actual);
            Ok(Snapshot::Matched)
        }
        Some(_) if update => {
            write(&accepted)?;
            let _ = std::fs::remove_file(&actual);
            Ok(Snapshot::Written(accepted))
        }
        Some(difference) => {
            write(&actual)?;
            Err(format!(
                "snapshot `{name}` does not match {}: {difference}. The frame is in {}. \
                 If the change is intended, rerun with {UPDATE_SNAPSHOTS}=1 to accept it.",
                accepted.display(),
                actual.display()
            ))
        }
    }
}

/// Assert that `app`'s current frame matches snapshot `name` in the calling
/// crate's `tests/snapshots` directory. See [`testing`](crate::testing).
#[macro_export]
macro_rules! assert_snapshot {
    ($app:expr, $name:expr $(,)?) => {
        if let Err(message) = $app.check_snapshot(
            ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("snapshots"),
            $name,
        ) {
            panic!("{message}");
        }
    };
}

/// A key for [`TestApp::press`]: the crate's [`Key`](crate::Key).
pub use crate::Key;

/// The key code a character's key reports: its upper-case letter or digit,
/// else 0.
fn key_code_of(character: char) -> i32 {
    if character.is_ascii_alphanumeric() {
        character.to_ascii_uppercase() as i32
    } else if character == ' ' {
        32
    } else {
        0
    }
}

fn innermost_with_text(element: &Element, text: &str) -> Option<Element> {
    let mut child = element.first_child().ok().flatten();
    while let Some(current) = child {
        if let Some(found) = innermost_with_text(&current, text) {
            return Some(found);
        }
        child = current.next_sibling().ok().flatten();
    }
    let own = element.text_content().ok()?;
    (own.trim() == text).then(|| element.clone())
}

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode(png: &[u8]) -> Result<Image, png::DecodingError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buffer)?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        other => unreachable!("the ALPHA transformation leaves no {other:?}"),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// How `actual` differs from `expected`, or `None` when they are equal.
fn difference(expected: &Image, actual: &Image) -> Option<String> {
    if (expected.width, expected.height) != (actual.width, actual.height) {
        return Some(format!(
            "the frame is {}x{}, the snapshot {}x{}",
            actual.width, actual.height, expected.width, expected.height
        ));
    }
    let mut first = None;
    let mut count = 0_u64;
    for (index, (want, got)) in expected
        .rgba
        .chunks_exact(4)
        .zip(actual.rgba.chunks_exact(4))
        .enumerate()
    {
        if want != got {
            count += 1;
            first.get_or_insert((index, want.to_vec(), got.to_vec()));
        }
    }
    let (index, want, got) = first?;
    let (x, y) = (index as u32 % actual.width, index as u32 / actual.width);
    Some(format!(
        "{count} pixels differ; the first, at ({x}, {y}), is {got:?} instead of {want:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_signal, view};

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("div-testing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    fn app(color: div_style::Color) -> TestApp {
        TestApp::new(LogicalSize::new(40.0, 20.0)).mount(move || {
            view! { <div style:width="20px" style:height="10px" style:background-color={color}></div> }
        })
    }

    #[test]
    fn snapshots_are_written_only_on_request_and_compared_exactly() {
        let directory = scratch("snapshots");
        let red = app(div_style::Color::from_hex(0xff0000, false)).render_png();
        let blue = app(div_style::Color::from_hex(0x0000ff, false)).render_png();
        let os = std::env::consts::OS;

        let missing = compare_snapshot(&directory, "box", &red, false).unwrap_err();
        assert!(missing.contains("no accepted snapshot"), "{missing}");
        assert!(directory.join(format!("box.{os}.actual.png")).exists());

        let accepted = directory.join(format!("box.{os}.png"));
        assert_eq!(
            compare_snapshot(&directory, "box", &red, true),
            Ok(Snapshot::Written(accepted.clone()))
        );
        assert!(!directory.join(format!("box.{os}.actual.png")).exists());
        assert_eq!(
            compare_snapshot(&directory, "box", &red, false),
            Ok(Snapshot::Matched)
        );

        let changed = compare_snapshot(&directory, "box", &blue, false).unwrap_err();
        assert!(changed.contains("200 pixels differ"), "{changed}");
        assert_eq!(
            std::fs::read(&accepted).unwrap(),
            red,
            "a mismatch keeps the snapshot"
        );
        assert_eq!(
            std::fs::read(directory.join(format!("box.{os}.actual.png"))).unwrap(),
            blue
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn lookups_clicks_and_typing_reach_the_document() {
        let clicks = create_signal(0);
        let mut app = TestApp::new(LogicalSize::new(200.0, 120.0)).mount(move || {
            view! {
                <div>
                    <p id="title">"  Notes  "</p>
                    <button on:click={move |_| clicks.set(clicks.get() + 1)}>
                        <span>"Add"</span>
                    </button>
                    <input id="field" />
                </div>
            }
        });
        assert_eq!(app.by_id("title").text_content().unwrap(), "  Notes  ");
        assert_eq!(
            app.by_text("Notes").get_attribute("id").unwrap().as_deref(),
            Some("title")
        );
        assert!(app.find_text("Missing").is_none());
        let add = app.by_text("Add");
        app.click(&add);
        assert_eq!(clicks.get(), 1, "one press and release is one click");

        let field = app.by_id("field");
        app.click(&field);
        app.type_text("hi 1");
        app.press(Key::Backspace);
        app.press(Key::Char('!'));
        assert_eq!(field.control_value().unwrap().as_deref(), Some("hi !"));
    }

    /// Records the type of each event in `types` that reaches `element`,
    /// with the pointer type for pointer events.
    fn record(element: &Element, types: &[&str]) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        for event_type in types {
            let log = log.clone();
            element
                .on(event_type, move |event| {
                    let entry = match event.pointer_info() {
                        Some(info) => format!("{}:{:?}", event.event_type, info.pointer_type),
                        None => event.event_type.clone(),
                    };
                    log.borrow_mut().push(entry);
                })
                .unwrap();
        }
        log
    }

    /// How long the fetch in [`fetch_app`] takes.
    const FETCH_TIME: Duration = Duration::from_millis(150);

    fn fetch_app() -> (TestApp, crate::Signal<String>) {
        use crate::task::{spawn_local, time::sleep, Task};
        let status = create_signal(String::from("Idle"));
        let fetch: std::rc::Rc<std::cell::RefCell<Option<Task<()>>>> = Default::default();
        let cancel = fetch.clone();
        let app = TestApp::new(LogicalSize::new(200.0, 120.0)).mount(move || {
            view! {
                <div>
                    <button on:click={move |_| {
                        status.set("Loading".into());
                        let task = spawn_local(async move {
                            sleep(FETCH_TIME).await;
                            status.set("Loaded".into());
                        });
                        *fetch.borrow_mut() = Some(task);
                    }}>"Fetch"</button>
                    <button on:click={move |_| {
                        cancel.borrow_mut().take();
                        status.set("Cancelled".into());
                    }}>"Cancel"</button>
                    <p>{status.get()}</p>
                </div>
            }
        });
        (app, status)
    }

    #[test]
    fn input_returns_before_the_work_it_starts_and_waits_are_explicit() {
        let (mut app, status) = fetch_app();
        app.click(&app.by_text("Fetch"));
        assert_eq!(
            status.get(),
            "Loading",
            "the click does not wait for the fetch"
        );
        app.click(&app.by_text("Cancel"));
        assert_eq!(status.get(), "Cancelled");
        app.run_until_idle();
        assert_eq!(status.get(), "Cancelled", "cancelling dropped the fetch");

        // The fetch's timer starts during this click, so a clock started
        // before it bounds the wait however slowly the click itself runs.
        let before_click = Instant::now();
        app.click(&app.by_text("Fetch"));
        assert!(app.find_text("Loading").is_some());
        let loaded = app.find_by_text("Loaded");
        assert!(before_click.elapsed() >= FETCH_TIME);
        assert_eq!(loaded.text_content().unwrap(), "Loaded");
    }

    fn panic_message(result: std::thread::Result<()>) -> String {
        let payload = result.unwrap_err();
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap()
    }

    #[test]
    fn waits_time_out_with_a_message() {
        assert_eq!(DEFAULT_WAIT_TIMEOUT, Duration::from_secs(1));
        let mut app = app(div_style::Color::from_hex(0xff0000, false));
        let started = Instant::now();
        let timed_out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.wait_for_with(Duration::from_millis(40), |_| false)
        }));
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert_eq!(
            panic_message(timed_out),
            "the condition did not hold within 40ms"
        );
        let missing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.find_by_text("Missing");
        }));
        assert_eq!(
            panic_message(missing),
            "no element has the text `Missing` after 1s"
        );
    }

    #[test]
    fn waits_run_frames() {
        let mut app = app(div_style::Color::from_hex(0xff0000, false));
        let frame_time = create_signal(None::<f64>);
        app.document()
            .request_animation_frame(move |time_ms| frame_time.set(Some(time_ms)));
        app.run_until_idle();
        assert_eq!(
            frame_time.get(),
            None,
            "only frames run animation frame callbacks"
        );
        app.wait_for(|_| frame_time.get().is_some());
        assert!(frame_time.get().unwrap() >= 16.0);
    }

    #[test]
    fn click_helpers_count_clicks() {
        let mut app = TestApp::new(LogicalSize::new(200.0, 200.0)).mount(|| {
            view! {
                <div>
                    <div id="target" style:width="100px" style:height="100px"></div>
                </div>
            }
        });
        let target = app.by_id("target");
        let log: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
        for name in ["mousedown", "mouseup", "click", "dblclick"] {
            let log = log.clone();
            target
                .on(name, move |event| {
                    let detail = event.mouse_info().unwrap().detail;
                    log.borrow_mut()
                        .push(format!("{}:{detail}", event.event_type));
                })
                .unwrap();
        }
        let take = || std::mem::take(&mut *log.borrow_mut()).join(" ");

        app.click(&target);
        app.click(&target);
        assert_eq!(
            take(),
            "mousedown:1 mouseup:1 click:1 mousedown:1 mouseup:1 click:1",
            "each click counts 1, however soon after another"
        );
        app.double_click(&target);
        assert_eq!(
            take(),
            "mousedown:1 mouseup:1 click:1 mousedown:2 mouseup:2 click:2 dblclick:2"
        );
        app.triple_click(&target);
        assert_eq!(
            take(),
            "mousedown:1 mouseup:1 click:1 mousedown:2 mouseup:2 click:2 dblclick:2 \
             mousedown:3 mouseup:3 click:3"
        );
        app.tap(&target);
        app.tap(&target);
        assert_eq!(
            take(),
            "mousedown:1 mouseup:1 click:1 mousedown:1 mouseup:1 click:1"
        );
    }

    #[test]
    fn pointer_helpers_send_chromium_sequences() {
        let mut app = TestApp::new(LogicalSize::new(200.0, 200.0)).mount(|| {
            view! {
                <div>
                    <div id="target" style:width="100px" style:height="100px"></div>
                </div>
            }
        });
        let target = app.by_id("target");
        let log = record(
            &target,
            &[
                "pointerdown",
                "pointermove",
                "pointerup",
                "mousedown",
                "mouseup",
                "click",
                "auxclick",
                "contextmenu",
                "gotpointercapture",
                "lostpointercapture",
            ],
        );
        let take = || std::mem::take(&mut *log.borrow_mut());

        app.right_click(&target);
        // `contextmenu` follows the release on Windows and the press on
        // Linux, as in Chromium.
        let expected: &[&str] = if cfg!(target_os = "windows") {
            &[
                "pointermove:Mouse",
                "pointerdown:Mouse",
                "mousedown",
                "pointerup:Mouse",
                "mouseup",
                "auxclick:Mouse",
                "contextmenu:Mouse",
            ]
        } else {
            &[
                "pointermove:Mouse",
                "pointerdown:Mouse",
                "mousedown",
                "contextmenu:Mouse",
                "pointerup:Mouse",
                "mouseup",
                "auxclick:Mouse",
            ]
        };
        assert_eq!(take(), expected);

        let captor = target.clone();
        target
            .on("pointerdown", move |event| {
                let id = event.pointer_info().unwrap().pointer_id;
                captor.set_pointer_capture(id).unwrap();
            })
            .unwrap();
        app.drag(Point::new(50.0, 50.0), Point::new(180.0, 180.0), 4);
        let log = take();
        assert_eq!(
            log.iter()
                .filter(|entry| entry.as_str() == "pointermove:Mouse")
                .count(),
            1 + 4,
            "the move to the start, then four steps, the last three outside \
             the target but captured by it: {log:?}"
        );
        assert!(
            log.contains(&"gotpointercapture:Mouse".to_owned()),
            "{log:?}"
        );
        assert!(
            log.contains(&"lostpointercapture:Mouse".to_owned()),
            "{log:?}"
        );

        app.tap(&target);
        let log = take();
        assert_eq!(
            log.first().map(String::as_str),
            Some("pointerdown:Touch"),
            "{log:?}"
        );
        assert_eq!(
            log.last().map(String::as_str),
            Some("click:Touch"),
            "{log:?}"
        );
        assert!(log.contains(&"mousedown".to_owned()), "{log:?}");

        app.touch_down(FIRST_TOUCH, Point::new(10.0, 10.0));
        app.touch_move(FIRST_TOUCH, Point::new(20.0, 20.0));
        app.touch_up(FIRST_TOUCH, Point::new(20.0, 20.0));
        let log = take();
        assert_eq!(
            log.first().map(String::as_str),
            Some("pointerdown:Touch"),
            "{log:?}"
        );
        assert!(log.contains(&"pointermove:Touch".to_owned()), "{log:?}");
        assert!(log.contains(&"pointerup:Touch".to_owned()), "{log:?}");
    }

    #[test]
    fn wheel_scrolls_and_its_scroll_events_follow_at_the_frame_or_idle() {
        let mut app = TestApp::new(LogicalSize::new(200.0, 200.0)).mount(|| {
            view! {
                <div id="port" style:width="100px" style:height="50px" style:overflow="auto">
                    <div style:height="400px"></div>
                </div>
            }
        });
        let port = app.by_id("port");
        let scrolls = record(&port, &["scroll"]);
        let at = app.center_of(&port);
        app.wheel(at, Point::new(0.0, 40.0));
        assert_eq!(port.scroll_top().unwrap(), 40.0);
        assert!(
            scrolls.borrow().is_empty(),
            "scroll events wait for the frame"
        );
        app.run_until_idle();
        assert_eq!(*scrolls.borrow(), ["scroll"]);

        app.wheel(at, Point::new(0.0, 40.0));
        app.wait_for(|_| scrolls.borrow().len() == 2);
        assert_eq!(port.scroll_top().unwrap(), 80.0);
    }
}
