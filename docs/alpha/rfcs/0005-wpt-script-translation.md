# RFC 0005: Translating WPT testharness scripts into Div tests

Status: accepted, 21 November 2026 (proposed 13 November 2026). The user
approved it and moved its ownership from the components track to the
framework track. Owner: framework track, taking item 0.13 (T3) of the
platform plan for the engine-tools lane. Amendment 1 (compiled subset
on a value runtime, below) accepted by the user on 22 November 2026.
Consumers: the
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
  front end. The resulting script must be straight-line. (Amendment 1
  replaces this bullet.)
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
  ownership table under the framework track.
- **Framework**, as owner. Runs the work below.

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

## Amendment 1: a compiled subset on a value runtime

Status: accepted by the user on 22 November 2026 (proposed the same day by
the framework track, with T3b-1). It replaces section 1's "same interpretation
approach" bullet. It changes nothing else: the capability table, the
ledger, the expectations and the "no near equivalent" rule stand.

T3b-2 extends the accepted subset under the same rule: strict mode moves
from rejected to accepted, and promises and async functions are added. It
also adds the reachability rule below. These extensions are proposed in the
T3b-2 PR.

### Problem

The straight-line model can't express the suites T3 starts with. Their
subtests are callbacks that run frames later and share mutable state:

- a `requestAnimationFrame` callback that requests itself until a counter
  reaches 2 (`callback-multicalls.html`);
- a callback that cancels another and sets a flag a third one reads
  (`cancel-pending.html`).

How many frames these take is a result of the test, not something known
when it's generated, so unrolling and inlining can't reach a straight line.

### Proposal

`tools/wpt_script/codegen.py` compiles each page's inline scripts, from the
pinned Acorn AST, into Rust that runs on a small value runtime
(`tests/wpt-script/src/runtime.rs`):

- JavaScript values are a Rust `Value`.
- Variables are shared cells, so closures see each other's writes.
- Every web surface is a host function that calls the public `divui` API.

Nothing is evaluated from strings, either when the test is generated or
when it runs. `eval`, `Function`, `with`, `document.write` and string
timers stay untranslatable, as in section 1.

**The rule.** Two things make a subtest untranslatable. It stops at run
time, at the point it reaches one of them, and its Div result is
`UNSUPPORTED` with a named reason:

- a construct outside the accepted subset;
- an operation where the web API and `divui` differ.

There's never a near mapping. The ledger records the reason in the subtest's
Div result, and `UNSUPPORTED` is counted apart from pass and fail.

**Accepted constructs** (everything else is `syntax:<node type>` or the
reason named below):

- Statements:
  - expression statements;
  - `var`, `let` and `const` with an identifier (no destructuring);
  - function declarations at the top of a function or script;
  - `return`, blocks, `if`/`else`;
  - `for`, `while`, `do`/`while`, and `for`/`of` over an array;
  - unlabelled `break` and `continue`;
  - empty statements.
- Expressions:
  - number, string, boolean and `null` literals;
  - untagged template literals;
  - identifiers and `this`;
  - member access, with a name or a computed key;
  - calls and method calls without spread;
  - `new` of `Promise`, `ResizeObserver` and `ResizeTestHelper`, which
    have hosts; any other constructor is reported;
  - `await`;
  - `=` and the arithmetic compound assignments;
  - `++` and `--`;
  - unary `!`, `-`, `+`, `typeof` and `void`;
  - binary `===`, `!==`, `==`, `!=`, `+`, `-`, `*`, `/`, `%`, `<`, `>`,
    `<=` and `>=`;
  - `&&`, `||` and `??`;
  - `?:` and the comma operator;
  - array literals without holes or spread;
  - object literals with name or string keys and plain values;
  - function expressions and arrow functions with identifier parameters,
    `async` or not.
- Not accepted yet, each with a named reason:
  - `try`, `throw`, `switch` and labels;
  - `for`/`in`, classes and regular expressions;
  - optional chaining and spread;
  - default, rest and pattern parameters;
  - logical assignment and getters;
  - generators (`syntax:generator`) and `for await`.
- Strict mode: a `"use strict"` directive of a script or function is
  accepted, with the semantics in the table below.
- Scoping:
  - Variables are hoisted to their function.
  - A block's `let` or `const` is accepted only when nothing outside the
    block names it (`syntax:block-scoped-binding`).
  - A closure over a binding declared inside a loop is rejected
    (`syntax:per-iteration-binding`).
  - Redeclaring a `let` or `const`, and a function declared inside a
    block, are rejected.

### JavaScript semantics a naive Rust mapping would get wrong

