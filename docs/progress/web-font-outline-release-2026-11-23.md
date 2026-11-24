# Web-font outline release (23 November 2026)

Engine track. The process-wide web-font outline cache
(`crates/div-text/src/shaping/web_font_outlines.rs`) kept up to 32 entries
after the last engine was gone. Each entry holds its own copy of the font's
bytes, its glyph paths and a custom typeface. #136 named this second retention
and left it for its own PR.

## What changed

- `FontCacheLifetime::drop` now empties the outline cache when the last client
  goes: after the shaper's parent fonts, before Skia's strike cache is purged.
  The entries are swapped out under the cache's lock and destroyed after it is
  released. Fonts and text blobs already built keep their own typeface
  reference.
- The two lifetime tests are now unit tests in `shaping::lifetime_tests`, in a
  new file. They replace `crates/div-text/tests/shaping_font_lifetime.rs`
  (from #136) and the outline test that this branch first added. Each test
  re-runs its own test binary for that one test, with a private environment
  variable marking the child, so the process-wide caches belong to that test
  alone. The parent fails unless the child reports exactly one passing test.
- `retained_parent_font_count`, which #136 made `#[doc(hidden)] pub`, and the
  new `retained_web_font_outline_count` are now `#[cfg(test)] pub(crate)`. Both
  `pub use` lines are removed, so this PR adds no public item and removes one.
  Every lifetime assertion is kept.

## Measurements

- Before the change, a probe kept 24,311,040 bytes for one 760 KB web font
  after the last engine: 32 entries, 480 paths and 32 typefaces. Resident
  memory grew from 8.9 MB to 46.4 MB over 48 engines.
- The worst case from the constants, not measured, is 32 entries of up to
  64 MB each.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, 1x | `db3e244` to `2d435c0` | 16,297 of 23,728 exact before and after, 0 Div images changed |
| Census, real zoom | `db3e244` to `2d435c0` | 10,404 of 17,796 exact before and after, 0 Div images changed |
| CSS Grid coverage, 1x | `db3e244` to `2d435c0` | 1,473 of 4,204 exact before and after, 0 Div images changed |
| CSS Grid coverage, real zoom | `db3e244` to `2d435c0` | 894 of 3,153 exact before and after, 0 Div images changed |
| Linux `div-text`, `div-paint`, `div-canvas`, `div-ffi`, `div-engine`, `divui` | `2d435c0` on `db3e244` | 3,527 passed, 0 failed, 10 ignored |
| The same suites | `26fb891` on `main` `b7e4f6a` | 3,532 passed, 0 failed, 10 ignored |
| Without `release_parent_fonts()` | stage before the fix | Parent-font test fails at `lifetime_tests.rs:69`: 2 retained, 0 expected |
| Without `release_web_font_outlines()` | stage before the fix | Outline test fails at `lifetime_tests.rs:171`: 1 retained, 0 expected |
| A misspelled test name | stage | The child runs no test and exits 0; the parent fails |
| Outline test under leak and address sanitizers | `2d435c0` | Passes under both |
| `generate_v02_contract.py --check`, `generate_ffi.py --check` | `26fb891` | No drift |

Every Chromium image is identical between the census runs. The receipts are in
the data repository at `4ce88e21`, under
`docs/progress/evidence/web-font-outline-release-div-*-v1.json`. Each stage
finished on its first attempt, and its logs hold no `/mnt/e` memory error.

**What the census pair does not cover.** It was measured at
`db3e244` against `2d435c0`, before #139, #140, #112, #141 and #126.
It was not rerun after the rebase. The code patch is unchanged by the rebase,
and the PR adds no generated file:

```text
git diff <base> <head> | git patch-id --stable
db3e244..2d435c0  993ea6e23e0651bed16e4ed229691e1a10a373c1
b7e4f6a..26fb891  993ea6e23e0651bed16e4ed229691e1a10a373c1
59d4b20..c40e8fd  993ea6e23e0651bed16e4ed229691e1a10a373c1
```

**Not run locally.** The Windows suites run only in CI for this PR.

## Pre-existing failure: 320 bytes through Fontconfig's XML parser

Owner: Fontconfig initialisation in `div-text`. This PR does not change it, and
adds no suppression, allowlist or environment setting.

- On `main` at `d53a2d9`, `RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p div-text --test shaping_font_lifetime` passes its test, then LeakSanitizer
  reports 320 bytes in 3 allocations (256 direct, 64 indirect) through
  `libexpat`, and the process exits 23.
- At `2d435c0`, `RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p div-text --lib lifetime_tests` passes the outline test. The parent-font
  test's child reports the same 320 bytes in 3 allocations and exits 23, so
  that test fails. AddressSanitizer reports the same 320 bytes.
- This machine has Fontconfig 2.17.1, and CI uses 2.15.0. CI's
  `native-sanitizers` job runs only `div-ffi` (`.github/workflows/hardening.yml`),
  so it does not run either test.
- The fix is a separate later item.
