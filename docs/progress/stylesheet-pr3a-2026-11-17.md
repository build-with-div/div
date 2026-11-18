# 0.1 PR 3a: `stylesheet!`, 17 November 2026

Tier 0 item 0.1 (the cascade), third PR, after R0a (#70) and R0b (#77). An application can now write CSS style rules, in a file or inline, and get a typed `StyleSheet` at compile time, plus typed class names. A document adopts the sheet. It follows RFC 0001's "Macros" and "Public API" sections and the PR 3 plan (3a; 3b brings `css!`, `use_classes!` and `view!` classes).

## Change

| Change | Owner | Source |
| --- | --- | --- |
| Macro-time sheet parser: style rules, CSS Nesting (`&` and implicit; declarations after a nested rule form their own rule after it, as nested declarations rules do), `@layer a, b;` and `@layer name { }`. Each declaration goes through the `style!` declaration parser, so values are exactly what `parse_literal` accepts. Every error is reported with its position. `@media` (item 0.5), `@supports`, other at-rules, anonymous and nested layers, dotted layer names and at-rules inside style rules are errors | `div-style` (engine) | `crates/div-style/src/stylesheet_parser.rs`, behind the internal `constructor-tokens` feature |
| `ConstructorTokens` derives on the selector types, so selectors are emitted as constructors; no selector text is parsed at run time | `div-style` | `crates/div-style/src/selector.rs` |
| `ClassId`: a typed class name. It derefs to `str`, so `element.add_class(&sheet::classes::card)` works | `div-style` | `crates/div-style/src/stylesheet.rs` |
| `stylesheet!(vis name, "file.css")` and `stylesheet!(vis name { ... })`: a module `name` holding `SHEET: LazyLock<StyleSheet>` and `classes`, with one `ClassId` per class the selectors use. The file form resolves against `CARGO_MANIFEST_DIR` and includes the file's bytes, so editing it rebuilds. Class names that are not identifiers get a stable, tested mangling (`hover:bg-muted` is `hover_u3a_bg_muted`), keywords become raw identifiers, and two names that mangle alike are a compile error | `div-macros` | `crates/div-macros/src/stylesheet_macro.rs`, `lib.rs` |
| `Document::adopt_stylesheet` and `remove_stylesheet`, `Error::StyleSheet`, and the `StyleSheetId`/`StyleSheetError` re-exports. `StyleSheetError` gains `Display` and `Error` | `divui` (framework surface, added by the engine as the plan says), `div-engine` | `crates/divui/src/document.rs`, `style.rs`, `lib.rs`; `crates/div-engine/src/rule_cascade.rs` |

The plan put the sheet in a `static` of the module's name, beside the module. The cascade test found why it cannot be: Rust forbids a `let` binding that shares a static's name, and a lowercase static such as `attributes` takes that name from every function in the module. The sheet is `name::SHEET` instead.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `tools/qualification/capture_cascade_oracle.py` on Windows with pinned Chrome 147.0.7727.50, written to `tests/fidelity/fixtures/cascade-v1/chromium-windows.json` | 8 cases, 43 elements, 8 probes each (`background-color`, `color`, `display`, `font-size`, `font-weight`, `margin-top`, `opacity`, `padding-left`): specificity, layers (normal and `!important`), the style attribute against `!important`, nesting, `:is`/`:where`/`:not`/`:has`, attribute operators with the `i` flag, structural pseudo-classes and `nth-child(of)`, inheritance |
| Oracle test | `cargo test --locked -p divui --test cascade_oracle` (Windows) | All 344 values (43 elements, 8 probes) equal Chromium's, with no named gaps |
| Macro tests | `cargo test --locked -p divui --test stylesheet_macro`; `cargo test --locked -p div-macros --lib`; `cargo test --locked -p div-style --features constructor-tokens --lib stylesheet_parser` (Windows) | 4, 50 and 7 passed |
| Touched crates | `cargo test --locked -j 4 -p div-style -p div-macros -p div-engine -p divui --no-fail-fast` (Windows) | 1,060 passed, 0 failed, 16 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `474d6e20` against `main` at `8b9f4d79` | 16,287 of 23,728 exact before and after; 0 Div images changed |
| CSS Grid coverage, all four profiles (WSL) | coverage matrix, same head and base | 1,473 of 4,204 exact before and after; 0 statuses or images changed |

The evidence is in div-fidelity-data `f70a3744`, which `fidelity-data.lock` now pins. That commit also holds T1's and T4's evidence (`b679bf2f`, `24125445`).

## Not done here

- **Duplicate-longhand warnings.** RFC 0001 asks for a compile warning on a repeated longhand, or a shorthand after a longhand it resets. Stable proc macros have no warning API; this needs a lint-style workaround or a later toolchain. Named for 3b.
- **`toggle_class` and `set_classes` taking `ClassId`.** `add_class`, `remove_class` and `has_class` take `&str`, and `&ClassId` derefs to it. A typed surface is the framework track's call, with 3b.
- **`@supports`** constant-folding, **`@media`** (0.5), CSS-wide keywords and `var()` (0.5), `<style>` and runtime sheet text (S2), pseudo-elements (S8).
- **Pixels and scripted state changes** in the cascade suite: later cases.
