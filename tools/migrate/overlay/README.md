> **Migration in progress.** This repository is not live yet; development
> continues at [zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) until
> cutover.

# Div

Everything you love about web UI, none of the weight.

Div is a native Rust UI framework for desktop apps. It lays out and paints
CSS-styled views with Skia: no HTML, no JavaScript, no webview. Its renderer
is measured pixel by pixel against Chrome for Testing 147.0.7727.50, so
designs built for the web can be ported and checked.

Div is pre-alpha. Linux (X11, Wayland) and Windows are supported; the API
will change.

```rust,no_run
use div::prelude::*;

fn main() -> Result<(), Error> {
    let count = create_signal(0_i32);
    let app = App::builder()
        .title("Div")
        .size(LogicalSize::new(800.0, 600.0))
        .build()?;

    app.run(move || view! {
        <button
            style:padding="8px 16px"
            on:click={move |_| count.update(|value| *value += 1)}
        >
            {count.get()}
        </button>
    })
}
```

## Build from this repository

Rust 1.85 or newer and a C/C++ toolchain are required. See
[the development guide](docs/DEVELOPMENT.md) for platform packages and the
shared prebuilt Skia.

```bash
cargo build --workspace --locked
cargo run --locked --package hello                    # headless, writes hello.png
cargo run --locked --package hello --features linux   # a native window
cargo install --locked --path crates/div-cli          # the `div` command
div new my-app
```

Applications depend on the `divui` package and write `use div::…`.

## Repository

| Path | Contents |
| --- | --- |
| `crates/divui` | The application framework (`use div::prelude::*`) |
| `crates/div-engine`, `div-style`, `div-layout`, `div-text`, `div-paint`, `div-compositor` | The rendering engine |
| `crates/div-platform` | Windows, X11 and Wayland integration |
| `crates/div-macros`, `div-cli`, `div-components` | `view!`, the `div` command, shadcn/ui components |
| `crates/div-ffi`, `include/` | The C ABI (`divui_*`) and its generated headers |
| `examples/rust`, `examples/c` | Rust apps and C/C++ ABI consumers |
| `tests/fidelity` | The Chromium comparison harness, WPT ports and component reference |
| `tools` | Generators, qualification, release and migration tooling |
| `docs` | Contributor documentation, RFCs and the renderer contract |

The comparison evidence (census results, the frozen 5,731-image archive and
older receipts) lives in
[build-with-div/div-fidelity-data](https://github.com/build-with-div/div-fidelity-data),
pinned by `fidelity-data.lock`. `python3 tools/fidelity_data.py fetch`
restores the part the gates read.

This repository was imported from
[zhuowcui/open-ui](https://github.com/zhuowcui/open-ui), where the earlier
history, the retired C++/Blink backend and the progress logs remain.

## Documentation

- [Development](docs/DEVELOPMENT.md) and [CI](docs/CI.md)
- [Contributing](CONTRIBUTING.md)
- [Rendering pipeline](docs/architecture/rendering-pipeline-overview.md)
- [Renderer contract](docs/renderer/contract.md)
- [C ABI](crates/div-ffi/README.md)
- [Alpha plan](docs/alpha/plan.md) and [RFCs](docs/alpha/rfcs)

Div is licensed under Apache-2.0.
