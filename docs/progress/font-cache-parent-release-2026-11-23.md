# Font cache parent release (23 November 2026)

Engine track. The shaper's process-wide cache kept up to 100 HarfBuzz parent
fonts, each owning a Skia typeface, after the last engine was gone. That
retention came from #125 and is why `main`'s weekly `native-sanitizers
(address)` and `(leak)` jobs failed (run 45197262690).

## What changed

- `FontCacheLifetime::drop` now releases the parent fonts when the last client
  goes, just before it purges Skia's strike cache. There is no LeakSanitizer
  suppression.
- `crates/div-text/tests/shaping_font_lifetime.rs` fails without the release:
  two parent fonts stay after the last client.

## Ordering

- The last drop holds the client lock for the whole release, so an engine
  starting during teardown waits in `FontCacheLifetime::new` until the release
  and purge are done.
- Every shaping call holds a `FontPlatformData`, which holds a lifetime. So no
  shape is running when the count reaches zero, and none can start before the
  client lock is released.
- The lock order is the client lock, then the parent-font lock. `create_font`
  never takes or drops a lifetime while it holds the parent-font lock.
- The parent fonts are swapped out under their lock and destroyed after it is
  released. Shaping after a release rebuilds the cache, and the new test checks
  that the width is unchanged.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, 1x | `8fbf18e` to `0a11dbe` | 16,297 of 23,728 exact before and after, 0 Div images changed |
| Census, real zoom | `8fbf18e` to `0a11dbe` | 10,404 of 17,796 exact before and after, 0 Div images changed |
| CSS Grid coverage, 1x | `8fbf18e` to `0a11dbe` | 1,473 of 4,204 exact before and after, 0 Div images changed |
| CSS Grid coverage, real zoom | `8fbf18e` to `0a11dbe` | 894 of 3,153 exact before and after, 0 Div images changed |
| `div-ffi` under LeakSanitizer, locally | `8fbf18e` | 145 pass, then 5,552 bytes in 121 allocations reported |
| New test, without the release | stage before the fix | Fails: 2 parent fonts retained |
| New test, with the release, under leak and address | `0a11dbe` | Passes under both |
| `div-ffi` under LeakSanitizer, locally, 9 runs | `0a11dbe` | 145 pass, 320 bytes in 3 allocations in all 9 |
| Hardening run 45247963427, dispatched on the branch | `0a11dbe` | `native-sanitizers (address)` and `(leak)`, `c-undefined-sanitizer`, `msrv`, `miri-handles` and `conformance-and-platform` pass. `fuzz-smoke` failed on `fuzz/Cargo.lock`, which #135 has since repaired |
| Linux `div-text`, `div-paint`, `div-canvas`, `div-ffi`, `div-engine`, `divui` | `e6a2c1f` on `main` `9219d0a` | 3,526 passed, 0 failed, 10 ignored |

The receipts are in the data repository at `63f6e1ae`, under
`docs/progress/evidence/font-cache-parent-release-div-*-v1.json`.

One fixture restore failed at the start of the census with the `/mnt/e`
`OSError: [Errno 12] Cannot allocate memory` error, before any page was
rendered. It was retried once and passed. The results were accepted only after
a check that each stage's logs hold no such error and that all four summaries
are fresh and complete.

**What the census pair does not cover.** It was measured at `8fbf18e` against
`0a11dbe`, before #134 (internal CSS names) and #135 (lockfiles). It was not
rerun after the rebase. The code patch is unchanged by the rebase:

```text
git diff <base> <head> | git patch-id --stable
8fbf18e..0a11dbe  1f32bb6c9a7937a7af2555144f050f303e13cecf
9219d0a..e6a2c1f  1f32bb6c9a7937a7af2555144f050f303e13cecf
```

**Not run locally.** The Windows suites run only in CI for this PR.

## Known remainders

- **320 bytes locally.** This machine still reports 320 bytes in 3 allocations
  from Fontconfig's XML config parser (Fontconfig 2.17.1). The framework lane
  measured the same remainder before #125, at `154d0a72`. 5,552 less 320 is
  CI's 5,232. CI uses Fontconfig 2.15.0, and its sanitizer jobs pass.
- **A second cache still outlives the last engine.** `web_font_outlines.rs`
  keeps up to 32 entries, each with its own copy of the web-font bytes, on
  Linux. A probe measured 24.3 MB retained for a 760 KB font after the last
  engine was gone. It gets its own PR after this one.
