# Skia is initialized before any Div code uses it, 14 November 2026

Merge-queue runs could abort in the `div-compositor` lib tests with heap
corruption (`munmap_chunk(): invalid pointer`, `free(): invalid size` and
similar), in about 2% of runs. Framework found it while triaging the
ejection of build-with-div/div#31 (run 43579462821); the cause is in
`div-paint`'s Skia initialization, not in #31.

## Cause

`SkGraphics::Init` installs Skia's CPU-specific function tables, including
the raster pipeline stages. Div ran it once, behind a `Once`, but lazily:
from `div_paint::paint_fragment` and `div_paint::rasterize_picture`. Any
thread that drew with Skia before the first of those could still be
drawing through the baseline tables while another thread swapped them. A
pipeline built from one table and run with the other's stride writes past
its buffers.

In the `div-compositor` lib tests,
`embedded_scene_replays_preserve_host_composition_at_each_device_scale`
draws with `skia_safe` directly, and recording an empty document never
reaches `paint_fragment`. The tests it races with rasterize through
`rasterize_picture`, whose first call initializes Skia. `div-paint`'s own
unit tests were safe only because that binary initializes at load
(`.init_array` / `.CRT$XCU`) under `#[cfg(test)]`.

## Change

`initialize_skia` moves to `div-text`, the lowest crate that uses Skia, and
Div calls it where it creates Skia users, so the `Once` orders the
initialization before everything that follows:

| Where | Owner | Source |
| --- | --- | --- |
| `FontCollection::system` and `FontCollection::default`. Every document owns a collection, so every layout, paint and image decode for a document follows them | `div-text` | `crates/div-text/src/font/collection.rs` |
| `TextShaper::new` | `div-text` | `crates/div-text/src/shaping/shaper.rs` |
| Paint recording (`record_fragment`, `record_fragment_with_program`, `record_document`, and through them `render_to_surface` and `render_to_png`), besides `paint_fragment` and `rasterize_picture` | `div-paint` | `crates/div-paint/src/render.rs` |
| `SoftwareCompositor::default` and `GaneshGlCompositor::new` | `div-compositor` | `crates/div-compositor/src/lib.rs`, `crates/div-compositor/src/ganesh_gl.rs` |
| `Engine::new` and its variants | `div-engine` | `crates/div-engine/src/lib.rs` |

`div_paint::initialize_skia` remains, as a re-export.

Test binaries that draw with `skia_safe` directly, outside Div's
constructors, initialize at load with `div_paint::initialize_skia_at_load!()`
(a hidden macro in `div-text`; it now also covers macOS): `div-paint`'s unit
tests, as before; the `div-compositor` lib tests; and the 24 `div-paint`
integration test files that create surfaces, canvases or picture
recorders. Test files that only use `skia_safe` types without drawing
(`source_layer_program.rs`, `div-text`'s `font_cache_lifetime.rs`) and the
single-threaded paint benchmark are left alone.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Parallel loop (WSL, CI's Linux environment: clang, Skia built from source with CI's `SKIA_GN_ARGS`, `rustflags=[]`) | `div-compositor` lib test binary, default parallel harness, run 900 times at each commit | `main` `d0d7f376`: 18 of 900 aborted (`free(): invalid size`, `corrupted size vs. prev_size while consolidating`, `malloc(): invalid size (unsorted)`, `realloc(): invalid next size`, `munmap_chunk(): invalid pointer`). This change (`255c617e`): 0 of 900. This change without the `div-compositor` tests' load-time initializer, so that only Div's constructors order the initialization: 0 of 900 |
| Windows tests | `cargo test --locked -p div-text -p div-paint -p div-compositor -p div-engine --no-fail-fast` | 2,886 passed, 0 failed, 5 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify` | pass |
| Expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, head `255c617e`, against the run of `5b7b8e47` (build-with-div/div#13, whose engine code is `main`'s) | 22,519 of 23,728 exact before and after; Chromium identical in all comparisons; 0 Div images changed ([delta](evidence/skia-init-order-div-census-v1.json)) |
| CSS Grid coverage (WSL) | coverage matrix, same head and base | 2,227 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/skia-init-order-div-coverage-v1.json)) |

The evidence files are in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data)
`5dc50ac3`; `python3 tools/fidelity_data.py fetch --profile all` restores
them here.

The census and the loops measured `255c617e` on `main` at `d0d7f376`. The
branch was then rebased on `9168ac91` (build-with-div/div#33, the nested
flex pass cache). That change is layout-only and its own census was
unchanged; this one changes only when Skia is initialized, so the receipts
hold for the head.
