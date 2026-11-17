//! Click counting (UI Events `detail`), as Chromium 147 counts repeated
//! clicks from platform input.
//!
//! The browser, not Blink, counts. The rules differ by platform:
//!
//! - **Windows** (`ui/events/blink/web_input_event_builders_win.cc`): the
//!   system's double-click time and box (`GetDoubleClickTime`,
//!   `SM_CXDOUBLECLK`, `SM_CYDOUBLECLK`), read for every event. A press
//!   counts on when it is within half the box of the first click of the
//!   series and within the time of the previous press, with the same button.
//!   The count has no limit. A move with no button held that leaves the box,
//!   or comes after the time, ends the series.
//! - **Linux** (`ui/events/event.cc`, `MouseEvent::GetRepeatCount`):
//!   Chromium's own 500 ms and 4×4 box (`ui::kDoubleClickTimeMs`), not the
//!   desktop's settings. A press counts on when it is within the time and
//!   half the box of the previous press, with the same button and modifiers.
//!   A triple click is the most: the press after it counts 1 again. A
//!   release reports its press's count.
//!
//! Touch taps are counted by the gesture detector instead ([`TapCounter`]):
//! a tap within 400 ms of the previous tap's release and less than 20 CSS
//! pixels from its press counts on, up to 3, then starts again
//! (`ui/events/gesture_detection/gesture_configuration.h`:
//! `double_tap_timeout_in_ms`, `max_distance_between_taps_for_double_tap`,
//! `max_tap_count`), on both platforms.
//!
//! The oracle is `tests/fidelity/fixtures/click-count-v1`, captured from
//! real platform input by `tools/qualification/capture_click_count_oracle.py`.
//! It shows that on Windows a move while a button is held (a drag, or a long
//! press that moves) does not end the series, and that the release still
//! clicks with the press's count.

use crate::events::{Modifiers, MouseButton};
use crate::pointer::PointerInputKind;
use std::time::Duration;

/// A platform's double-click limits: the time between presses, and the box,
/// in device pixels, around the first press that a repeated press must stay
/// within (half its width and height either side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoubleClickSettings {
    pub time: Duration,
    pub width: u32,
    pub height: u32,
}

impl DoubleClickSettings {
    /// Chromium's limits on Linux, and Windows' defaults: 500 ms and 4×4.
    pub const DEFAULT: Self = Self {
        time: Duration::from_millis(500),
        width: 4,
        height: 4,
    };
}

impl Default for DoubleClickSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Which of Chromium's click-counting rules applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickCountRule {
    /// Windows: the system's settings, no limit on the count.
    Windows,
    /// Linux (Ozone): fixed limits; the count goes up to 3, then starts
    /// again.
    Linux,
}

impl ClickCountRule {
    /// The rule of the platform Div is built for: Windows on Windows,
    /// Linux elsewhere.
    pub const fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// One mouse input, as the click counter sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickInput {
    pub kind: PointerInputKind,
    /// The button pressed or released; ignored for other kinds.
    pub button: MouseButton,
    /// Whether any button is held after the input.
    pub buttons_held: bool,
    pub modifiers: Modifiers,
    /// The point, in device pixels.
    pub x: f64,
    pub y: f64,
    /// When the input happened, on any clock that only moves forward.
    pub time: Duration,
}

#[derive(Debug, Clone, Copy)]
struct Press {
    button: MouseButton,
    modifiers: Modifiers,
    x: f64,
    y: f64,
    time: Duration,
    count: u32,
}

/// Counts repeated mouse clicks for `detail`, by one of Chromium's rules.
#[derive(Debug, Clone)]
pub struct ClickCounter {
    rule: ClickCountRule,
    settings: DoubleClickSettings,
    /// Windows: the first press of the series, whose point the box is
    /// around. Linux: the previous press.
    anchor: Option<Press>,
    /// Windows: the previous press's time.
    last_time: Duration,
    count: u32,
}

impl ClickCounter {
    pub fn new(rule: ClickCountRule, settings: DoubleClickSettings) -> Self {
        Self {
            rule,
            settings,
            anchor: None,
            last_time: Duration::ZERO,
            count: 0,
        }
    }

    pub fn rule(&self) -> ClickCountRule {
        self.rule
    }

    /// Use new limits from the next input on, as Windows Chromium reads the
    /// system's for every event.
    pub fn set_settings(&mut self, settings: DoubleClickSettings) {
        self.settings = settings;
    }

