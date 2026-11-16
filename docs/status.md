# Status

A short snapshot, first written at the import from
[zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) (November 2026)
and updated on 16 November 2026.
Tracks record what changes after it in [progress/tracks](progress/tracks/README.md);
the full earlier log is
[current-status.md in the archived repository](https://github.com/zhuowcui/open-ui/blob/main/docs/progress/current-status.md).

| Area | State |
| --- | --- |
| Renderer census | 22,517 of 23,728 comparisons pixel-exact (94.9%) against Chrome for Testing 147 across four profiles, in ten CSS areas. Text, tables, transforms and others are only partly measured. Above scale 1 these are against CDP-emulated Chromium; [RFC 0010](alpha/rfcs/0010-real-zoom-oracle.md) moves that oracle to real zoom and resets the baseline ([#59](https://github.com/build-with-div/div/pull/59)). |
| CSS Grid | Measured in a separate coverage suite: 2,218 of 4,204 comparisons exact (52.8%). |
| App framework | `view!` markup, `#[component]`, signals and effects, typed events, inline typed styles, `requestAnimationFrame` ([#47](https://github.com/build-with-div/div/pull/47)) and `ResizeObserver` ([#52](https://github.com/build-with-div/div/pull/52)); several windows, an async runtime, clipboard, native dialogs, an app menu and an HTTP client. |
| Platforms | Linux (X11, Wayland, AT-SPI) and Windows (Win32, DirectWrite text, UI Automation). No macOS yet. |
| C ABI | 164 `divui_*` exports in `include/divui.h`; C and C++ consumers in `examples/c`. |
| CLI | `div new`, `div dev` (rebuild and restart), `div build`, `div doctor`. |
| Components | shadcn/ui reference harness; separator, button, badge, card, label and tabs ([#23](https://github.com/build-with-div/div/pull/23)), 16 of 216 comparisons exact at `6fc0c1a7`; switch in progress ([#56](https://github.com/build-with-div/div/pull/56)). |

Not yet: a stylesheet cascade (selectors match on the element tree since
[#17](https://github.com/build-with-div/div/pull/17)), custom properties,
media queries, the UA stylesheet, Pointer Events, inline SVG, Canvas 2D,
a top layer for dialogs and popovers, and macOS. The
[alpha.1 definition](alpha/plan.md#alpha1-definition-16-november-2026)
lists each item with its owner and state.
