# RFC 0005: Translating WPT testharness scripts into Div tests

Status: proposed, 13 November 2026. Owner: components track, taking item
0.13 (T3) of the platform plan for the engine-tools lane. Consumers: the
engine and framework tracks, for A1–A3 (animations), O1–O3 (observers), I1
(Pointer Events), R1 (`requestAnimationFrame`) and C2 (Canvas 2D). Asks the
engine track to agree, because it owns `tools/wpt` and the qualification
tooling.

## Problem

Most WPT tests for the coming platform features are `testharness.js`
scripts. Div's WPT tooling ports only static pages. It builds the page in
Rust and compares its pixels with Chromium's, and it rejects scripts on
purpose:

- `tools/wpt/javascript_mutation_ir.mjs` rejects `test`, `promise_test`,
  `assert_*`, timers, `requestAnimationFrame`, `await` and event synthesis,
  each with a named reason (`:14-23`, `:420-428`).
- The porter freezes every animation at 0 ms (`port_wpt.py:7657-7663`).
  The motion sample found that none of the 91 portable animation tests
  exercises the animation engine.
- The generated builders call engine internals (`FixtureEngine`,
  `base_doc`), not the public `divui` crate. `requestAnimationFrame`,
  `ResizeObserver` and Pointer Events will be `divui` APIs.

So none of these areas can be measured today:

| Area | WPT directories at Chromium `09d377d9` | Size |
| --- | --- | --- |
| A1, A2 | `css/css-transitions`, `css/css-animations`, `css/css-easing` | 118, 112 and 12 testharness files |
| A3 | `web-animations` | 233 files including support files |
| A4 | the `*-interpolation.html` files under `css/` (`test_interpolation`) | 327 files, 1,137 blocks (motion sample) |
| R1 | `html/webappapis/animation-frames`, `html/webappapis/update-rendering` | 13 and 2 files |
| O1, O2 | `resize-observer`, `intersection-observer` | 32 and 145 files |
| O3 | `dom/nodes/MutationObserver-*` | 14 files |
| I1 | `pointerevents` | 232 files |
| C2, C3 | `html/canvas/tools/yaml/*.yaml` | 19 files, 1,157 tests (canvas sample) |

File counts come from `git ls-tree` at `09d377d9` and include helpers.
PR 2 replaces them with per-test and per-subtest counts.

## Proposal

A generator reads a WPT test and writes a Rust test against the public
`divui` crate. The Rust test advances a virtual clock frame by frame and
checks every assertion the WPT test makes. A ledger records the outcome of
every upstream subtest, including the ones that were not translated.

### 1. Front end

- **Reuse.** The front end uses the same pinned Acorn (`CHROMIUM_ACORN`,
  `acorn@8.14.0` with a checked sha256) and the same WPT root
  (`CHROMIUM_WPT_ROOT`) as today's tools.
- **Separate file.** It is a new module and does not edit
  `javascript_mutation_ir.mjs`. That file's output is pinned by the
  renderer contract (`javascript-mutation-audit-v4.json`), so extending it
  would change a checked artifact. The plan's T3 row says "extend"; this is
  the one deviation.
- **Same interpretation approach.** Constant branches are evaluated, bounded
  loops are unrolled and named functions are inlined, as in the existing
  front end. The resulting script must be straight-line.
- **What it accepts:**
  - `test`, `async_test`, `promise_test` and `setup`, plus `t.step_func`,
    `t.add_cleanup` and `t.step_timeout`.
  - `async` and `await`, and `.then` chains.
  - Event listeners whose bodies are themselves translatable.
- **Rejections.** Anything else rejects the whole subtest, with a reason in
  the existing vocabulary (`runtime-dependent-control-flow`, and so on).
  The generator never drops a statement or an assertion.
- **Helper libraries as intrinsics.** WPT's helper libraries are recognised
  by path and sha256 rather than interpreted:
  - `testharness.js`
  - `EventWatcher`
  - `web-animations/testcommon.js` (`createDiv`, `waitForNextFrame`,
    `waitForAnimationFrames`, `assert_times_equal`)
  - `css/css-transitions/support/helper.js`
  - `css/support/interpolation-testcommon.js`
  - `resize-observer/resources/resizeTestHelper.js`
  - `dom/nodes/mutationobservers.js`
  - `pointerevents/pointerevent_support.js`
  - `testdriver.js` and `testdriver-actions.js`

  If an upstream helper changes, its hash no longer matches and
  `--check` fails until someone reviews the change.
