# Paint chunks and damage (23 November 2026)

RFC 0006, step A1 of Tier 0 item 0.7. Every recording now carries paint
chunks, and two recordings can be compared for the region that may differ.
Nothing presents with this damage yet: the presenter still rasters and
presents the whole frame, so pixels are unchanged. A2 uses the damage for
partial tile raster.

## What changed

- `crates/div-paint/src/damage.rs` (new) defines `PaintChunk` and
  `PaintChunks`. A chunk is a maximal run of draws by one owner, keyed by
  owner and occurrence. Its bounds are the draws' paint bounds, mapped by the
  device matrix, outset by one pixel and clipped. Its fingerprint hashes the
  paint state, matrix, clip and operands.
- Operands hash by value where Div owns them. Effects (shaders, filters and
  path effects) hash by pointer and stay alive with the recording, so a
  recycled address cannot alias. Images and pictures hash by unique id. Text
  blobs hash by content (font, glyphs and positions) through the new
  `crates/div-text/src/blob_fingerprint.rs`, which the three blob builders
  register.
- Nested pictures registered by the painter splice their chunks into the
  outer stream. An unregistered picture is one chunk bounded by its clip.
- A layer with a single draw folds its shape into that draw's chunk.
- `SceneSnapshot::physical_damage_since(&previous)` in `div-compositor`
  returns the union of old and new bounds of every changed, added or removed
  chunk, plus chunks that moved out of the longest kept order. A change of
  viewport, scale or configuration, or an incomplete recording, damages the
  whole viewport.
- `pixel_compare damage-coverage` (new, in `tests/fidelity/pixel-compare`)
  renders each fixture, applies 14 mutations one at a time, and fails if any
  pixel outside the reported damage differs between full re-records before
  and after the mutation. `crates/div-engine/tests/paint_damage.rs` covers the
  same contract in four unit tests.
- `docs/renderer/generated/author-style-inventory.json` is regenerated. It
  now lists `damage.rs` (and `render.rs` for two of them) as consumers of
  `bottom`, `clear`, `left`, `right` and `top`. The inventory matches field
  names in the source text; these are rectangle fields and a
  `clear()` call on a thread-local table, not style reads.

## Deviations from RFC 0006

- **Fingerprint.** The RFC describes a hash of the serialized `SkPicture`.
  A1 hashes state, matrix, clip and operands instead, which needs no
  serialization and gives the same answer for the draws Div records.
- **Pairwise damage.** The RFC says A1 replaces a scene's own damage. A
  scene's `damage()` and `physical_damage()` stay the conservative full
  viewport. Damage is pairwise, `physical_damage_since(&previous)`, so A2's
  presenter must pass the scene it last presented.
- **Picture shaders** hash by pointer, so a re-created picture shader with the
  same content damages its chunk. This over-damages locally and never
  under-damages.
- **Inline SVG** is not yet wired into `div-paint` or `div-engine` (RFC
  0009), so A1 has nothing to chunk there. When it lands, each SVG element
  must enter a `PaintOwnerScope`, as HTML boxes do.

## Public API

A1's first revision made the chunk types public. The user chose the narrower
version, answering "Do you approve my narrower version, or each as it is?"
with "yes approve". Commit `9d705b2` makes that change:

- `ChunkOwner`, `PaintChunk` and `PaintChunks`, with their fields and
  methods, are `pub(crate)` in `div-paint`, and the root re-export is gone.
- `RecordedPicture::paint_chunks` and `PaintChunks::is_complete` had no
  callers and are removed.
- `div-text`'s `blob_fingerprint` module is `#[doc(hidden)]`. `div-paint`
  still calls `BlobFingerprint`, `text_blob_fingerprint`,
  `clear_text_blob_fingerprints` and `hash_font`, so they stay `pub` inside
  it. `9d705b2` first made `hash_font` private, on the wrong belief that only
  its own file used it. `div-paint`'s `damage.rs` hashes the font of string
  draws with it, so CI failed to compile, and the next commit makes it `pub`
  again.
- `RecordedPicture::damage_since`, which `div-compositor` calls, and
  `SceneSnapshot::physical_damage_since`, which A2's presenter will call,
  stay public.

