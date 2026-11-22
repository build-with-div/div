# Setup track progress

## Windows Skia without FMA contraction 22 November 2026

Chromium 147.0.7727.50 compiles every file with `-ffp-contract=off`; on
Windows it passes `/clang:-ffp-contract=off` (`build/config/compiler/BUILD.gn`
lines 653-658). Skia's own GN adds the flag only for clang off Windows
(`gn/skia/BUILD.gn:71`). rust-skia builds Windows Skia with clang-cl, so
Div's Windows Skia fused `a*b+c` into FMA in its `/arch:AVX2` (HSW) raster
stages, and Chromium's does not.

- The windows-msvc job's `SKIA_GN_ARGS` adds `/clang:-ffp-contract=off`.
  clang-cl ignores the plain `-ffp-contract=off` ("unknown argument
  ignored"), and Linux clang would read the `/clang:` form as a file name,
  so the flag is Windows' alone.
- `crates/div-cli/src/skia.rs` has a Windows profile, `WINDOWS_GN_ARGS`,
  that `div new` writes on Windows, and its drift test pins windows-msvc to
  it. The framework lane reviewed it.
- `docs/DEVELOPMENT.md` gives the flag in the Windows snippet.

Measured on Windows at main `848551a`, under the census lock, with CI's
flags against the same flags plus `/clang:-ffp-contract=off`, over CI's
headless workspace tests, divui's `testing` suites and div-components:

| Run | Passed | Failed | Ignored |
| --- | --- | --- | --- |
| CI flags | 10,113 | 0 | 31 |
| With `/clang:-ffp-contract=off` | 10,112 | 1 | 31 |

- The one failure is div-svg `paint_v4_matches_chromium_147`, which reports
  that the listed gaps `windows 2x rad/focal` and `windows 2x rad/repeat`
  are now exact. Both are removed from
  `tests/fidelity/fixtures/svg-paint-v4/known-gaps.txt`.
- Every other test, including every pinned pixel gap, is unchanged.
- `2x rad/fr-focal` (both OSes) and `linux 2x rad/focal` stay listed; their
  cause is not known.

## windows-msvc MSVC toolset pin 18 November 2026

Runner image `windows-2025-vs2026` 20261116.315.1 made VS 18's MSVC
14.52.37025 the newest toolset. Its STL rejects the image's Clang 20
(`yvals_core.h` STL1000, "expected Clang 22 or newer"), so windows-msvc
failed in the native-consumer check (`examples/c/native/window.cc`) on that
image and passed on 20261103.298.1, whichever runner a job got.

- The windows-msvc job pins MSVC `14.44.35207`, the toolset of local VS 2022
  Build Tools 17.14. The image still ships it
  (`Microsoft.VisualStudio.Component.VC.14.44.17.14.x86.x64`), and its STL
  needs Clang 19 or newer.
- `VCToolsVersion` pins cc-rs and rustc's linker lookup (find-msvc-tools).
  `VCToolsInstallDir` pins clang and libclang.
- Skia's GN build ignores both and takes the highest toolset under its
  Visual Studio (`gn/BUILDCONFIG.gn`). So `tools/skia_prebuilt.py` passes
  `VCToolsInstallDir`'s version as `win_toolchain_version`, which makes it
  part of the archive inputs.
- `tools/ci/msvc_toolset.py verify` checks four things: the toolset exists,
  the pins agree, Skia gets the same version, and clang compiles against
  exactly that STL. The STL's compiler check is never relaxed.
- cli-template (windows-2025) compiles Skia from the generated app's own
  `.cargo/config.toml`, outside `skia_prebuilt.py`, and its check strips the
  job's Skia variables on purpose. There, `msvc_toolset.py skia-vc` makes
  rust-skia's first fallback VC directory
  (`C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC`) hold only a
  junction to the pinned toolset. Skia's GN reads only
  `Tools/MSVC/<version>/{include,lib,bin}` there.
  `verify --skia app` checks that Skia resolves that toolset. The job also
  enables `core.longpaths`, as windows-msvc does, for rust-skia's dependency
  checkouts ("Filename too long" in spirv-cross and harfbuzz test data).
- `VCINSTALLDIR` is not used: it would make find-msvc-tools take `link.exe`
  from `PATH`, where Git bash puts GNU `link.exe` first. Skia would also
  still take the highest toolset under that directory.
- No other workflow job runs on Windows: `release.yml`, `hardening.yml`,
  `components.yml` and `site.yml` run on Linux only.

| Check | Command | Result |
| --- | --- | --- |
| Pin unit tests | `python -m unittest tools.test_skia_prebuilt tools.ci.test_msvc_toolset` | 11 passed on Windows and in WSL |
| Toolset check, local | `msvc_toolset.py verify` with Build Tools 14.44.35207, clang 23.1.3 | passes; a mismatched `VCToolsVersion` or a missing toolset fails |
| App-build check, local | `msvc_toolset.py verify --skia app`, the same toolset | passes with vcvars' `VCINSTALLDIR`; fails without it, where rust-skia resolves no VC directory and Skia would search for one itself |
| On the runner image | windows-msvc on the pull request | the CI run of this change |

## alpha.1 definition 16 November 2026

The user decided to update the alpha.1 definition ("yes let's update the
definition", 16 November 2026). `docs/alpha/plan.md` now defines alpha.1
in five parts: developer experience; the web platform foundations from the
six samples (all of Tier 0, keyboard scrolling, V1 inline SVG core, C1 and
C2 Canvas 2D core, and web fonts), with the rest of Tiers 1–3 listed for
alpha.2 or later; ten shadcn components with the dialog on a top layer; a
real-zoom fidelity baseline (RFC 0010); and the samples as proof. Each item
names its owner and its state (done, in progress or not started) with PR
links, as measured on `main` at `82d27be` and in the open PRs on 16
November. The 9 November definition stays below it as history. The risks
gain the engine lane's capacity, recorded and not decided. D5 is
recorded as decided: the product has been named Div since 11 November.
Section 5 also accounts for sample-git, the first dogfood app.

The platform plan and the 13 November decisions, until now only in the
parent session's notes, are in the repository as
`docs/alpha/platform-plan.md`, unchanged except for one local path.
`docs/status.md` gains `requestAnimationFrame`, `ResizeObserver`, tabs,
the pending real-zoom oracle and a current "Not yet" line.

Verification: docs only. Every relative link and heading anchor in the three
changed documents resolves (a local script over their Markdown links).

## Merge queue 14 November 2026

The user chose a GitHub merge queue for `main`, to replace the rule that
every PR rebases and reruns CI whenever `main` gains code. `AGENTS.md` and
`CONTRIBUTING.md` now describe the queue process: a PR whose own CI is green
is queued with `python tools/enqueue_pr.py <number>` (see below), the queue
tests it on top of `main` and the PRs ahead of it, and it is rebased only on
a conflict or a failed queue run. The ruleset is switched on separately, after this change
merges.

The two workflows that produce the eight required checks, `ci.yml`
(`rust-parity`, `windows-msvc`, `python-accountability`,
`historical-pixel-audit`, `dependency-security`, both `cli-template` legs)
and `format-check.yml` (`clang-format`), also run on `merge_group`
(`checks_requested`). Audit of what could differ in a queue run:

- No required job has an `if:` condition or a path filter, so none can be
  skipped without reporting. The path-filtered `components.yml` and
  `site.yml` and `hardening.yml` produce no required check and stay off the
  queue; `hardening.yml`'s non-PR jobs (MSRV, Miri, sanitizers, fuzzing)
  would otherwise start running on every queue entry.
- Both concurrency groups key on `github.ref`, which is unique per queue
  entry (`refs/heads/gh-readonly-queue/main/…`), not on `github.head_ref`.
- No step or tool reads the pull request base, `GITHUB_BASE_REF`,
  `GITHUB_HEAD_REF` or the event payload; the only diff is
  `git diff --exit-code` against the checked-out commit.

Verification: both workflows parse with PyYAML and list `merge_group` with
`types: [checks_requested]` and the same jobs and names;
`python tools/accountability/audit.py --repository-only` passes;
`git diff --check` is clean. `actionlint` is not installed on this host.

### Queueing a pull request

`gh pr merge <number> --merge` does not queue a pull request here: gh 2.102.0
calls `enablePullRequestAutoMerge`, and GitHub answers "Auto merge is not
allowed for this repository". Auto-merge stays disabled.
`tools/enqueue_pr.py <number>` instead runs the `enqueuePullRequest`
mutation behind the "Add to merge queue" button, with the node id and head
commit from `gh pr view <number> --json id,headRefOid`, and
`expectedHeadOid` set so that a push after the lookup is refused rather than
queued unseen. `AGENTS.md` and `CONTRIBUTING.md` name the script and the
mutation.

## Plan update: RFC corrections and status 10 November 2026

`docs/alpha/plan.md` records the following:

- D5 (name) is deferred. Div stays the project name; branding will be
  decided later and does not block alpha work.
- Correction from RFC 0001 (PR #89): `pointerenter` and `pointerleave`
  already exist. The remaining A1 pointer work is `pointerleave` on window
  exit and `:active` on the pressed element's ancestors (see
  [engine](engine.md)).
- RFC 0001 (PR #89) and RFC 0002 (PR #90) are merged. The components harness
  and the separator landed in PR #91 with 8 of 16 comparisons exact; dark is
  one level off from a paint color clamp (see [components](components.md)).
- The CLI skeleton (`new`, `dev`, `build`, `doctor`) landed in PR #93, so
  the gaps list no longer says there is no CLI.
- `oklch()`, `oklab()` and `color-mix()` landed in PR #95 (A2), and the app
  services in PRs #96, #98, #99, #100, #101, #103 and #105 (B1); the gaps list
  drops them. The C4 assets (#94) and the first static components (#102)
  are recorded, with the WOFF2 font failure that the open engine PR #97
  addresses (see [components](components.md) and [framework](framework.md)).

The change is documentation only. Verification:
`python tools/accountability/audit.py --repository-only` and
`git diff --check`.

## Week-0 setup 9 November 2026

The alpha plan is committed at `docs/alpha/plan.md`. `AGENTS.md` and
`CONTRIBUTING.md` now describe the per-track policy (D2): a worktree, branch
and one focused PR at a time per track, each merged to `main` on green CI
after rebasing; track ownership; one-page RFCs under `docs/alpha/rfcs/`; and
progress files in this directory.

Merged in PR #92 (`61e5e229`) on 10 November 2026. CI passed on the head
`c502eb89`, rebased on `ee46e350`. PR #105 merged during that run and
changed only other lines of `.github/workflows/ci.yml`, so the merge onto
`a9de940d` was checked locally (census lock tests, the repository audit,
`git diff --check`), not by another CI run.

### Census lock

`tools/qualification/census_lock.py` runs a command while holding
`<main checkout>/out/census.lock`, an exclusively created file that Windows
(`E:\open-ui\out\census.lock`) and WSL (`/mnt/e/open-ui/out/census.lock`)
both see. It finds the main checkout without git, because WSL git cannot
follow a worktree made by Windows git. A dead holder on the same system is
reclaimed; a holder on the other system needs `release --force`.

| Check | Where | Result |
| --- | --- | --- |
| `python -m unittest tools.qualification.test_census_lock` | Windows and WSL | 8 passed on each |
| WSL holds `run -- sleep 30`; Windows runs `status` and `run` | Windows and WSL | Windows names the WSL holder and `run` exits 1 without running; free after |

### Census lock queue (17 November 2026)

`run --wait` used to poll the lock every 30 seconds, so after a release
whichever waiter polled first took it, and `status` could not name the
waiters. On 17 November 2026 the rounded-clip census rerun nearly took the
lock ahead of the engine's T4 census, which had waited longer. Lanes gated
on each other's waiter pids by hand until this change.

Now each waiter writes a ticket, an exclusively created file in
`<lock>.queue/` named by the time it was queued (nanoseconds), its system
and its pid, and takes the lock only when the lock is free and no live
ticket is older. Its ticket is removed when it takes the lock or stops
waiting. A ticket of a dead waiter on the same system is dropped; one from
the other system keeps its place until `release --force --ticket <name>`.
`run` without `--wait` fails while anyone is queued. `status` lists the
queue in order and marks stale tickets.

| Check | Where | Result |
| --- | --- | --- |
| `python -m unittest tools.qualification.test_census_lock` | Windows and WSL | 16 passed on each: queue order with two waiter processes, a free lock held back by an older live ticket, `run` without `--wait` refused while a waiter is queued, a killed waiter's ticket dropped, a dead waiter's older ticket skipped, the other system's ticket kept, `release --force --ticket` and `status` |
| WSL holds `run -- sleep 8` on a scratch lock; a Windows waiter queues, then a WSL waiter; Windows runs `status`, WSL runs `run` without `--wait` | Windows and WSL | `status` lists the Windows then the WSL ticket; `run` exits 1; the commands ran holder, Windows waiter, WSL waiter; the lock and queue are empty after |

### One shared `out/` for every clone (18 November 2026)

`census_lock.py` and `skia_prebuilt.py` found the shared `out/` through
the main checkout of the checkout they ran from. A lane working in its own
clone kept under `E:\open-ui\out\` (for example `E:\open-ui\out\div-fidelity`
and its worktrees) therefore got that clone's `out/`. Its census lock was a
private file that excluded no one, and `skia_prebuilt.py env` found no
archive, so Skia compiled from source again. Lanes had to call the copies in
the `E:\open-ui` checkout by full path.

Both now use one resolver, `census_lock.shared_out()`. It returns
`DIV_SHARED_OUT` if that is set. Otherwise it takes the main checkout and,
while that checkout lies under another checkout's `out/`, moves to the
outer one, then returns its `out/`. `skia_prebuilt.py` no longer asks git.

| Check | Where | Result |
| --- | --- | --- |
| `python -m unittest tools.qualification.test_census_lock tools.test_skia_prebuilt` | Windows | 23 passed. The new cases: a nested clone, its worktree, the outer checkout and the outer checkout's worktree all resolve to the outer `out/`; a standalone checkout's worktree resolves to its own `out/`; `DIV_SHARED_OUT` wins; `skia_prebuilt` uses the same rule |
| `python3 -m unittest tools.qualification.test_census_lock` | WSL | 19 passed |
| `census_lock.py status` from a worktree of the `E:\open-ui\out\div-fidelity` clone | WSL | `free (/mnt/e/open-ui/out/census.lock)`, the shared lock (before: the clone's own `out/census.lock`) |
| `default_lock_path()` and `main_checkout_out()` from that worktree | Windows | `E:\open-ui\out\census.lock` and `E:\open-ui\out` |

### Shared prebuilt Skia (Windows)

`tools/skia_prebuilt.py build` compiles Skia once with the CI
`SKIA_GN_ARGS` and packs it as a rust-skia binary-cache archive under
`<shared out>/skia-prebuilt/<rev>-<inputs hash>/`; `env` points a
worktree's Cargo at it with `FORCE_SKIA_BINARIES_DOWNLOAD` and a `file://`
`SKIA_BINARIES_URL`. See
[the development guide](../../DEVELOPMENT.md#shared-prebuilt-skia-windows).

Findings:

- rust-skia's key holds the rust-skia commit, target and Skia features, not
  `SKIA_GN_ARGS` or the compiler, so the script names the archive directory
  by a hash of those as well.
- rust-skia's own export hook refuses to run on Windows (it calls the
  workspace's `freetype-woff2` feature redundant there), so the script asks
  rust-skia for the key with a forced download from a missing file, builds
  from source, and packs the files rust-skia's export would ship.
- On Linux rust-skia passes Cargo's `OPT_LEVEL` to the Skia compiler but not
  into the key, so the script is Windows-only and WSL keeps building Skia
  from source.
- Two from-source builds of the same inputs in different target directories
  already differ in bytes: every library differs only in COFF object and
  archive-member timestamps, and `skia-bindings.lib` also in the debug-info
  path of its output directory and a CodeView hash derived from it.
  `bindings.rs` and `icudtl.dat` are identical. The archive is a byte copy of
  one such build.
- The Skia compile is about 3 minutes on this 24-core host and about 140 MB
  per Skia feature set. The workspace's own Rust artifacts dominate a target
  directory (35 GB of `debug\deps` in `out\cargo-win-a8`), and they cannot be
  shared between worktrees that build concurrently.

Verification on this branch (based on `a0eae3ed`), Windows, with
`E:\open-ui\out\win-env-a8.ps1`:

| Check | Command | Result |
| --- | --- | --- |
| Build the archive | `python tools\skia_prebuilt.py build` (default `check --locked --workspace --all-targets`, target `out\skp`) | key `a31b86ba3b767344d39a-x86_64-pc-windows-msvc-jpegd-jpege-pdf-textlayout`, 29 MB archive, 7 min 10 s including the workspace check |
| Install in a fresh target | `skia_prebuilt.py env`, then `cargo test --locked --no-fail-fast -p div-paint` in `out\skc` | build script reports `DOWNLOAD AND INSTALL SUCCEEDED`; no Skia compile |
| Focused tests, source vs prebuilt | `cargo test --locked --no-fail-fast -p div-paint` in `out\skp` (`FORCE_SKIA_BUILD`) and `out\skc` (archive) | 1,284 passed, 0 failed, 1 ignored in both |
| Installed files | SHA-256 of the installed `*.lib`, `icudtl.dat`, `bindings.rs` | `out\skc` equals the archive's source build exactly |
| Every registered test rendered, source vs prebuilt | `pixel_compare render <id> <png>` for all 7,209 IDs from `pixel_compare list`, with each build's `pixel-compare` | 7,168 PNGs byte-identical, 0 different; the same 41 `css-grid` tests fail to render in both (layout panics and unbounded growth already recorded in the [grid coverage record](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/docs/progress/grid-coverage-2026-11-09.md)) |

`pixel_compare render-all` itself cannot write these IDs on Windows: it
does not create the subdirectories that IDs containing `/` need. The
comparison above renders each ID with `render` and a flattened file name
instead; that `render-all` defect is left to the engine track.