- **Page setup.** The test page's markup and `<style>` become
  `divui` element creation and the public style API. Because the output is
  Rust source, CSS text known when the test is generated can go through a
  compile-time style macro. CSS built while the test runs needs runtime
  sheets (plan Q3).

### 2. Capability table: no API is invented

`capabilities.json` maps each web surface a test uses to its plan feature
and to the `divui` call that implements it, or to `null` while that call
doesn't exist. Some entries:

| Web surface | Feature | Div today |
| --- | --- | --- |
| `requestAnimationFrame` | R1 | `null` |
| `transition-*` in style rules | A1 | `null` |
| `Element.animate` with one property | A3 | `Element::animate` |
| `animation.ready`, `startTime` | A3 | `null` |
| `ResizeObserver` | O1 | `null` |
| `test_driver.Actions` | T1, I1 | `null` |
| `getContext('2d')` | C1, C2 | `null` |

The generator emits a Rust test only when every surface a subtest uses has
a `divui` call. Otherwise the subtest is recorded as
`blocked: [features]`. When a feature lands, a follow-up PR fills in its
entry and regenerates, and the ledger diff shows the newly translated
subtests with their results.

The table never maps a surface to a near equivalent. One example: Div
starts an animation immediately, with no pending phase, so `await
animation.ready` stays blocked on A3 rather than becoming "advance one
frame". Making that substitution would hide a fidelity difference that the
tests exist to measure.

### 3. Runtime and frame advance

The generated tests use a small runtime module next to them. It runs on
`TestApp` and `Document`, and has these parts:

- **Virtual clock.**
  - Frame *n* runs at *t* = *n* × 1000/60 ms, set with
    `Document::begin_frame(t)`. Div's animation clock is already virtual:
    Headless, `Engine::set_animation_time` is fed only by
    `Document::advance_time` and `begin_frame`. The runtime never calls
    `TestApp::render_png`, which renders at 0 ms.
  - `performance.now()` and `document.timeline.currentTime` read this
    clock.
- **Event loop.** Each frame does three things, in order:
  1. It runs the timers that are due on the virtual clock. These are
     `setTimeout` and `step_timeout`. `divui`'s own `time::sleep` uses the
     wall clock, so the runtime keeps its own timer queue.
  2. It runs Div's rendering step through the frame API that R1 adds:
     update animations and send events, then animation frame callbacks,
     style and layout, resize observations, paint, and intersection
     observations, as in HTML's "update the rendering".

     The runtime calls that step and never reorders it, so the
     `update-rendering` and `animation-frames` tests measure Div's own
     order. Until R1 lands, only `begin_frame` is available and the R1
     tests are blocked.
  3. It runs the promise continuations queued by the awaits that frame
     satisfied.
- **What rAF means here.** `waitForNextFrame` and
  `waitForAnimationFrames(n)` are harness waits: they advance whole
  frames. A test's own call to `requestAnimationFrame` calls Div's R1 API,
  through one runtime function, so settling the API shape (plan Q4)
  changes only that function.
- **Awaits.** Each `await` blocks until its condition holds, advancing
  frames as it goes. The conditions are a frame, an event, a promise
  settling or a timer. Every wait has a limit of 10 s of virtual time,
  WPT's default test timeout. A subtest that reaches the limit times out,
  and a timeout counts as a failure.
- **Events.**
  - Listeners use `Element::on`.
  - `EventWatcher` fails on an unexpected event, as it does upstream.
  - `dispatchEvent` uses `Document::dispatch_event`.
  - Pointer actions become sequences of `Document::dispatch_pointer_event`
    calls, with a frame between ticks (T1).
- **Assertions.**
  - Assertions keep WPT's semantics and tolerances:
    `assert_approx_equals` uses the test's own epsilon, and
    `assert_times_equal` uses `TIME_PRECISION`. Neither is ever widened.
  - Div has no CSSOM serialization, so `getComputedStyle(el).x` can't be
    compared as a string. The runtime compares the typed computed value
    with the expected string parsed by Div's own parser instead
    (`parse_literal`). A subtest that relies on this is counted as
    `pass-typed`, kept apart from `pass`, until serialization exists.

    Interpolation tables are exact without serialization:
    `test_interpolation` compares the animated value with the computed
    value of a reference element in the same engine.