The change only alters visibility. It was not built locally, because drive
E: was below its free-space floor; CI builds and tests it.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Damage gate, Linux, 1x and 2x | `0cdd653` on `main` `271415b` | Pass, 100,898 steps per scale, 0 with uncovered pixels, 0 fixture errors, 2 known aborts |
| Damage gate, Windows, 1x and 2x | `974962a` (final head) on `main` `4dc219e` | Pass, 100,898 steps per scale, 0 with uncovered pixels, 0 fixture errors, 2 known aborts |
| Damage gate, Windows, 1x and 2x | `f488add` (earlier revision) | Pass, 104,860 steps per scale, 0 with uncovered pixels, 4 aborted fixtures |
| Census, all four profiles | `4dc219e` (engine's run) to `974962a` | 16,297 of 23,728 exact before and after, 0 tests with a changed status or image hash |
| CSS Grid coverage, all four profiles | `4dc219e` (engine's run) to `974962a` | 1,473 of 4,204 exact before and after, 156 errors before and after, 0 tests changed |
| Census, all four profiles | `4e69bdf` to `7c816ac` | 16,297 of 23,728 exact before and after, every status, mismatch and Div image hash identical |
| Census, real zoom subset | `4e69bdf` to `7c816ac` | 10,404 of 17,796 exact before and after, identical |
| CSS Grid coverage, all four profiles | `4e69bdf` to `7c816ac` | 1,473 of 4,204 exact before and after, 156 errors before and after, identical |
| CSS Grid coverage, real zoom subset | `4e69bdf` to `7c816ac` | 894 of 3,153 exact before and after, identical |
| Linux `div-paint`, `div-engine`, `div-text`, `div-compositor`, `div-svg` | `0cdd653` on `main` `271415b` | 3,047 passed, 0 failed, 2 ignored |
| The same crates | `e0a80c2` on `main` `c2ad60a` | 3,051 passed, 0 failed, 2 ignored |
| Skia flag check, v0.2 and renderer contracts, `fidelity_data.py verify`, format of changed files | final head | Pass |

The damage gate's fixture crate (`pixel-compare`) was built unoptimized
(`--config profile.release.package.pixel-compare.opt-level=0`), with the
engine crates in release, as the census builds it. The gate's comparison code
and thresholds are the ones in this PR; there is no tolerance: a single
differing pixel outside the damage fails the step.

Linux gate per mutation at 1x (damaged area and changed area, as a share of
the viewport, over 7,207 steps each): background-color 64.7% and 31.5%,
color 4.9% and 0.0%, opacity 68.2% and 33.6%, margin-left 42.0% and 2.4%,
width 40.7% and 29.5%, font-size 7.9% and 0.3%, append 22.3% and 0.5%,
scroll 11.6% and 0.4%, animation 12.3% and 6.4%, display none 23.3% and
10.3%, display block 23.4% and 10.1%, remove 23.4% and 10.1%. 522 steps per
scale damage the full viewport.

**Known aborts.** `grid-definition_grid-support-named-grid-lines-002-ref`
and `-003-ref` abort the gate's fixture process (exit -6, memory allocation
failure under the gate's 8 GB address-space cap). In unoptimized builds
they first hit `attempt to subtract with overflow` at
`crates/div-layout/src/grid.rs:350`, which predates A1 and belongs to the
engine track. They count as aborts only with that allocation message; any
other abort is no result. On Windows, at `f488add`, those two references and
their test pages (`-002` and `-003`) aborted with exit
`0xC0000409`. At `974962a` only the two references aborted on Windows, with
the same exit, at both scales.

The Linux gate's Skia build used `main`'s Linux flags, including
`-USK_ENABLE_AVX512_OPTS`, read back from its `args.gn`.

The receipts were added to the data repository at `fcbb6f3e`, under
`docs/progress/evidence/paint-chunks-a1-*`: the census and coverage pair,
the gate receipt and log, and the frame-cost and P7 logs with their
conditions and binary hashes. `main`'s pin, `b6c41deb`, already contains
that commit, so this change does not move the lock.

**Census pair on the final head.** The earlier pair, `4e69bdf` against
`7c816ac`, predates #120, #131, #132, #134, #135, #136, #137, #139 and the
Linux `-O2` flags of #142. The code patch is unchanged by every rebase:

```text
git diff <base> <head> | git patch-id --stable
4e69bdf..7c816ac  35a50ffec9c76fb5e1967617954f374c960ea44a
271415b..0cdd653  35a50ffec9c76fb5e1967617954f374c960ea44a
c2ad60a..e0a80c2  35a50ffec9c76fb5e1967617954f374c960ea44a
dde5083..33255b7  35a50ffec9c76fb5e1967617954f374c960ea44a
4dc219e..ab3a6ed  35a50ffec9c76fb5e1967617954f374c960ea44a
```

So that pair carries to `ab3a6ed`, A1's code on `main` `4dc219e`. The
narrowing after it (`9d705b2` and the `hash_font` fix) was checked first by a
masked byte comparison: release `pixel_compare` built at `efa3f8a` (A1
before the narrowing, on `4dc219e`) and at `974962a` in one WSL clone and
one target directory, with the Skia flags of `ci.yml` (identical on both
sides), masking only the embedded build-identity block. The binaries were
not comparable: 472,848,424 and 472,849,984 bytes (sha256 `5576df71...` and
`ddf8c202...`), with the identity block at different offsets. The narrowing
changes the binary, so a full pair was run on `main` `4dc219e`.

The base side of that pair is the engine track's run of `main` `4dc219e` for
#149, used because its recipe is the same: the same commit, data pin
`b6c41deb`, Linux Skia flags read from `ci.yml` with no override (`-O2`
included), Chromium 147.0.7727.50 from the shared oracle cache, 16 jobs, no
`--profile` argument, and a byte-identical list of 1,051 CSS Grid coverage
IDs (sha256 `179fe661...`). Before measuring the head, the run checked that
both base summaries were complete, at `4dc219e`, with the head's flags and
that ID list. Base files, which this change did not produce:

```text
/mnt/f/div-out/engine/census-results/expanded-m4dc2/expanded-summary.json  41a6240fd34f28688f203285fed8c5029bb0663d16872eb277a21b7560d3b25d
/mnt/f/div-out/engine/census-results/coverage-m4dc2/coverage-summary.json  e7ee6727960525c7b49e1a299def0b014582376ae3bc49ad41dcb35139f61ffe
pixel_compare  89b154c367fa98916488f158e1b6b9c47975b46b46d3d7b6778b3abe112a0508
receipt.json   6fa6f267448acd40c71a5373801e830e7290a7bc1f5748e95971a053f7eedf5d
```

The head, `974962a`, was built and measured by this track: `pixel_compare`
`ef0437bb55ad90aa0fff39a3aa09cbb2d807021fb41dd6c8dc5b2e0f57ae970d`, receipt
`b1068fdcae6669a5477f1a9a2832973ae210d364c60c3a45602885f94bfbb1ab`. Across
the 23,728 census results and 4,204 coverage results, every test's status,
Div image hash and Chromium image hash is identical. The only per-test field
that differs is the cache identity, which is derived from the build. The
Chromium, raster, contract, resource and font hashes are the same on both
sides. The pair covers the default profiles only, not the real-zoom subset.
Its receipts are in the data repository at `b1cd0d57`, under
`docs/progress/evidence/paint-chunks-a1-{census,coverage}-v2.json`, with
`paint-chunks-a1-pair-v2-sources.json` holding both sides' binary,
build-receipt and summary hashes. The full summaries and the build receipts
are not there: the receipts hold local paths, and the base's receipt file no
longer exists. The data commit is archive-only, so `fidelity-data.lock` was
not moved for it.

**Rebase onto `main` `e9b6768`.** After these measurements the branch was
rebased onto `e9b6768`, which adds #151, #152 and #149. The results above
carry across that rebase under D4 without being rerun, for these reasons:

```text
code only (git diff <base> <head> -- . ':!docs' | git patch-id --stable)
4dc219e..974962a  d6d83efde0d1c4fdb2fbfbf3565462d136916a92
e9b6768..a8e7809  d6d83efde0d1c4fdb2fbfbf3565462d136916a92
per commit (git show <commit> | git patch-id --stable)
ab3a6ed, a9d80bd  35a50ffec9c76fb5e1967617954f374c960ea44a  A1 code
a44c29b, 981dd89  1a265ac6d7f72d587491a318c5dac2e8f3a53fcc  inventory
9d705b2, ee043bf  5708747aa1b54edb67b3262205888d66f17597a7  narrowing
974962a, a8e7809  5e00cedce3d1219270367d9b058dbf94756707d8  hash_font
```

- **Generated output.** The regenerated author style inventory adds and
  removes byte-identical lines at both bases; only the hunk line numbers
  moved, because #149 added entries. `generate_renderer_contract.py --check`
  passes at the rebased head with no drift, and so does the v0.2 contract
  check.
- **No overlap.** A1 touches `div-compositor`, `div-paint`, `div-text`, a
  `div-engine` test and `pixel-compare`. The new code on `main` is in
  `div-style`, `div-engine`'s cascade and accessibility, `div-macros`,
  `divui`, `div-cli` and the workflows. The only conflict was this track's
  file, `docs/progress/tracks/engine.md`, where both entries were kept.
- **Not covered.** #152 compiles Skia with four Chromium raster defines on
  Linux and Windows. The census pair, the Linux gate and the Windows gate
  above were all measured without them, and the carry assumes A1 does not
  interact with them. The rebased head was not built or tested locally;
  CI runs it.

#136 and #143 change `div-text` font lifetimes next to A1's text
fingerprints, so the crate tests were rerun on `0cdd653` and `e0a80c2`. The
later rebases, onto `dde5083` and `4dc219e`, cross #147, which changes how
`div-text` sums run widths, #142, which builds Linux Skia at `-O2`, and #150.
The crate tests were not rerun on `ab3a6ed` or `9d705b2`, because drive E: was below its free-space floor; CI
runs them.

## Cost

A1 records chunks on every frame and does not yet save any raster, so it
adds recording cost. All measurements are WSL on a 24-thread machine, release
builds, under the census lock, with main `4e69bdf` and A1 `7c816ac` built
in separate target directories and checked to differ.

**`measure_frame_cost`** (sample-motion `9b37723`, 1280x800 at 1x, 120 frames
after 10 warm-up frames, three alternating rounds, load average 3 to 8).
Median update time at 1,000 animated elements rises 0.70 to 0.90 ms:
transform 5.05 to 5.77, opacity 4.21 to 4.91, background-color 3.35 to
4.25, width 4.91 to 5.77 ms. Raster time is flat (1 of 16 rows separated
across rounds). The ratio of total median time, A1 to main, is 0.94 to 1.19
over the 16 rows, median 1.04; only background-color at 1,000 elements is
slower in every round (17.69 to 18.83 ms).

**`start_cost_by_animation_count`** (same probe). Per-animation start cost
ranges overlap between main and A1 at 100, 300 and 1,000 animations, so no
difference is shown.

**P7** (sample-canvas probe, 640x480 canvas in a 1024x768 viewport, 65
frames, five alternating rounds, load average about 1.2). Median Div render
per frame 11.36 ms on main and 11.21 ms on A1 (ranges 10.99 to 11.68 and
10.85 to 11.70); unchanged-document render 2.50 and 2.40 ms (ranges overlap);
draw and push times equal. The probe's crate was copied unchanged except for
one unrelated line in P6, `canvas.set_pointer_capture(PointerId(0))`, which
no longer compiled against `main`.

RFC 0006's frame targets (one changed square under 1 ms) need A2 and A3 and
are not claimed here.

**Windows.** The Windows suites run in CI. The Windows damage gate was
rerun on the final head, `974962a`, with `main`'s Windows Skia flags read
from the `windows-msvc` job of `ci.yml` (including
`/clang:-ffp-contract=off`, checked in the build's `args.gn`), under the
census lock. It used MSVC toolset 14.52.37025 from Visual Studio 18 Build
Tools, not CI's 14.44, which is no longer installed. Skia's ninja ran through
a `SKIA_NINJA_COMMAND` wrapper with `-j4`; the wrapper does not log each
call on Windows. The fixture crate was unoptimized, as on Linux. The earlier
Windows run at `f488add` used the flags of that time, before #132 and #112.
