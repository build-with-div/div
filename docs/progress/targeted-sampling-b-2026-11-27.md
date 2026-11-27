# RFC 0006 B: targeted style sampling (27 November 2026)

Tier 0 item 0.7, stage B (R5) of
[RFC 0006](../alpha/rfcs/0006-rendering-pipeline.md). Branch
`pipeline/b-targeted-sampling`, code commits `3c59392` and `62c0cba` (the
engine review's fix) on `main` `0211362`, then rebased onto `main`
`087de2b` as `4dc589f` and `2bbf298` with the code patch ID unchanged
(only `docs/progress/tracks/engine.md` conflicted).
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
- **With a restyle pending, sampling restyles every root, as before.** A
  pending selector restyle finds the elements whose state changed by
  comparing their state with the state their style was last resolved with,
  and restyling an element records its current state. So while a selector,
  media or font restyle waits for the next `update()`, a tick or a start
  re-cascades every root as `main` does, instead of only its targets
  (`restyle_pending`).

## Gates

**Targeted equals full.** In debug builds, after every targeted sample the
engine resolves a full cascade of every root without publishing it, and
asserts that every element's style, and every effect target's base style,
equals the published one. A start also asserts that sampling every other
effect again would change no output and emit no event. Every test in a
debug build runs these checks, so CI's suites run them on Linux and
Windows. The full-cascade check runs after every targeted sample: with a
restyle pending, sampling takes the full path instead. Each check costs a
full cascade of the document, in debug builds only. It slows the debug
suites by no measurable amount: `div-engine`'s debug suite took 437 and
452 s of test time at `62c0cba` against 450 and 464 s at `main`
`0211362` (two alternating rounds on the WSL host, 2 jobs, compile time
excluded; B also runs 8 more tests).

- The check fires: with the descendants left out of the restyle (a
  temporary edit, not committed), it failed 4 of the 6 new tests.
- `div-engine/tests/targeted_sampling.rs` (new) checks which elements a
  tick and a start restyle, through the restyle log: only the changed
  targets and their descendants, each once, in one pass; nested targets
  staged outer first; an ended effect without fill restoring its target and
  descendants; a root element `font-size` effect reaching an attached and a
  detached `rem` element; and a detached target.
- Two tests added in review check `.a:hover ~ .b` (Tailwind's
  `peer-hover:`) with `.a` animated: hovering `.a`, then a tick or a start
  before `update()`, must still turn `.b` red. Both failed at `4d5c06c`,
  where a targeted sample recorded `.a`'s new hover state and the pending
  restyle then saw no change; both pass with the full path above. The
  engine track found this in review.

**Motion sample's sampled-time probe.** sample-motion's
`tests/fidelity_sampled.rs` (copied unchanged into `divui/tests` of `main`
`0211362` and of `3c59392`) writes Div's side of its Chromium comparison:
54 frames, 9 cases at 6 times. All 54 frames are byte-identical between the
two sides, so its comparison with pinned Chromium (46 of 54 when it was
recorded) is unchanged by this PR. Chromium was not run again.

## Behavior

No behavior change from `main` is intended. With no restyle pending, every
debug test checks that the targeted result equals a full cascade; with one
pending, sampling takes `main`'s full path.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, all four profiles | `0211362` to `62c0cba` | 0 of 23,728 Div outputs changed; 16,297 exact on both sides |
| CSS Grid coverage, all four profiles | `0211362` to `62c0cba` | 0 of 4,204 Div outputs changed; 1,473 exact on both sides |
| Census and CSS Grid coverage, first pair (superseded) | `0211362` to `3c59392` | 0 of 23,728 and 0 of 4,204 Div outputs changed |
| Linux `div-engine`, debug (targeted-equals-full checks on) | `62c0cba` | 593 passed, 0 failed, 2 ignored (76 targets) |
| Linux `div-engine`, `divui`, `div-ffi`, `div-components`, `div-svg`, debug | `62c0cba` | 1,183 passed, 0 failed, 10 ignored (150 targets) |
| Motion sampled-time frames, `main` against B | `0211362`, `62c0cba` | 54 of 54 byte-identical (also at `3c59392`) |
| Linux `div-engine` and `divui`, debug, rebased on `087de2b` | `2bbf298` | 980 passed, 0 failed, 10 ignored (140 targets) |
| v0.2 and renderer contracts, format of changed files | `2bbf298` | Pass |

**Census.** The review fix changed the code patch ID, so the first pair
(`3c59392`, evidence `0b2f531`) does not carry. `62c0cba` was measured
again on its own, under the census lock, and compared with the `main`
`0211362` base measured in the first pair's run: the same WSL host (i9-12900KF,
16 jobs, default profiles, no `--profile`), the same Chromium
147.0.7727.50 binary (sha256 `0ba87684df0afc11faaea40ac57bd2f1d68b67191443b38215a36b90e6f08848`),
the same oracle cache, the same 1,051 measured CSS Grid ids and the
fidelity data at `26fec8c7`, as A1's final-head remeasure was. Every
Chromium output is identical between the sides, no Div output changed,
there are no errors in the census, and the coverage run's 156 errors are
the same tests on both sides. The rebase onto `087de2b` left the code
patch ID unchanged, so this result carries to the rebased head; it was
measured without #159's and #160's code.

**Evidence.** [div-fidelity-data](https://github.com/build-with-div/div-fidelity-data)
commit `371507a`, archive only, under
`archive/docs/progress/evidence/targeted-sampling-b-*-v2*`: the census and
coverage deltas, the remeasure's sources (binary, receipt and summary
hashes, with the base's), the 54 sampled-frame hashes at `62c0cba` and the
debug suite timing. The first pair's files (`-v1`, commit `0b2f531`) are
superseded. No lock bump: no gate file changed.

## Cost

WSL on a 24-thread machine (i9-12900KF), release builds under the census
lock, `main` `0211362` and B `3c59392` in separate target directories, each
checked to compile its own `div-engine` and to produce a different binary.
Probes copied unchanged from A2's cost run (sample-motion `9b37723`). Load
average 5.5 at the first round (a stopped debug build), 1.3 from the third
onwards.
The cost was measured at `3c59392` and not again: the review fix changes
only the path taken while a restyle is pending.

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

- **A double restyle in one rare case** (engine review). A start while a
  restyle is pending can restyle every root twice: once through sampling
  and once explicitly. `main` did comparable work there.
- **One style pass per frame batch of starts.** Each start still restyles
  its target at once, because the API returns with styles current. With a
  start sampling only its own effect, N starts are already linear, so a
  deferred batch was not needed for the RFC's cost goal.
