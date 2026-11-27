//! The operating system's appearance preferences that media queries read:
//! `prefers-color-scheme` and `prefers-reduced-motion`.
//!
//! [`SystemPreferences::system`] reads them now, so an app can start with
//! them before any view exists. The desktop runtime reports later changes as
//! [`PlatformEvent::SystemPreferencesChanged`](crate::PlatformEvent::SystemPreferencesChanged).

/// The user's appearance preferences, as the operating system reports them.
///
/// Windows reports both: `dark` when apps use the dark theme ("Choose your
/// app mode" in Settings), and `reduced_motion` when "Animation effects" is
/// off. Other platforms report the defaults, a light scheme with no motion
/// preference, until their desktop settings are read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SystemPreferences {
    /// Apps should use a dark color scheme: `prefers-color-scheme: dark`.
    pub dark: bool,
    /// The user asked for less motion: `prefers-reduced-motion: reduce`.
    pub reduced_motion: bool,
}

impl SystemPreferences {
    /// The operating system's preferences now.
    pub fn system() -> Self {
        #[cfg(all(feature = "windows", target_os = "windows"))]
        {
            windows::read()
        }
        #[cfg(not(all(feature = "windows", target_os = "windows")))]
        {
            Self::default()
        }
    }
}

#[cfg(all(feature = "windows", target_os = "windows"))]
pub(crate) mod windows {
    use super::SystemPreferences;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, WM_NCDESTROY, WM_SETTINGCHANGE,
    };

    pub(crate) fn read() -> SystemPreferences {
        SystemPreferences {
            dark: apps_use_light_theme() == Some(0),
            reduced_motion: client_area_animation() == Some(false),
        }
    }

    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`
    /// `AppsUseLightTheme`: 0 when apps use the dark theme. Missing on
    /// systems that predate the setting, which are light.
    fn apps_use_light_theme() -> Option<u32> {
        let wide = |text: &str| {
            text.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        };
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("AppsUseLightTheme");
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the key and value names are NUL-terminated UTF-16 buffers
        // and the output buffer is a u32 whose size is passed in `size`.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(value)
    }

    /// `SPI_GETCLIENTAREAANIMATION`: false when "Animation effects" is off.
    fn client_area_animation() -> Option<bool> {
        let mut enabled = 0i32;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to `enabled`.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut enabled as *mut i32).cast(),
                0,
            )
        };
        (ok != 0).then_some(enabled != 0)
    }

    /// The subclass id of the settings hook, unique to this crate.
    const SUBCLASS_ID: usize = 0x6469_7670; // "divp"

    /// Call `notify` on the window's thread whenever Windows broadcasts
    /// `WM_SETTINGCHANGE` to it, which it does for theme and animation
    /// changes alike; winit does not forward the message. The hook removes
    /// itself, and drops `notify`, when the window is destroyed.
    pub(crate) fn watch_settings(hwnd: HWND, notify: Box<dyn Fn()>) -> bool {
        let data = Box::into_raw(Box::new(notify));
        // SAFETY: `hwnd` is a live window of this thread. The subclass owns
        // `data` until `settings_proc` frees it at WM_NCDESTROY.
        let installed =
            unsafe { SetWindowSubclass(hwnd, Some(settings_proc), SUBCLASS_ID, data as usize) };
        if installed == 0 {
            // SAFETY: the subclass was not installed, so `data` is still ours.
            drop(unsafe { Box::from_raw(data) });
        }
        installed != 0
    }

    unsafe extern "system" fn settings_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        let notify = data as *mut Box<dyn Fn()>;
        match message {
            WM_SETTINGCHANGE => {
                // SAFETY: `data` is the live box `watch_settings` installed.
                unsafe { (*notify)() };
            }
            WM_NCDESTROY => {
                // SAFETY: the window's last message: remove the hook, then
                // free the box it owned.
                unsafe {
                    RemoveWindowSubclass(hwnd, Some(settings_proc), SUBCLASS_ID);
                    drop(Box::from_raw(notify));
                }
            }
            _ => {}
        }
        // SAFETY: forwards the message to the next procedure in the chain.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    #[cfg(test)]
    mod tests {
        use std::cell::Cell;
        use std::rc::Rc;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, HWND_MESSAGE, WM_SETTINGCHANGE,
        };

        #[test]
        fn the_system_is_read_without_failing() {
            // The values depend on this machine; reading must not panic and
            // must agree with itself.
            assert_eq!(super::read(), super::read());
        }

        #[test]
        fn the_settings_hook_notifies_and_is_freed_with_its_window() {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            // SAFETY: a message-only window of the predefined STATIC class,
            // owned by this thread and destroyed below.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            let count = Rc::new(Cell::new(0));
            let seen = count.clone();
            assert!(super::watch_settings(
                hwnd,
                Box::new(move || seen.set(seen.get() + 1))
            ));
            // SAFETY: `hwnd` is live and belongs to this thread.
            unsafe { SendMessageW(hwnd, WM_SETTINGCHANGE, 0, 0) };
            assert_eq!(count.get(), 1);
            assert_eq!(Rc::strong_count(&count), 2);
            // SAFETY: as above; WM_NCDESTROY removes the hook and frees it.
            assert_ne!(unsafe { DestroyWindow(hwnd) }, 0);
            assert_eq!(Rc::strong_count(&count), 1, "the hook's box was freed");
        }
    }
}