    /// The click count of `input`: the series' count for a press or a
    /// release, 0 for a move or a leave.
    pub fn count(&mut self, input: ClickInput) -> u32 {
        match self.rule {
            ClickCountRule::Windows => self.count_windows(input),
            ClickCountRule::Linux => self.count_linux(input),
        }
    }

    fn within(&self, anchor: &Press, x: f64, y: f64) -> bool {
        (x - anchor.x).abs() <= f64::from(self.settings.width / 2)
            && (y - anchor.y).abs() <= f64::from(self.settings.height / 2)
    }

    fn count_windows(&mut self, input: ClickInput) -> u32 {
        let continues = |counter: &Self| {
            counter.anchor.as_ref().is_some_and(|anchor| {
                counter.within(anchor, input.x, input.y)
                    && input.time.saturating_sub(counter.last_time) <= counter.settings.time
            })
        };
        match input.kind {
            PointerInputKind::Down => {
                let same_button = self
                    .anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.button == input.button);
                if self.count > 0 && same_button && continues(self) {
                    self.count += 1;
                } else {
                    self.count = 1;
                    self.anchor = Some(Press {
                        button: input.button,
                        modifiers: input.modifiers,
                        x: input.x,
                        y: input.y,
                        time: input.time,
                        count: 1,
                    });
                }
                if let Some(anchor) = self.anchor.as_mut() {
                    anchor.button = input.button;
                }
                self.last_time = input.time;
                self.count
            }
            PointerInputKind::Up => self.count,
            PointerInputKind::Move | PointerInputKind::Leave | PointerInputKind::Cancel => {
                if !input.buttons_held && !continues(self) {
                    self.count = 0;
                    self.anchor = None;
                }
                0
            }
        }
    }

    fn count_linux(&mut self, input: ClickInput) -> u32 {
        match input.kind {
            PointerInputKind::Down => {
                let count = match self.anchor {
                    Some(previous)
                        if input.time != previous.time
                            && input.time.saturating_sub(previous.time) <= self.settings.time
                            && self.within(&previous, input.x, input.y)
                            && previous.button == input.button
                            && previous.modifiers == input.modifiers
                            // A triple click's flags differ from a press's,
                            // so the press after it starts again.
                            && previous.count < 3 =>
                    {
                        previous.count + 1
                    }
                    _ => 1,
                };
                self.anchor = Some(Press {
                    button: input.button,
                    modifiers: input.modifiers,
                    x: input.x,
                    y: input.y,
                    time: input.time,
                    count,
                });
                count
            }
            PointerInputKind::Up => match self.anchor {
                Some(previous) if previous.button == input.button => previous.count,
                _ => 1,
            },
            PointerInputKind::Move | PointerInputKind::Leave | PointerInputKind::Cancel => 0,
        }
    }
}

/// The touch tap limits of Chromium's gesture detector.
const TAP_REPEAT_TIME: Duration = Duration::from_millis(400);
/// The distance between the presses of repeated taps must be less than
/// this, in CSS pixels.
const TAP_REPEAT_SLOP: f64 = 20.0;
const TAP_REPEAT_INTERVAL: u32 = 3;

/// Counts repeated touch taps, as Chromium's gesture detector does
/// (`GestureDetector::IsRepeatedTap`, with the single-tap repeat interval
/// of 3): a tap whose press comes within 400 ms of the previous tap's
/// release, less than 20 CSS pixels from its press, counts on; the count
/// goes 1, 2, 3, then starts again.
#[derive(Debug, Clone, Default)]
pub struct TapCounter {
    previous: Option<Tap>,
}

#[derive(Debug, Clone, Copy)]
struct Tap {
    down: (f64, f64),
    up_time: Duration,
    count: u32,
}

impl TapCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The count of a tap pressed at `down` (CSS pixels) at `down_time`
    /// and released at `up_time`.
    pub fn tap(&mut self, down: (f64, f64), down_time: Duration, up_time: Duration) -> u32 {
        let count = match self.previous {
            Some(previous)
                if down_time.saturating_sub(previous.up_time) <= TAP_REPEAT_TIME
                    && (down.0 - previous.down.0).hypot(down.1 - previous.down.1)
                        < TAP_REPEAT_SLOP =>
            {
                previous.count % TAP_REPEAT_INTERVAL + 1
            }
            _ => 1,
        };
        self.previous = Some(Tap {
            down,
            up_time,
            count,
        });
        count
    }

    /// Forget the previous tap, as a touch that moved beyond the tap slop
    /// does.
    pub fn reset(&mut self) {
        self.previous = None;
    }
}
