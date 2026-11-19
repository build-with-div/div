//! The system's text raster settings, as Windows Chromium 147 reads them
//! (RFC 0012).
//!
//! Sources, BSD-licensed, in 147.0.7727.50:
//! - `ui/gfx/font_render_params_win.cc`: `CachedFontRenderParams::GetParams`
//!   and `GetSubpixelRenderingGeometry`;
//! - `ui/gfx/font_util_win.cc`: `GetTextParameters`,
//!   `GetTextSettingsRegistryKey`, `ClampContrast` and `ClampGamma`.

// Off Windows only the tests use the mapping.
#![cfg_attr(not(all(feature = "windows", target_os = "windows")), allow(dead_code))]

use div_geometry::{
    RasterBackend, RasterConfiguration, RasterPixelGeometry, TextEdging, TextRasterConfiguration,
};

/// Subpixel order, from `GetSubpixelRenderingGeometry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubpixelOrder {
    None,
    Rgb,
    Bgr,
}

/// What Chromium reads on Windows to choose its text raster.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WindowsTextSettings {
    /// `SPI_GETFONTSMOOTHING`.
    pub(crate) smoothing: bool,
    /// `SPI_GETFONTSMOOTHINGTYPE` is `FE_FONTSMOOTHINGCLEARTYPE`.
    pub(crate) cleartype: bool,
    /// The primary display's `PixelStructure`, else the smoothing
    /// orientation, else none.
    pub(crate) order: SubpixelOrder,
    /// DirectWrite's rendering parameters `(gamma, enhanced contrast)`,
    /// read only when the primary display's `Avalon.Graphics` key exists.
    pub(crate) tuned: Option<(f32, f32)>,
}

/// Skia's `SkSurfaceProps` ranges, which `FontUtilWin` clamps to.
const MAX_CONTRAST: f32 = 1.0;
const MAX_GAMMA_EXCLUSIVE: f32 = 4.0;

/// The raster configuration Chromium uses for `settings`.
///
/// - Smoothing off: aliased text without subpixel positioning.
/// - Smoothing on: antialiased text with subpixel positioning, LCD when
///   ClearType is on and the order is RGB or BGR, else grayscale.
/// - Gamma and contrast: Chromium's Windows build defaults, the sRGB curve
///   (`SK_GAMMA_SRGB`, `gamma_milli` 0) and contrast 1.0, unless the
///   primary display has a ClearType tuning key; then DirectWrite's gamma
///   and enhanced contrast, clamped to Skia's ranges.
///
/// Only ClearType with RGB order and no tuning key is measured (RFC 0012):
/// it is exactly [`RasterConfiguration::chromium_windows_lcd`].
pub(crate) fn windows_raster_configuration(settings: WindowsTextSettings) -> RasterConfiguration {
    let lcd = TextRasterConfiguration::chromium_lcd();
    let (text, pixel_geometry) = if !settings.smoothing {
        (
            TextRasterConfiguration {
                edging: TextEdging::Alias,
                subpixel_positioning: false,
                ..lcd
            },
            RasterPixelGeometry::Unknown,
        )
    } else {
        match (settings.cleartype, settings.order) {
            (true, SubpixelOrder::Rgb) => (lcd, RasterPixelGeometry::RgbHorizontal),
            (true, SubpixelOrder::Bgr) => (lcd, RasterPixelGeometry::BgrHorizontal),
            _ => (
                TextRasterConfiguration {
                    edging: TextEdging::AntiAlias,
                    ..lcd
                },
                RasterPixelGeometry::Unknown,
            ),
        }
    };
    let (gamma_milli, contrast_milli) = match settings.tuned {
        None => (0, 1000),
        Some((gamma, contrast)) => {
            let gamma = if gamma.is_nan() { 0.0 } else { gamma };
            let contrast = if contrast.is_nan() { 0.0 } else { contrast };
            let gamma = gamma.clamp(0.0, MAX_GAMMA_EXCLUSIVE - f32::EPSILON);
            let contrast = contrast.clamp(0.0, MAX_CONTRAST);
            // Thousandths, kept below Skia's exclusive gamma maximum.
            (
                ((gamma * 1000.0).round() as u16).min(3999),
                (contrast * 1000.0).round() as u16,
            )
        }
    };
    RasterConfiguration {
        backend: RasterBackend::Skia,
        author_text: text,
        native_text: text,
        embedded_text: text,
        pixel_geometry,
        gamma_milli,
        contrast_milli,
    }
}

