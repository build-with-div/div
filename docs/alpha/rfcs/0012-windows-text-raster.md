# RFC 0012: Chromium's text raster as the Windows default

**Status:** Approved by the user on 18 November 2026, as drafted.
- The framework track reviewed it the same day; its changes are folded in.
- Implementation follows the Migration order below.

**Owner:** the fidelity lane.

**Consumers:**
- the framework track, which owns what an app gets by default;
- the engine track, which owns `div-geometry`'s raster presets and the
  oracle tooling;
- the components track, the SVG and canvas lanes, and the samples, which
  render text on Windows.

## Problem

On Windows, Chromium 147 draws web text with LCD (ClearType) subpixel
anti-aliasing. A Div app on Windows draws it in grayscale: `App` uses
`EngineOptions::default()`, whose `RasterConfiguration::default()` has
grayscale author text, unknown pixel geometry, gamma 1.0 and contrast 0.

The difference was measured on 18 November 2026:
- **Page:** the six web-font lines of `webfont-fontations-v1` (Geist Mono,
  Inter and Geist 650 at 13px and 16px).
- **Div:** rendered through the public API on Windows.
- **Chromium:** pinned Windows CfT 147.0.7727.50, at 1x and real-zoom 2x.
- **Details:** `docs/progress/tracks/fidelity.md`.

