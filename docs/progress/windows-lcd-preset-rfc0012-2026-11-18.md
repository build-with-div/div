# Windows text raster preset and oracle pin (RFC 0012, engine part), 18 November 2026

RFC 0012 ([build-with-div/div#90](https://github.com/build-with-div/div/pull/90)) makes Chromium's Windows text raster the Windows app default. Its
engine part, in this change, adds the preset and the oracle pin and changes
no default. The framework PR that adopts the preset follows this one.

## What changes

- **`RasterConfiguration::chromium_windows_lcd()`** in `div-geometry`
  (`crates/div-geometry/src/raster.rs`), next to `chromium_linux_lcd()`:
  - author, native and embedded text use `TextRasterConfiguration::chromium_lcd()`;
  - pixel geometry is RGB horizontal;
  - `gamma_milli` is 0, which selects Skia's sRGB curve, and
    `contrast_milli` is 1000;
  - the backend is the software Skia one, as in the measurement below.

  It reads nothing from the system. Nothing in the workspace calls it yet,
  and `RasterConfiguration::default()` is unchanged.
- **The oracle pin** in `tools/qualification/windows_chrome.py`:
  `verify_chrome` now also refuses to go on unless the system matches the
  setup the Windows oracles were captured under:
  - font smoothing on (`SPI_GETFONTSMOOTHING`), with type ClearType
    (`SPI_GETFONTSMOOTHINGTYPE`);
  - RGB subpixel order (`SPI_GETFONTSMOOTHINGORIENTATION`);
  - no per-display key `HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<primary display>`.

  It names every setting that differs. Every Windows capture tool goes
  through `verify_chrome`, including the three that launch Chrome
  themselves (`capture_click_count_oracle.py`,
  `capture_keyboard_scroll_oracle.py` and `capture_wheel_oracle.py`).

## Sources

- **The preset's values** come from the fidelity lane's measurement of
  18 November 2026, recorded in RFC 0012
  ([build-with-div/div#90](https://github.com/build-with-div/div/pull/90),
  `docs/alpha/rfcs/0012-windows-text-raster.md`).
  - Setup: the six web-font lines of `webfont-fontations-v1`, rendered by
    Div on Windows with LCD text, RGB order, sRGB gamma and contrast 1.0,
    against pinned Windows Chrome 147.0.7727.50.
  - Result: 0 differing pixels at 1x. At 2x, 0 differ after the one-pixel
    row shift that RFC 0011 owns.
  - The preset is that configuration: `RasterConfiguration::default()`
    with LCD author text, RGB geometry, gamma 0 and contrast 1000.
- **The pin's settings** are the ones Chromium 147.0.7727.50 reads (BSD
  licensed, read from `out\chromium-ref`):
  - `ui/gfx/font_render_params_win.cc`: antialiasing from
    `SPI_GETFONTSMOOTHING`, LCD when `SPI_GETFONTSMOOTHINGTYPE` is
    ClearType, and the order from `PixelStructure` in the per-display key or
    else `SPI_GETFONTSMOOTHINGORIENTATION`.
  - `ui/gfx/font_util_win.cc`: the per-display key is the base name of the
    first `EnumDisplayDevices` entry flagged as primary. Only when that key
    exists does Chromium take DirectWrite's gamma and enhanced contrast.

  The pin finds the primary display the same way.

## Verification

| Check | Result |
| --- | --- |
| `cargo test -p div-geometry` (Windows) | 103 passed, 0 failed |
| `python -m unittest tools.qualification.test_windows_chrome tools.qualification.test_compare_windows_text tools.qualification.test_scrollbar_probe_tools` | 27 passed on Windows and on Linux (WSL, Python 3.14) |
| The pin on the oracle machine | ClearType on, RGB order, primary display `DISPLAY1`, no per-display key: no failures. `verify_chrome` accepts the pinned `chrome.exe`. |
| Generator checks, `cargo fmt --check`, `fidelity_data.py verify` | all pass |

- **New unit tests:**
  - `windows_lcd_profile_matches_chromium_windows_defaults` in
    `raster.rs`.
  - `TextRasterPinTests` in `test_windows_chrome.py`. With the system reads
    injected, these cover: the pinned setup; smoothing off; standard
    smoothing; BGR order; a per-display key; a key for a display other than
    the primary one, which doesn't count; settings that could not be read;
    and every failure named in one message.
- **Census and CSS Grid coverage:** not run. Nothing the census or the
  coverage builds calls the new constructor, and no default changes, so
  neither can change (D4).

## Not done here

- **The app default:** the framework PR adds
  `div_platform::system_raster_configuration()` and makes Windows apps use
  it.
- **The native test:** the fidelity lane's web-font fixture and its native
  test, using this preset, come in the fidelity PR.
- **Other Windows setups:** ClearType tuning (the per-display key),
  standard smoothing and BGR order are not measured. The pin refuses to
  capture under them.