/// The text raster configuration Chromium would use on this machine, for
/// windowed documents (RFC 0012). Windows reads the settings once per
/// process; Chromium too keeps them while it runs. Other platforms, and
/// Windows builds without the `windows` feature, return `None`.
pub fn system_raster_configuration() -> Option<RasterConfiguration> {
    #[cfg(all(feature = "windows", target_os = "windows"))]
    {
        static SYSTEM: std::sync::OnceLock<RasterConfiguration> = std::sync::OnceLock::new();
        Some(*SYSTEM.get_or_init(|| windows_raster_configuration(read::settings())))
    }
    #[cfg(not(all(feature = "windows", target_os = "windows")))]
    {
        None
    }
}

#[cfg(all(feature = "windows", target_os = "windows"))]
mod read {
    use super::{SubpixelOrder, WindowsTextSettings};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE,
    };
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        RRF_RT_REG_DWORD,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGORIENTATIONBGR,
        FE_FONTSMOOTHINGORIENTATIONRGB, SPI_GETFONTSMOOTHING, SPI_GETFONTSMOOTHINGORIENTATION,
        SPI_GETFONTSMOOTHINGTYPE,
    };

    /// One UINT or BOOL system parameter, or `None` when the call fails.
    fn parameter(action: u32) -> Option<u32> {
        let mut value = 0u32;
        // SAFETY: these actions write one UINT (or BOOL, also 32 bits) to
        // the pointer, which is valid for the call.
        let ok = unsafe { SystemParametersInfoW(action, 0, (&raw mut value).cast(), 0) };
        (ok != 0).then_some(value)
    }

    /// `HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<primary display>`, opened
    /// for reading: `FontUtilWin::GetTextSettingsRegistryKey`.
    struct TextSettingsKey(HKEY);

    impl TextSettingsKey {
        fn open() -> Option<Self> {
            let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
            let mut index = 0;
            // SAFETY: `device` is a DISPLAY_DEVICEW whose `cb` holds its size.
            while unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) } != 0 {
                index += 1;
                if device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE == 0 {
                    continue;
                }
                let length = device
                    .DeviceName
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(device.DeviceName.len());
                let name = String::from_utf16_lossy(&device.DeviceName[..length]);
                // `FilePath::BaseName`: `\\.\DISPLAY1` becomes `DISPLAY1`.
                let base = name.rsplit(['\\', '/']).next().unwrap_or_default();
                let path: Vec<u16> = format!(r"SOFTWARE\Microsoft\Avalon.Graphics\{base}")
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let mut key: HKEY = std::ptr::null_mut();
                // SAFETY: the path is NUL-terminated and `key` receives the
                // opened handle, which `Drop` closes.
                let status = unsafe {
                    RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key)
                };
                return (status == ERROR_SUCCESS).then_some(Self(key));
            }
            None
        }

        fn dword(&self, name: &str) -> Option<u32> {
            let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut value = 0u32;
            let mut size = std::mem::size_of::<u32>() as u32;
            // SAFETY: the key is open, the name is NUL-terminated and the
            // output is a u32 whose size is passed in `size`.
            let status = unsafe {
                RegGetValueW(
                    self.0,
                    std::ptr::null(),
                    name.as_ptr(),
                    RRF_RT_REG_DWORD,
                    std::ptr::null_mut(),
                    (&raw mut value).cast(),
                    &mut size,
                )
            };
            (status == ERROR_SUCCESS).then_some(value)
        }
    }

    impl Drop for TextSettingsKey {
        fn drop(&mut self) {
            // SAFETY: the handle came from RegOpenKeyExW and is closed once.
            unsafe { RegCloseKey(self.0) };
        }
    }

    /// DirectWrite's default rendering parameters for the primary display:
    /// `(gamma, enhanced contrast)`.
    fn direct_write_parameters() -> Option<(f32, f32)> {
        use windows::Win32::Graphics::DirectWrite::{
            DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
        };
        // SAFETY: plain COM calls on interfaces the calls return; the
        // shared factory needs no COM initialization.
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let parameters = factory.CreateRenderingParams().ok()?;
            Some((parameters.GetGamma(), parameters.GetEnhancedContrast()))
        }
    }

    pub(super) fn settings() -> WindowsTextSettings {
        let smoothing = parameter(SPI_GETFONTSMOOTHING).is_some_and(|enabled| enabled != 0);
        let cleartype =
            smoothing && parameter(SPI_GETFONTSMOOTHINGTYPE) == Some(FE_FONTSMOOTHINGCLEARTYPE);
        let key = TextSettingsKey::open();
        let order = match key.as_ref().and_then(|key| key.dword("PixelStructure")) {
            Some(1) => SubpixelOrder::Rgb,
            Some(2) => SubpixelOrder::Bgr,
            // 0, and Chromium's fallback for other values.
            Some(_) => SubpixelOrder::None,
            None => match parameter(SPI_GETFONTSMOOTHINGORIENTATION) {
                Some(FE_FONTSMOOTHINGORIENTATIONRGB) => SubpixelOrder::Rgb,
                Some(FE_FONTSMOOTHINGORIENTATIONBGR) => SubpixelOrder::Bgr,
                _ => SubpixelOrder::None,
            },
        };
        // Chromium applies DirectWrite's values only when the key exists.
        let tuned = key.is_some().then(direct_write_parameters).flatten();
        WindowsTextSettings {
            smoothing,
            cleartype,
            order,
            tuned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(
        smoothing: bool,
        cleartype: bool,
        order: SubpixelOrder,
        tuned: Option<(f32, f32)>,
    ) -> WindowsTextSettings {
        WindowsTextSettings {
            smoothing,
            cleartype,
            order,
            tuned,
        }
    }

    #[test]
    fn cleartype_rgb_without_tuning_is_the_measured_preset() {
        assert_eq!(
            windows_raster_configuration(settings(true, true, SubpixelOrder::Rgb, None)),
            RasterConfiguration::chromium_windows_lcd()
        );
    }

    #[test]
    fn bgr_order_keeps_lcd_text_and_reverses_the_geometry() {
        let bgr = windows_raster_configuration(settings(true, true, SubpixelOrder::Bgr, None));
        assert_eq!(bgr.pixel_geometry, RasterPixelGeometry::BgrHorizontal);
        assert_eq!(bgr.author_text, TextRasterConfiguration::chromium_lcd());
    }

    #[test]
    fn standard_smoothing_or_no_order_is_grayscale() {
        for config in [
            settings(true, false, SubpixelOrder::Rgb, None),
            settings(true, true, SubpixelOrder::None, None),
        ] {
            let raster = windows_raster_configuration(config);
            assert_eq!(raster.author_text.edging, TextEdging::AntiAlias);
            assert!(raster.author_text.subpixel_positioning);
            assert_eq!(raster.pixel_geometry, RasterPixelGeometry::Unknown);
            assert_eq!((raster.gamma_milli, raster.contrast_milli), (0, 1000));
        }
    }

    #[test]
    fn smoothing_off_is_aliased_without_subpixel_positioning() {
        let raster = windows_raster_configuration(settings(false, true, SubpixelOrder::Rgb, None));
        for text in [raster.author_text, raster.native_text, raster.embedded_text] {
            assert_eq!(text.edging, TextEdging::Alias);
            assert!(!text.subpixel_positioning);
        }
    }

    #[test]
    fn a_tuning_key_takes_direct_write_values_clamped_to_skia() {
        let tuned = |gamma, contrast| {
            let raster = windows_raster_configuration(settings(
                true,
                true,
                SubpixelOrder::Rgb,
                Some((gamma, contrast)),
            ));
            (raster.gamma_milli, raster.contrast_milli)
        };
        assert_eq!(tuned(1.8, 0.5), (1800, 500));
        assert_eq!(tuned(2.2, 1.0), (2200, 1000));
        assert_eq!(tuned(9.0, 4.0), (3999, 1000));
        assert_eq!(tuned(-1.0, -1.0), (0, 0));
        assert_eq!(tuned(f32::NAN, f32::NAN), (0, 0));
    }

    #[test]
    fn other_platforms_have_no_system_configuration() {
        if !cfg!(all(feature = "windows", target_os = "windows")) {
            assert_eq!(system_raster_configuration(), None);
        }
    }
}
