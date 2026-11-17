//! Wheel deltas as Chromium 147 reports them to the web.
//!
//! winit reports a wheel in lines (notches), positive when the content
//! should move right or down, that is when the user scrolls left or up. The
//! DOM's `WheelEvent` deltas are the opposite: positive scrolls right or
//! down. [`WheelSettings::deltas`] converts one to the other with the
//! platform's pixels per notch. The oracle is
//! `tests/fidelity/fixtures/wheel-events-v1`, captured from real platform
//! wheel input by `tools/qualification/capture_wheel_oracle.py`.

/// The unit of a wheel event's deltas, as the DOM's `WheelEvent.deltaMode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WheelDeltaMode {
    /// `DOM_DELTA_PIXEL` (0): CSS pixels. Every native mouse wheel and
    /// touchpad on Windows and Linux.
    #[default]
    Pixel = 0,
    /// `DOM_DELTA_LINE` (1): lines.
    Line = 1,
    /// `DOM_DELTA_PAGE` (2): pages. Windows' "one screen at a time" wheel
    /// setting.
    Page = 2,
}

/// A wheel event's deltas in the DOM's direction and unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelDeltas {
    pub delta_x: f32,
    pub delta_y: f32,
    pub mode: WheelDeltaMode,
}

impl WheelDeltas {
    /// Deltas from a precise (touchpad) source in CSS pixels with winit's
    /// direction: positive moves the content right or down.
    pub fn from_content_pixels(x: f32, y: f32) -> Self {
        Self {
            delta_x: flip(x, 1.0),
            delta_y: flip(y, 1.0),
            mode: WheelDeltaMode::Pixel,
        }
    }
}

/// How a platform turns wheel notches into web deltas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelSettings {
    pixels_per_line_x: f32,
    pixels_per_line_y: f32,
    vertical_pages: bool,
}

/// Windows' `WHEEL_PAGESCROLL`: the wheel scrolls one page per notch.
pub const WHEEL_PAGESCROLL: u32 = u32::MAX;

impl WheelSettings {
    /// Chromium on Windows (`WebMouseWheelEventBuilder::Build` in
    /// `ui/events/blink/web_input_event_builders_win.cc`): a vertical notch
    /// scrolls `SPI_GETWHEELSCROLLLINES` lines and a horizontal one
    /// `SPI_GETWHEELSCROLLCHARS` characters, each 100/3 pixels, so the
    /// default 3 lines is 100 pixels. [`WHEEL_PAGESCROLL`] lines make a
    /// vertical notch one page.
    pub fn windows(scroll_lines: u32, scroll_chars: u32) -> Self {
        const PIXELS_PER_LINE: f32 = 100.0 / 3.0;
        let vertical_pages = scroll_lines == WHEEL_PAGESCROLL;
        Self {
            pixels_per_line_x: scroll_chars as f32 * PIXELS_PER_LINE,
            pixels_per_line_y: if vertical_pages {
                1.0
            } else {
                scroll_lines as f32 * PIXELS_PER_LINE
            },
            vertical_pages,
        }
    }

    /// Chromium on Linux: 120 pixels per notch on both axes
    /// (`kWheelScrollAmount` in `ui/events/x/events_x_utils.cc`, and
    /// `MouseWheelEvent::kWheelDelta` in `ui/events/event.cc`).
    pub fn linux() -> Self {
        Self {
            pixels_per_line_x: 120.0,
            pixels_per_line_y: 120.0,
            vertical_pages: false,
        }
    }

    /// This system's settings. On Windows they are read on every call, as
    /// Chromium reads them for every wheel message, so a change in Settings
    /// applies at once; Chromium's defaults (3 lines, 1 character) stand in
    /// when they cannot be read.
    pub fn system() -> Self {
        #[cfg(all(feature = "windows", target_os = "windows"))]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SystemParametersInfoW, SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES,
            };
            let read = |action, default: u32| {
                let mut value = default;
                // SAFETY: both actions write one UINT to the pointer, which
                // is valid for the call.
                let ok = unsafe { SystemParametersInfoW(action, 0, (&raw mut value).cast(), 0) };
                if ok != 0 {
                    value
                } else {
                    default
                }
            };
            Self::windows(
                read(SPI_GETWHEELSCROLLLINES, 3),
                read(SPI_GETWHEELSCROLLCHARS, 1),
            )
        }
        #[cfg(all(
            not(all(feature = "windows", target_os = "windows")),
            target_os = "windows"
        ))]
        {
            Self::windows(3, 1)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::linux()
        }
    }

    /// The web deltas for winit's line deltas (`MouseScrollDelta::LineDelta`,
    /// positive when the user scrolls left or up). A Windows notch in page
    /// mode is one page, in [`WheelDeltaMode::Page`].
    pub fn deltas(self, lines_x: f32, lines_y: f32) -> WheelDeltas {
        if self.vertical_pages && lines_y != 0.0 {
            return WheelDeltas {
                delta_x: 0.0,
                delta_y: flip(lines_y, 1.0),
                mode: WheelDeltaMode::Page,
            };
        }
        WheelDeltas {
            delta_x: flip(lines_x, self.pixels_per_line_x),
            delta_y: flip(lines_y, self.pixels_per_line_y),
            mode: WheelDeltaMode::Pixel,
        }
    }
}

fn flip(value: f32, scale: f32) -> f32 {
    if value == 0.0 {
        0.0
    } else {
        -value * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_notches_follow_the_system_settings() {
        let default = WheelSettings::windows(3, 3);
        // winit: WM_MOUSEWHEEL -120 (towards the user) is LineDelta(0, -1).
        assert_eq!(
            default.deltas(0.0, -1.0),
            WheelDeltas {
                delta_x: 0.0,
                delta_y: 100.0,
                mode: WheelDeltaMode::Pixel
            }
        );
        // WM_MOUSEHWHEEL +120 (tilt right) is LineDelta(-1, 0).
        assert_eq!(default.deltas(-1.0, 0.0).delta_x, 100.0);
        assert_eq!(
            WheelSettings::windows(6, 1).deltas(-1.0, -0.5).delta_y,
            100.0
        );
        assert_eq!(
            WheelSettings::windows(6, 1).deltas(-1.0, 0.0).delta_x,
            100.0 / 3.0
        );
        assert_eq!(
            WheelSettings::windows(WHEEL_PAGESCROLL, 3).deltas(0.0, 2.0),
            WheelDeltas {
                delta_x: 0.0,
                delta_y: -2.0,
                mode: WheelDeltaMode::Page
            }
        );
    }

    #[test]
    fn linux_notches_are_120_pixels() {
        // winit X11: button 5 is LineDelta(0, -1), button 6 LineDelta(1, 0).
        assert_eq!(WheelSettings::linux().deltas(0.0, -1.0).delta_y, 120.0);
        assert_eq!(WheelSettings::linux().deltas(1.0, 0.0).delta_x, -120.0);
    }

    #[test]
    fn content_pixels_flip_to_the_web_direction() {
        assert_eq!(
            WheelDeltas::from_content_pixels(3.0, -4.5),
            WheelDeltas {
                delta_x: -3.0,
                delta_y: 4.5,
                mode: WheelDeltaMode::Pixel
            }
        );
    }
}
