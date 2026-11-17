//! The platform's double-click limits, as Chromium 147 reads them for click
//! counting (`detail`).

/// The time between presses, in milliseconds, and the box, in device
/// pixels, that a repeated press must stay within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoubleClickLimits {
    pub time_ms: u32,
    pub width: u32,
    pub height: u32,
}

impl DoubleClickLimits {
    /// Chromium's limits on Linux (`ui::kDoubleClickTimeMs` and the 4×4 box
    /// of `MouseEvent::IsRepeatedClickEvent` in `ui/events/event.cc`), which
    /// are also Windows' defaults.
    pub const CHROMIUM_LINUX: Self = Self {
        time_ms: 500,
        width: 4,
        height: 4,
    };

    /// This system's limits. On Windows they are read on every call
    /// (`GetDoubleClickTime`, `SM_CXDOUBLECLK`, `SM_CYDOUBLECLK`), as
    /// Chromium reads them for every mouse message
    /// (`ui/events/blink/web_input_event_builders_win.cc`), so a change in
    /// Settings applies at once. Elsewhere they are Chromium's fixed Linux
    /// limits: Chromium on Linux does not read the desktop's settings.
    pub fn system() -> Self {
        #[cfg(all(feature = "windows", target_os = "windows"))]
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
            };
            // SAFETY: neither function takes pointers; both only read
            // system settings.
            let (time_ms, width, height) = unsafe {
                (
                    GetDoubleClickTime(),
                    GetSystemMetrics(SM_CXDOUBLECLK),
                    GetSystemMetrics(SM_CYDOUBLECLK),
                )
            };
            Self {
                time_ms,
                width: u32::try_from(width).unwrap_or(0),
                height: u32::try_from(height).unwrap_or(0),
            }
        }
        #[cfg(not(all(feature = "windows", target_os = "windows")))]
        {
            Self::CHROMIUM_LINUX
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_limits_are_chromium_linux_off_windows() {
        let limits = DoubleClickLimits::system();
        if cfg!(all(feature = "windows", target_os = "windows")) {
            // Windows clamps the time to 5000 ms and reports a positive box.
            assert!(limits.time_ms > 0 && limits.time_ms <= 5000, "{limits:?}");
            assert!(limits.width > 0 && limits.height > 0, "{limits:?}");
        } else {
            assert_eq!(limits, DoubleClickLimits::CHROMIUM_LINUX);
        }
    }
}