| Semantics | Runtime |
| --- | --- |
| Number to string (`String(x)`, `+` with a string, computed keys) | Exact: ECMA-262 `Number::toString`, with the shortest round-trip digits, `0` for `-0`, and exponent form below 1e-6 and from 1e21. |
| String to number (`+s`, arithmetic, comparisons) | Exact `StringToNumber` for decimal strings and `Infinity`, with JavaScript's whitespace set. Any other string is NaN, as in JavaScript. Hexadecimal, octal and binary strings are rejected (`non-decimal-string-to-number`). |
| Objects to primitives (`ToPrimitive`: an array's `join`, an object's own `toString` or `valueOf`) | Rejected (`to-string:object`, `to-number:object`). |
| `typeof` | Exact for every value kind. An undeclared name that the runtime doesn't define is rejected, not answered `"undefined"`. |
| `===` | Exact: NaN differs from itself, `+0` equals `-0`, and objects compare by identity. |
| `==` | Exact between primitives (`null` equals `undefined`, mixed primitives through `ToNumber`). An object against a primitive is rejected (`loose-equality:object`). |
| `<` and the other comparisons | Exact: strings compare by UTF-16 code units, everything else through `ToNumber`. |
| Closure capture | Exact: closures share their variables' cells, and `let` and `const` keep their temporal dead zone. Per-iteration loop bindings are rejected. |
| `this` | Exact. Sloppy: a plain call's `this` is the global object, and a primitive `this` (which JavaScript boxes) is rejected. Strict: `this` is what the caller passed, `undefined` for a plain call, and is never boxed. Arrows keep their enclosing `this`. |
| `arguments` | Exact as a copy of the arguments. In sloppy mode, a function that also assigns a parameter, which aliases `arguments`, is rejected (`syntax:mapped-arguments`). In strict mode `arguments` is unmapped, so the copy is exact. |
| Strict mode's other differences | Assigning a named function expression's own name throws `TypeError` (sloppy: ignored). Assigning an undeclared name is rejected in both modes (`global-assignment:<name>`): strict mode throws `ReferenceError` only when the global doesn't exist, and the runtime doesn't know every global. Writing a read-only property is rejected, because read-only platform objects reject every write. The directive applies per script and is inherited by nested functions. |
| Promises | Exact job order (ECMA-262): a reaction is one microtask, and resolving with a promise goes through `NewPromiseResolveThenableJob`. The microtask checkpoint runs when the stack empties, after each script, timer, animation frame callback and `ResizeObserver` callback. An unhandled rejection is testharness.js's harness error. Rejected by name: rejection handlers (`.then(f, g)`, `.catch`, `.finally`), `reject(value)`, thenables that aren't promises, and other `Promise` statics. |
| `async` and `await` | Exact: the body runs synchronously up to its first `await`; `await` is `PromiseResolve` and one reaction; a `return` resolves the function's promise, and a throw rejects it. |
| `promise_test` | testharness.js's queue: each test starts from a reaction on the one before it. After an `UNSUPPORTED` test the queue stops, because the next test would start from a state the translation can't vouch for. |
| `ResizeTestHelper` | A native intrinsic of the sha256-pinned helper, statement for statement. An unsupported operation finishes the test as `UNSUPPORTED` and stops the page's chain; the helper's `catch` would otherwise turn it into a failure and carry on. |
| Built-in methods on arrays, objects, strings, numbers and functions | Rejected by name (`builtin:Array.prototype.push`), never `undefined`. |
| Timers (`step_timeout`) | Exact WebIDL `long` delay (`ToInt32`), due-time then creation order, the test as `this` and the extra arguments. A timer that HTML's nesting clamp could raise to 4 ms is rejected (`timer-nesting-clamp`). |

testharness.js keeps its own definitions:

- `assert_equals` is SameValue;
- `assert_true` and `assert_false` need a boolean;
- the ordering assertions need numbers;
- `assert_approx_equals` uses the test's own epsilon.

The runtime adds no tolerance. The clock stays exact: frame *n* runs at
*n* × 1000/60 ms (section 3).

### Expectations

The generated test checks each translatable subtest against
`tests/wpt-script/expected/`, which `codegen.py --record` writes from a
recording run, and checks the page's harness error too:

- any change of outcome fails the test, including a recorded fail that
  starts passing;
- `codegen.py --check` fails when the generated tests or their embedded
  expectations differ from what the pinned inputs and `expected/` give.

A recorded fail is a named gap. Counts report pass, fail, unsupported and
not run separately.

### Reachability

Pages stay unmodified. Some chain their subtests with promises, as
`observe.html` does (`test0().then(() => test1())...`), so a link that can't
run stops every link after it. A subtest behind such a link doesn't run, so
it isn't translatable, whatever surfaces it uses:

- `translate.mjs` reports each top-level promise chain, link by link. A
  link's surfaces exclude synchronous `test()` bodies, which catch their own
  failures and let the chain go on.
- `ledger.py` finds the first link with a blocked surface. Every subtest a
  later link declares or finishes that would otherwise be translatable gets
  the status `unreachable`, with the reason `blocked-chain:<link>`.
- An `unreachable` subtest's expected Div result is `NOTRUN`. If it starts
  running, the generated test fails until the row is updated.
- The expectations also record the run's reason for the stop (for
  `observe.html`, `surface:Image`). A chain that stops elsewhere or for
  another reason fails the generated test, even with the same subtests
  unrun.
- `ledger.py` also fails when the classifier and the recorded run disagree:
  a `translatable` row recorded `NOTRUN`, or an `unreachable` row recorded
  with any other result.

### Real input in the Chromium recording

Most `pointerevents` pages need `test_driver` actions, and WPT's own
`testdriver-vendor.js` is empty. `capture_chromium_subtests.py` therefore
runs them with the driver Chromium's web test server serves
(`config.tmpl.json`, pinned by sha256). Its vendor file sends pointer actions
through `chrome.gpuBenchmarking.pointerActionSequence`. The browser takes the
switches of `content/web_test`'s runner that decide these results, and an
800x600 viewport. Each page's `<meta name=variant>` is its own run.

This is pinned Chrome for Testing, not `content_shell`, so a run counts as
Chromium's result only where it reproduces Chromium's own record. Each run is
repeated three times and gets an `oracle`:

- `agrees`: stable, no `TestExpectations` entry applies on Linux, and every
  subtest and the harness status match the `-expected.txt` baseline (or
  pass, without one).
- `disputed`, with the reason: `test-expectations:<values>`, `unstable`,
  `driver:keyboard` (the vendor file needs `eventSender`) or
  `baseline-mismatch`.

A disputed run's subtests have the ledger status `disputed`. They stay in the
denominator, are never generated, and are never counted as a Chromium pass.

### Affected tracks

The framework track owns it, and no other track is affected:

- no `divui` API is added;
- `tools/wpt` and the renderer contract stay untouched.