| Div raster configuration | 1x differing px | 2x differing px (after the row shift) |
| --- | --- | --- |
| `RasterConfiguration::default()` (today's app default) | 11,750, by up to 154 | 25,053, by up to 154 |
| LCD, gamma 1.2, contrast 0.2 (Div's Linux tone) | 9,874, by up to 16 | 21,271, by up to 16 |
| LCD, sRGB gamma, contrast 1.0 | **0** | **0** |

The 2x row shift is the engine's line-box offset of one device pixel,
owned by RFC 0011. It is not part of this RFC.

Chromium's choice on Windows comes from the system, not from constants
alone. In 147.0.7727.50:
- **Antialiasing and LCD** (`ui/gfx/font_render_params_win.cc`):
  - text is antialiased when `SPI_GETFONTSMOOTHING` is on;
  - it is LCD when `SPI_GETFONTSMOOTHINGTYPE` is ClearType;
  - the subpixel order is `PixelStructure` under
    `HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<primary display>`, or else
    `SPI_GETFONTSMOOTHINGORIENTATION`, or else none.
- **Gamma and contrast:**
  - Gamma and contrast come from `FontUtilWin` (`ui/gfx/font_util_win.cc`,
    feature `kUseGammaContrastRegistrySettings`, enabled by default).
  - It applies DirectWrite's `IDWriteRenderingParams` gamma and enhanced
    contrast, clamped to Skia's ranges, only when that per-display registry
    key exists. The ClearType tuner normally writes it.
  - Otherwise Chromium keeps its Windows build defines (`skia/BUILD.gn`):
    `SK_GAMMA_SRGB` (the sRGB curve) and `SK_GAMMA_CONTRAST=1.0`.

The oracle machine has ClearType on, RGB order and no per-display key, so
its Chromium uses the build defines. That is the configuration measured
exact above.

**Why only Windows:**
- Linux Chromium takes its text settings per font from Fontconfig.
- Div's `chromium_linux_lcd()` reproduces the census's real-font Fontconfig
  profile, not a desktop's.
- A Linux default would need its own measurement of how desktops configure
  Fontconfig, so it is left to a separate RFC.

## Proposal

1. **A Windows preset** in `div-geometry` (`raster.rs`):
   `RasterConfiguration::chromium_windows_lcd()`.
   - Author, native and embedded text use `TextRasterConfiguration::chromium_lcd()`.
   - Pixel geometry is RGB horizontal.
   - `gamma_milli` is 0 (sRGB) and `contrast_milli` is 1000.

   It is deterministic and reads nothing from the system. Tests and oracles
   use it.
2. **The system read** is a free function in `div-platform`, next to
   `system_accent_color()` and `WheelSettings::system()`:
   `div_platform::system_raster_configuration() -> Option<RasterConfiguration>`.
   - It returns `None` off Windows.
   - It reads what Chromium reads, once per process (`OnceLock`):
     - font smoothing off gives aliased text;
     - standard smoothing gives grayscale with sRGB gamma and contrast 1.0;
     - ClearType gives the preset above, with the order from the primary
       display's `PixelStructure` or the orientation setting (none means
       grayscale);
     - gamma and contrast follow Chromium's rule:
       - **No per-display `Avalon.Graphics` key** (the usual case): sRGB
         gamma (`gamma_milli` 0) and contrast 1.0, Chromium's Windows build
         defaults. This is the measured, exact configuration.
       - **The key exists** (a ClearType-tuned machine): DirectWrite's
         `IDWriteRenderingParams` gamma and enhanced contrast, clamped like
         Chromium. This path is unmeasured, and Div does not claim to match
         Chromium on such a machine until it is measured.
   - It needs the `windows` crate's `Win32_Graphics_DirectWrite` feature in
     `div-platform` only; `divui` takes no direct dependency on `windows`.
   - It is not an inherent method on `RasterConfiguration`: that type lives
     in `div-geometry`.
3. **Who gets it** (Windows, no explicit choice):
   - **The system configuration:**
     - `App::new(id)…run()` and every window it builds (`Defaults::build`);
     - `App::builder().build()`;
     - windows opened later by an app, which get the app's choice.
   - **`RasterConfiguration::default()`, unchanged on every platform,** so
     their output does not depend on the machine:
     - `HeadlessApp`;
     - `Document::new` and `Document::with_options`;
     - the `divui::testing` harness;
     - `DIV_RENDER_PNG` first-frame renders, which the CLI template check
       and the smoke tests use, as the system accent already does.
   - **One read for all windows:** every window shares it. Like Chromium, it
     uses the primary display, so a window on a second monitor with a
     different subpixel order gets the primary's.
   - **Settings changes:** like Chromium, which caches its parameters and
     does not update the renderer when they change, Div does not follow
     changes while running.
4. **The explicit choice:** `App::engine_options(...)` wins whenever an app
   calls it, `RasterConfiguration::default()` included. The framework
   makes the app's options internally `Option<EngineOptions>`, so "not set"
   differs from "set to the default"; the public API does not change.
5. **Where grayscale comes from:**
   - **Inside a page**, `div-paint` already decides LCD per layer
     (`LcdTextLayerScope` in `paint_record.rs`, after cc's rule). Layers
     that cc would not let keep LCD text stay grayscale. That stays
     engine-owned.
   - **Transparent windows:** Div has no transparent or per-pixel-alpha
     window option today. When one is added, it forces grayscale text
     itself, so apps do not have to opt out.
   - **Captured frames:** `render_to_png*` and the testing snapshots use
     headless documents and stay grayscale. An app that captures window
     frames to scale or rotate them sets a grayscale configuration
     explicitly.

```rust
// An app can always choose explicitly:
App::new("dev.example.app").engine_options(EngineOptions {
    raster_configuration: RasterConfiguration::chromium_windows_lcd(),
    ..EngineOptions::default()
});
// Without engine_options, a Windows app gets
// div_platform::system_raster_configuration().
```

**What is not measured:**
- A machine with ClearType tuning (the per-display key present), with
  standard smoothing or with BGR order. The source defines those paths;
  each needs a measurement on a machine configured that way before Div
  claims to match it.
- Changing this machine's settings is the user's decision.

**Oracle pin:**
- Windows Chromium oracles are captured with ClearType on, RGB order and
  no per-display `Avalon.Graphics` key.
- `tools/qualification/windows_chrome.py` checks those three settings and
  refuses to capture otherwise.

## Affected tracks

- **Engine:** add `chromium_windows_lcd()` next to `chromium_linux_lcd()`
  in `div-geometry`, and the oracle check in `windows_chrome.py`.
- **Framework:**
  - add `div_platform::system_raster_configuration()` and its system
    reads, Windows only;
  - make the app's engine options optional internally;
  - use the system configuration on the windowed paths above;
  - fix `run_document`, which builds later windows with
    `EngineOptions::default()` instead of the app's choice;
  - document the boundary.
- **Fidelity lane:** a native test on Windows that pins the web-font page
  with `chromium_windows_lcd()` (exact at 1x; the 2x rows wait on RFC
  0011), and records the gap closed.
- **Components, SVG and canvas lanes, and the samples:**
  - In this repository at `4815e9d`, their harnesses use headless documents
    (`HeadlessApp`) or create none, so this RFC does not change their
    frames. The framework track checked this on 18 November 2026.
  - Each lane confirms that on the framework PR.
  - Harnesses that capture a live window, such as samples outside this
    repository, change, and re-measure on that PR.
  - The windowed examples (hello, counter, todo and others) change, as
    intended. Their only pixel check, `check_cli_template.py` through
    `DIV_RENDER_PNG`, keeps the default.
  - The SVG lane's Windows text gap (LCD, found in its T1 work) is the same
    policy; it closes when its harness uses the preset.
- **Linux and macOS:** unchanged by this RFC.

## Migration

1. Merge this RFC once the user has approved it.
2. Engine PR: the preset and the oracle check. No default changes, so
   `main` stays green.
3. Fidelity PR: the Windows web-font fixture (`chromium-windows-1x.png`
   and `-2x.png`, captured under the oracle pin) and its native test, using
   the preset explicitly.
4. Framework PR, after the engine PR:
   - adds the system read, the optional options and the Windows default on
     the windowed paths;
   - lanes with Windows comparisons re-measure on it before it merges, and
     any changed count is recorded in that PR.
