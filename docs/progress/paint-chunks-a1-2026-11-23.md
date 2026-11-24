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
  still calls `BlobFingerprint`, `text_blob_fingerprint` and
  `clear_text_blob_fingerprints`, so they stay `pub` inside it. `hash_font`
  is private.
- `RecordedPicture::damage_since`, which `div-compositor` calls, and
  `SceneSnapshot::physical_damage_since`, which A2's presenter will call,
  stay public.

The change only alters visibility. It was not built locally, because drive
E: was below its free-space floor; CI builds and tests it.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Damage gate, Linux, 1x and 2x | `0cdd653` on `main` `271415b` | Pass, 100,898 steps per scale, 0 with uncovered pixels, 0 fixture errors, 2 known aborts |
| Damage gate, Windows, 1x and 2x | `f488add` (earlier revision) | Pass, 104,860 steps per scale, 0 with uncovered pixels, 4 aborted fixtures |
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
`0xC0000409`.

The Linux gate's Skia build used `main`'s Linux flags, including
`-USK_ENABLE_AVX512_OPTS`, read back from its `args.gn`.

The receipts were added to the data repository at `fcbb6f3e`, under
`docs/progress/evidence/paint-chunks-a1-*`: the census and coverage pair,
the gate receipt and log, and the frame-cost and P7 logs with their
conditions and binary hashes. `main`'s pin, `b6c41deb`, already contains
that commit, so this change does not move the lock.

**What the census pair and the gate do not cover.** The census pair was
measured at `4e69bdf` against `7c816ac`, before #120, #131, #132, #134,
#135, #136, #137 and #139, and was not rerun on the new Skia flags. The
Linux gate ran at `0cdd653` on `main` `271415b`, before #112 (Windows Skia
flags only), #140, #141, #126, #133, #143, #145, #147, #142 (Linux Skia at
`-O2`) and #150. The code patch is unchanged by every rebase:

```text
git diff <base> <head> | git patch-id --stable
4e69bdf..7c816ac  35a50ffec9c76fb5e1967617954f374c960ea44a
271415b..0cdd653  35a50ffec9c76fb5e1967617954f374c960ea44a
c2ad60a..e0a80c2  35a50ffec9c76fb5e1967617954f374c960ea44a
dde5083..33255b7  35a50ffec9c76fb5e1967617954f374c960ea44a
4dc219e..ab3a6ed  35a50ffec9c76fb5e1967617954f374c960ea44a
```

`9d705b2` (above) comes after `ab3a6ed` and changes the patch ID. Whether
the census pair carries across it is decided by a byte comparison of the
`pixel_compare` binaries built at `efa3f8a` (A1 before the narrowing, on
the same `main`) and at the final head, once drive E: has space again;
until then the pair covers `ab3a6ed`'s code only.

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

**Not run locally.** The Windows suites run in CI. The Windows damage gate
ran at `f488add`, an earlier revision of this change, and not on the final
head. It used the Windows Skia flags of that time, before #132 and #112. The
gate compares renders made by one binary, so the flags change which pixels it
sees, not whether the damage covers them.
