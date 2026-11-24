# calc() literals in the style macros, 24 November 2026

Engine track. Owner: `div-style-derive`, plus one attribute line in `div-style`.

## Problem

On `main` `c2ad60ad`, every compile-time `calc()` failed to compile. That
covers `calc()` in `style:`, in `style!` and in `stylesheet!`. The
`ConstructorTokens` derive builds each value from `root::<Type>`, where `root`
is `::div::typed_style`. The `calc()` tree (`MathFunction`, `MathNode`,
`MathUnit`, `MathOperation`, `MathOperator`, `MathCategory` and `MathType`)
is public only at `div_style::math`, which is `div::typed_style::math`. The
components track found the bug and wrote the repro.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| A variant option, `#[constructor_tokens(module = "name")]`. The variant's fields are built with `root::name` as their root. `unsupported` and `module` cannot be combined, and both apply to enum variants only | `div-style-derive` | `crates/div-style-derive/src/lib.rs` |
| `StyleValue::Math` uses `module = "math"` (one added attribute line) | `div-style` | `crates/div-style/src/property.rs` |
| The components track's repro: a `calc()` `line-height` in each macro must compile and equal the runtime parse. The constructor corpus gains `calc(1.25 / 0.875)` and `calc(1 / 0.75)` | tests | `crates/divui/tests/calc_style_literals.rs`, `crates/div-macros/src/typed_style_corpus.txt` |

No public path is added. `math.rs` is unchanged, and so is every comment
naming Blink or Chromium code.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Red, on `main` `c2ad60ad` with only the tests added | `cargo test --locked -p divui --test calc_style_literals --test typed_style_constructors` (Linux, CI's Skia flags of that commit) | Both test binaries fail to compile: E0422 and E0433, because `MathFunction`, `MathNode`, `MathUnit`, `MathOperation`, `MathOperator`, `MathCategory` and `MathType` are not found in `div::typed_style` |
| Green, `fad6aa8` | The same, plus `--test style_macro` | `calc_style_literals` 3 of 3, `style_macro` 6 of 6, `typed_style_constructors` 3 of 3 |
| Generators and ledgers (Windows), `fad6aa8` | `check_skia_flags.py`, `check_platform_scoped_tests.py`, the renderer contract, v0.2, property and FFI generators, and the SP13-R, SP17, SP18 and SP20 closures with `--check`; rustfmt on each changed file; `fidelity_data.py fetch` and `verify --profile all` | All pass; 3 changed Rust files are clean |
| Same files at every head | `git rev-parse <commit>:<path>` for the four changed files | Identical blobs at `fad6aa8`, `b618850b` (measured) and `da8d482` (this PR) |

The tests and checks were not rerun at `da8d482`. On 24 November, E: was
below the 50 GB free-space floor, and no new builds were started. The four
changed files are byte-identical to `fad6aa8`, and CI runs the full set.

## D4: census and coverage

**A binary comparison first.** The orchestrator agreed to it as a D4
substitute for this PR only, and it failed its condition.

Both sides were built as release `pixel_compare` binaries:
- in one WSL clone and one target directory (`~/census-target-a8`);
- under the census lock;
- with Skia's flags from the commit's `ci.yml` and no override, identical on
  both sides.

The comparison masked the embedded `renderer-build-identity.json` block.

| Measure | Base `c2ad60ad` | Head `fad6aa84` |
| --- | --- | --- |
| Binary size | 480,983,384 bytes | 480,983,384 bytes |
| sha256 | `64801ff69f01da1842c71bf3d9b64023b98a61ebeecb11fbd9abc12e433e4a3f` | `fbdf332324e50d0237987966b7087ed90f5001f7c5aa131a4298e2d4d543bf87` |
| Identity block | 661 bytes at offset 4,943,312, once | the same |
| `source_tree_sha256` | `dfe7947cafd32fcf3bbf2012660c4b1dfb825084c0ea4b361e3020caee0f546d` | `7576a7e1da75ab12fdf90765fe8e3f021a50a9756d4f3e517928277bddbf9cb8` |
| `harness_sha256` | `5fd2df6f6c1a8b8875254a7acd72c1babbb12e4a56014269022676355f05f21e` | the same |

In the masked range `[4943312, 4943973)`, 99 bytes differ. Outside it,
861,954 bytes differ, the first at offset 812. This is expected:
pixel-compare enables the `constructor-tokens` feature, so the derive change
changes the code compiled into it. Any byte differing outside the block
requires the full pair.

**The full pair.**
- **How it ran:** the expanded census (16 jobs) and CSS Grid coverage, at 1x
  and with real zoom, under the shared lock, on Linux.
- **Commits:** the base is `main` `b7e4f6a0`. The head is `b618850b`, the
  fix replayed onto `b7e4f6a0` with the same patch ID.
- **Base results:** the ones measured at `b7e4f6a0` for the outline release
  pair (#143), reused unchanged.
- **Skia flags:** `b7e4f6a0`'s, the same on both sides, before #142 added
  `-O2`.

| Profile | Exact, base and head |
| --- | --- |
| Census, 1x | 16,297 of 23,728 |
| Census, zoom | 10,404 of 17,796 |
| CSS Grid coverage, 1x | 1,473 of 4,204 |
| CSS Grid coverage, zoom | 894 of 3,153 |

No profile changed: no Div image changed, there were no gains, losses or
errors, and every Chromium image was identical. Evidence: `div-fidelity-data`
`b6c41deb`,
`archive/docs/progress/evidence/calc-macro-paths-div-{census,census-zoom,coverage,coverage-zoom}-v1.json`.

**Carried under D4.** `git diff <base> <head> | git patch-id --stable` gives
`0669dca8` for each of these ranges:
- `b7e4f6a0..b618850b` (measured);
- `c2ad60ad..fad6aa84`;
- `97247afa..2addea70`;
- `cbfd23c3..da8d482` (this PR).

## Text gaps left by the T8 sweep

These four cases are outside the 14 widths the multi-run width change
cleared. They are now engine-owned named gaps, listed in
[the engine track file](tracks/engine.md#text-gaps-left-by-the-t8-sweep-24-november-2026).
No work on them is planned yet.
