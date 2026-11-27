# RFC 0006 B: targeted style sampling (27 November 2026)

Tier 0 item 0.7, stage B (R5) of
[RFC 0006](../alpha/rfcs/0006-rendering-pipeline.md). Branch
`pipeline/b-targeted-sampling`, code commit `3c59392` on `main` `0211362`.
SVG lane, owned by the engine track. The change is in
`crates/div-engine/src/animation.rs` only, plus a new test; no `div-style`,
`lib.rs` or public API change.

## What changed

- **A tick restyles only what changed.** Sampling used to re-run the
  authored cascade for every root whenever any effect's output changed
  (`recompute_authored_styles`). It now collects the targets whose output
  changed and restyles only those elements and the elements that inherit
  from them, in one style pass (`prepare_native_cascade_roots`, then one
  publish).
  - Only the outermost changed targets are passed, by their style parent (a
    scroll button inherits from its origin), so no element is staged before
    an ancestor that is also restyled.
  - A changed root element still restyles every tree, as before: every
    element's `rem`, in detached trees too, resolves against it.
  - An unchanged sample restyles nothing, as before.
- **A start samples only the new effect.** `start_animation` used to sample
  every effect and restyle every root, so starting N effects cost N full
  cascades. On the document timeline every other effect was already sampled
  at the current time by the clock or control call that last changed it, so
  a start now samples only the new effect and restyles only its target (and
  that target's descendants). When any effect uses a scroll or view
  timeline, a start still samples every effect, because those depend on
  layout.

## Gates

**Targeted equals full.** In debug builds, after every targeted sample the
engine resolves a full cascade of every root without publishing it, and
asserts that every element's style, and every effect target's base style,
equals the published one. A start also asserts that sampling every other
effect again would change no output and emit no event. Every test in a
debug build runs these checks, so CI's suites run them on Linux and
Windows. The check is skipped while a selector, media or font restyle is
pending: the next `update()` applies those, as it does after any other style
change.

- The check fires: with the descendants left out of the restyle (a
  temporary edit, not committed), it failed 4 of the 6 new tests.
- `div-engine/tests/targeted_sampling.rs` (new) checks which elements a
  tick and a start restyle, through the restyle log: only the changed
  targets and their descendants, each once, in one pass; nested targets
  staged outer first; an ended effect without fill restoring its target and
  descendants; a root element `font-size` effect reaching an attached and a
  detached `rem` element; and a detached target.

**Motion sample's sampled-time probe.** sample-motion's
`tests/fidelity_sampled.rs` (copied unchanged into `divui/tests` of `main`
`0211362` and of `3c59392`) writes Div's side of its Chromium comparison:
54 frames, 9 cases at 6 times. All 54 frames are byte-identical between the
two sides, so its comparison with pinned Chromium (46 of 54 when it was
recorded) is unchanged by this PR. Chromium was not run again.

## Behavior change

A tick no longer restyles elements that no effect touched. On `main`, a tick
re-cascaded every element and so applied a pending selector, media or font
restyle early; now those apply at the next `update()`, as documented for
`set_color_scheme`, interaction state and `@media`, and as after every other
style change. Frames are produced after `update()`, so they are unaffected;
a style read between a tick and the next `update()` can see the pending
state later than on `main`.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, all four profiles | `0211362` to `3c59392` | 0 of 23,728 Div outputs changed; 16,297 exact on both sides |
| CSS Grid coverage, all four profiles | `0211362` to `3c59392` | 0 of 4,204 Div outputs changed; 1,473 exact on both sides |
| Linux `div-engine`, debug (targeted-equals-full checks on) | `3c59392` | 591 passed, 0 failed, 2 ignored (76 targets) |
| Linux `divui`, `div-ffi`, `div-components`, `div-svg`, debug | `3c59392` | 590 passed, 0 failed, 8 ignored (74 targets) |
| Motion sampled-time frames, `main` against B | `0211362`, `3c59392` | 54 of 54 byte-identical |
| v0.2 and renderer contracts, format of changed files | `3c59392` | Pass |

**Census pair.** Both sides were built and measured in one run under the census lock (WSL, 16 jobs, default profiles), against the same Chromium 147.0.7727.50 binary and the same 1,051 measured CSS Grid ids, with the fidelity data at `26fec8c7`. Every Chromium output is identical between the sides; no Div output changed, and there are no errors in the census (the coverage run's 156 errors are the same tests on both sides).

**Evidence.** [div-fidelity-data](https://github.com/build-with-div/div-fidelity-data) commit `0b2f531`, archive only, under `archive/docs/progress/evidence/targeted-sampling-b-*`: the census and coverage deltas, the pair's sources (binary, receipt and summary hashes), the cost logs and binary hashes, and the 54 sampled-frame hashes. No lock bump: no gate file changed.

## Cost

WSL on a 24-thread machine (i9-12900KF), release builds under the census
lock, `main` `0211362` and B `3c59392` in separate target directories, each
checked to compile its own `div-engine` and to produce a different binary.
Probes copied unchanged from A2's cost run (sample-motion `9b37723`). Load
average 5.5 at the first round (a stopped debug build), 1.3 from the third
onwards.

**`start_cost_by_animation_count`** (three alternating rounds). Time to
build N cells and start one infinite transform animation on each, then the
first update, against building the cells alone:

| N | `main` | B | per `animate()`, `main` | per `animate()`, B |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 99.9 to 101.3 ms | 62.4 to 66.2 ms | 383 to 406 µs | within noise (−111 to 41 µs) |
| 300 | 454.6 to 461.2 ms | 71.4 to 73.3 ms | 1,294 to 1,311 µs | 8 to 19 µs |
| 1,000 | 5,887 to 6,262 ms | 118.9 to 123.0 ms | 5,798 to 6,172 µs | 28 to 31 µs |

Starting N animations is no longer quadratic in this probe.

**`measure_frame_cost`** (1280x800 at 1x, 120 frames after 10 warm-up
frames, three rounds). Every cell in this probe animates, so every target
changes each frame and a tick restyles the same elements as on `main`;
sample and total times are unchanged within the rounds' spread (at 1,000
elements, sample 12.1 to 15.1 ms on B and 12.7 to 15.5 ms on `main`). An
idle frame is 0.14 to 0.16 ms on both. The saving per tick is the elements
no effect touches, which this probe does not have.

**P7** (five alternating rounds). Div render per frame 10.24 to 10.74 ms on
`main` and 10.20 to 10.70 ms on B; the canvas probe has no animation, so no
change was expected.

**What is left at 1,000 animations.** Each restyled element still finds its
effects by scanning every effect (`animation_values_for_node`, twice), and
publishing checks every effect for an owner, so a tick of N changed targets
is still O(N²) in effect lookups. Between 100 and 1,000 elements the sample
time per element doubles (6.5 to 13 µs). A per-target effect index would
remove that; it is not in this PR. RFC 0006's 1,000-animation target
(60 frames/s) also needs A4 and C1 and is not claimed.

## Not done

- **One style pass per frame batch of starts.** Each start still restyles
  its target at once, because the API returns with styles current. With a
  start sampling only its own effect, N starts are already linear, so a
  deferred batch was not needed for the RFC's cost goal.