- **Canvas.** Canvas YAML tests go through the same front end, with
  `@assert` macros lowered to pixel reads on Div's canvas. Examples are
  `@assert pixel 25,25 == 0,255,0,255;` and the `==~` tolerance form, with
  the tolerance the YAML gives. Call-trace replay against Chromium (plan
  Q10) stays a separate oracle.

### 4. Results without weakening any gate

- **Ledger.** Each suite has a checked-in ledger, generated from WPT at
  `09d377d9` and the capability table.
  - One row per upstream subtest, with a status: `pass`, `pass-typed`,
    `fail` with its message, `blocked` with its features, `untranslatable`
    with its reason, or `excluded`.
  - `excluded` covers cross-origin, `idlharness`, manual tests and iframes
    until they are supported.
  - Each row also records Chromium 147's result. A subtest passes in
    Chromium unless the pinned web tests contain an `-expected.txt` file
    for it or a `TestExpectations` entry. There are 6 `-expected.txt` files
    in css-transitions, 9 in css-animations, 23 in web-animations and 18 in
    pointerevents.
- **Counts.**
  - Denominators are always the upstream subtests.
  - Headline counts are `pass` only, with "matches Chromium" next to them.
  - `pass-typed`, `blocked` and `untranslatable` are reported, never merged
    into `pass`.
- **Expectations in CI.**
  - Every translated subtest runs as a Rust test whose expected outcome
    comes from the ledger, as in Chromium's own expectations.
  - A regression fails CI.
  - A new pass also fails CI until the ledger records it, so every claimed
    pass is one that was measured.
- **Reproducibility.** `--check` regenerates the Rust tests and the ledger
  from the pinned inputs and fails on any difference.
- **Separate from the census.** These results are kept apart from the
  renderer census and the coverage manifests. They change no exactness
  gate, no frozen sprint ledger and no baseline. Census sample-at-time (the
  rest of plan row T3) changes the census, so it stays with the engine
  track (decision D4) and is not part of this RFC.

### Location

- **Generator.** `tools/wpt_script/`, a new directory, with Python tests
  run by `python-accountability`.
- **Generated tests.** `tests/wpt-script/`, a new unpublished crate,
  `div-wpt-script`, that depends on `divui` with the `testing` feature
  only.
- **CI.** `ci.yml` adds the sparse checkout paths these suites need:
  - `wpt/web-animations`, `wpt/resize-observer`,
    `wpt/intersection-observer`, `wpt/pointerevents` and `wpt/dom/nodes`
  - `wpt/html/canvas/tools`
  - `wpt/html/webappapis/animation-frames` and
    `wpt/html/webappapis/update-rendering`
  - `wpt/resources`

## Affected tracks

- **Engine.**
  - Agrees that `tools/wpt` and the renderer contract stay untouched, and
    that the new tool lives beside them.
  - Owns census sample-at-time.
  - Gets ledgers it can cite as the measurement for A1, A2, A4 and C2.
- **Framework.** No API change is requested. The runtime adapts to R1's
  frame API, T1's input helpers and the observer APIs as they land. Two
  things would let more subtests translate:
  - A per-listener removal handle. `Element::remove_event` removes every
    listener of a type, but `EventWatcher` removes only its own.
  - Computed-value serialization, which retires `pass-typed`. This needs an
    engine part as well.
- **Setup.** Adds `tools/wpt_script` and `tests/wpt-script` to the
  ownership table under the components track.
- **Components.** Runs the work below.

## Migration

One PR at a time, each merged by hand on green CI, rebased after any Rust,
tools or workflow change on `main`.

1. **This RFC.** It changes no code.
2. **Front end, intrinsics, capability table and ledgers** for
   css-transitions, css-animations, css-easing and web-animations, plus
   `--check` in CI. It produces no Rust yet, and it gives the first exact
   counts of what is blocked on which feature.
3. **The `div-wpt-script` crate and Rust code generation** for the subtests
   whose surfaces exist today. That is expected to be a small set: most
   tests need style rules (S1, A1, A2) or the WAAPI surface (A3).
4. **The interpolation-table generator** (`test_interpolation`), starting
   with its Web Animations method.
5. **The canvas YAML generator.** Every test stays blocked on C1 and C2
   until `div-canvas` exists.
6. **The R1, O1–O3 and I1 suites.** Each one is translated as its feature
   lands. Its ledger is generated in step 2's format, so it is measurable
   from the feature's first PR.

`main` stays green between these PRs. Each one adds new paths and CI steps
and changes no existing test.
