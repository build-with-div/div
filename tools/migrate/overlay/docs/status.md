# Status

A short snapshot at the import from
[zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) (November 2026).
Tracks record what changes after it in [progress/tracks](progress/tracks/README.md);
the full earlier log is
[current-status.md in the archived repository](https://github.com/zhuowcui/open-ui/blob/main/docs/progress/current-status.md).

| Area | State |
| --- | --- |
| Renderer census | 22,517 of 23,728 comparisons pixel-exact (94.9%) against Chrome for Testing 147 across four profiles, in ten CSS areas. Text, tables, transforms and others are only partly measured. |
| CSS Grid | Measured in a separate coverage suite: 2,218 of 4,204 comparisons exact (52.8%). |
| App framework | `view!` markup, `#[component]`, signals and effects, typed events, inline typed styles; several windows, an async runtime, clipboard, native dialogs, an app menu and an HTTP client. |
| Platforms | Linux (X11, Wayland, AT-SPI) and Windows (Win32, DirectWrite text, UI Automation). No macOS yet. |
| C ABI | 164 `divui_*` exports in `include/divui.h`; C and C++ consumers in `examples/c`. |
| CLI | `div new`, `div dev` (rebuild and restart), `div build`, `div doctor`. |
| Components | shadcn/ui reference harness; separator, button, badge, card and label. |

Not yet: a stylesheet cascade, custom properties, media queries, a top
layer for dialogs and popovers, and macOS. See the
[alpha plan](alpha/plan.md) for the work in progress.
