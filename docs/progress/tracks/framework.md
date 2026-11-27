# Framework track progress

Workstream B of the [alpha plan](../../alpha/plan.md): the public `openui`
crate, `div-platform` and the CLI and tooling crates. Names are working
names until D5 picks the product name.

## Plan for alpha.1

The order and verification of each PR are in
[RFC 0002](../../alpha/rfcs/0002-cli-and-app-layout.md#6-delivery).

| # | PR | State |
| --- | --- | --- |
| 1 | RFC 0002: CLI, app layout and async model | Merged (#90) |
| 2 | `div-cli` skeleton: `new`, `dev`, `build`, `doctor` | Merged (#93) |
| 3 | Multi-window runtime | Merged (#96) |
| 4 | Async runtime integrated with the UI thread and signals | Merged (#98) |
| 5 | Clipboard service | Merged (#99) |
| 6 | File dialogs | Merged (#100) |
| 7 | App menu | Merged (#101, #103) |
| 8 | HTTP client | Merged (#105) |
| 9 | `default` template and test utilities | Merged (#106) |

## RFC 0003 requests

The framework's part of
[RFC 0003](../../alpha/rfcs/0003-component-primitives.md#affected-tracks),
one PR each, smallest first.

| RFC request | Change | State |
| --- | --- | --- |
| 7 | `Element::contains` | Merged (#25) |
| 2 | Key names on keyboard events, and `repeat` | Merged (#27) |
| 3 | `view!` fills omitted optional props | Not started |
| 1 | `provide_context` / `use_context` | Not started |

## Dogfood gaps (sample-git)

Gaps the first dogfood app, `build-with-div/sample-git`, found in the
framework (its capability probe, run against `03573e7d`, and its
`docs/framework-gaps.md`). One PR
each, small fixes first, in the order 1, 2, 6, 5, RFC 0003 requests 3 and
1, then 3, 4 and 7.

| # | Gap | State |
| --- | --- | --- |
| 1 | `on:` on a component tag was silently dropped | Merged (#29) |
| 2 | `{expr}` children that are views (`For`, `Show`) | Merged (#30) |
| 3 | App services: `contextmenu` and popup menus, window activation, app data directory, reveal in file manager | Not started |
| 4 | Headless pointer events and pointer capture for drags | Merged (#68, #72) |
| 5 | `current_document()` in effect re-runs (sample-git 25) | Not started |
| 6 | zbus fails to build with a fresh lockfile (sample-git 24) | Merged (#31) |
| 7 | Public SVG drawing, for icons | Folded into RFC 0009 (inline SVG); the framework part is F1 below |

## Platform plan, Tier 0

The framework's foundations from the platform plan (six sample
investigations, consolidated on 13 November 2026), with the API of
[RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md).
They come before the remaining dogfood gaps and RFC 0003 requests, which
jump the queue only when they block a sample, in the order 0.10, 0.11,
0.12 A, 0.9, 0.12 B, 0.12 C, then keyboard scrolling. (0.12 was split in
three and 0.9 moved after 0.12 A on 16 November 2026, agreed through the
parent session: 0.12 A brings passive listeners, which the wheel work
uses.)

| # | Item | State |
| --- | --- | --- |
| 0.9 | Wheel direction, lines per notch and `deltaMode` (I3) | Merged (#63) |
| 0.10 | `requestAnimationFrame` and headless frame advance (R1, T1) | Merged (#47) |
| 0.11 | `ResizeObserver`, with the engine's part (O1) | Merged (#52) |
| 0.12 A | Listener handles and options (sample-editor gap 29) | Merged (#61) |
| 0.12 B | Pointer Events (I1), with sample-canvas gap 16 (focus on `mousedown`) | Merged (#68) |
| 0.12 C | Test input (T1), with sample-git gap 29 and dogfood gap 4 | Merged (#72) |
| 0.12 D | Click counting: `detail` and `dblclick` (RFC 0007 section 6) | Merged (#75) |
| I2a | Keyboard scrolling and `keypress` (sample-markdown MD-23, RFC 0007 section 7) | Merged (#86) |
| I2b | Keyboard-focusable scrollers and the navigation starting point (gap `keyboard-focusable-scrollers`, RFC 0007 section 8) | Merged (#89) |

## T3: WPT script translation (item 0.13, RFC 0005)

The framework track took T3 on 21 November 2026, in the re-plan that moves
the large Tier 0 items forward (approved through the parent session). It
follows RFC 0005, which the user accepted on 21 November 2026 with the
framework track as its owner. The translator is a new
`tools/wpt_script/`; `tools/wpt` and the renderer contract stay untouched.
Item 1.2 (`matchMedia` change events, OS reduced-motion and colour-scheme,
`devicePixelRatio`) moves ahead of T3b–T3d once the engine's S3/S4 PR 2
merges. (Item 1.2 is in review: see the 27 November 2026 log entry.) The editor gaps below wait behind T3.

| # | Item | State |
| --- | --- | --- |
| T3a | Front end, intrinsics, capability table and ledgers for animation-frames and resize-observer, with `--check` in CI | Merged (#113) |
| T3b-1 | The `div-wpt-script` crate, its runtime, the code generator and animation-frames end to end | Merged (#116) |
| T3b-2 | resize-observer: the `ResizeTestHelper` intrinsic, promises, and the DOM and style hosts | Merged (#119) |
| T3c-1 | `pointerevents`: the Chromium recording with real input, its oracle status, and the ledger | Merged (#124) |
| T3c-2 | `pointerevents`: computed subtest names, `test_driver` actions on pointer input, and the generated tests | Merged (#127) |
| T3d | `css-transitions`, `css-animations` and `css-easing` ledgers and their translatable subset (empty today) | Merged (#128) |
| T3e | `web-animations` ledger, the last suite of RFC 0005 step 2 and the measurement for A3 | Merged (#130) |
| T3f | Canvas generator (RFC 0005 step 5): T3f-1 recording and ledger, T3f-2 runtime host and tests (waits on C1 step 5b), T3f-3 reftests | Not started |
| Later | Interpolation-table generator (step 4), once A1–A3 exist; O2 and O3 suites (step 6), as each lands | Not started |

## After Tier 0

Queued after keyboard scrolling, in this order, unless one blocks a sample
outright. F1 moves up to follow 0.10 once the engine's E1 (namespaces in
element identity) merges, because the samples' icons are written in `view!`.

| # | Item | State |
| --- | --- | --- |
| editor 26 | `innerWidth` and `innerHeight` on `Document` | Not started |
| editor 27 | `offsetWidth`, `offsetHeight` and `offsetParent` | Not started |
| editor 23 | `attributes` and `attribute_names` in insertion order | Not started |
| editor 25 | `createElement` for any tag, `nodeName` and `tagName` (waits on the engine's `HTMLUnknownElement`; designed with F1) | Not started |
| editor 24 | Node type, `childNodes`, `insertBefore` over all node types, identity | Not started |
| editor 22 | Inline `style` and `cssText` (waits on the engine's R0b; with decision 2's runtime custom-property writes) | Not started |
| charts TZ1 | System time zone in `div-platform` | Not started |
| F1 | RFC 0009: `Element::create_ns`, and `view!` with SVG's namespace and spellings (waits on the engine's E1) | Not started |

## Log

### 27 November 2026: `matchMedia` change events and the system's preferences (item 1.2)

The framework's half of `matchMedia()` (RFC 0014 step 3). The engine's
lists and its report merged in #154. The public items were approved by the
user on 27 November 2026, relayed by the parent session: `MediaQueryList::on_change`,
`MediaQueryListEvent`, `div_platform::SystemPreferences` with `system()`, and
`PlatformEvent::SystemPreferencesChanged`.

Changes:

- `crates/divui/src/media_query_list.rs`: `on_change` adds a `change`
  listener and returns the existing `ListenerHandle`. A list with listeners
  stays registered after its last clone drops, as on the web, and is
  released with its last listener.
- `crates/divui/src/document.rs`: `begin_frame` runs HTML's "evaluate media
  queries and report changes" step after the scroll and selection events and
  before the animations. It fires `change` at each list the engine reports,
  oldest first, with the reported result. Listeners follow the DOM's rules:
  one removed during the dispatch is not called, and one added waits for the
  next event. A change to the viewport, color scheme or reduced motion while
  lists exist asks the native App for a frame, even when no style reads it.
- `crates/div-platform/src/preferences.rs`: `SystemPreferences::system()`
  reads Windows' app mode (`AppsUseLightTheme`) and animation effects
  (`SPI_GETCLIENTAREAANIMATION`); other platforms report the defaults. Each
  window gets a subclass hook for `WM_SETTINGCHANGE`, which winit does not
  forward, and the runtime reports an actual change once to every window.
- `crates/divui/src/app.rs` and `app_spec.rs`: a built App applies the
  system's preferences before any view mounts (not under `DIV_RENDER_PNG`),
  and a window applies them when it opens, which covers `App::from_document`.
  A change applies reduced motion first, then the color scheme (choice (a)),
  and overrides earlier `set_color_scheme` and `set_prefers_reduced_motion`
  calls (choice (b)). `devicePixelRatio` already reaches the lists through
  `ScaleFactorChanged` and `set_viewport`.
- `crates/div-ffi/src/native_app.rs`: the frozen v0.2 raw platform event has
  no type for the new event, so C callbacks don't see it; the App still
  applies it to the document. No ABI change.

Oracle: `match-media-v1` (#154). `crates/divui/tests/match_media_events.rs`
replays its 23 matched sequences through listeners and `begin_frame`, and
requires the same events per frame. The one gap is the engine's
(`scheme-and-motion-together`).

Remaining gaps:

- Linux reports the default preferences. Reading the XDG settings portal
  (`org.freedesktop.appearance`) is a follow-up.
- Chromium's order for one OS notification that changes both preferences is
  not recorded; choice (a) is a working rule, pinned by
  `system_preferences_apply_reduced_motion_before_the_color_scheme`.
- C apps cannot observe the change; that needs a new raw event type (an ABI
  change for the user).

Verification, at `baef6801` plus this change:

- WSL debug with CI's Skia flags: `cargo test --locked -j 4 -p div-platform
  -p divui -p div-ffi --features "div-platform/linux divui/linux divui/async
  divui/testing div-ffi/linux" --lib --tests`: 64 binaries, 578 passed,
  0 failed. `-p divui -p div-ffi --features "divui/async divui/http
  divui/testing"`: 65 binaries, 569 passed, 0 failed, 8 ignored.
- Windows debug, MSVC 14.52 (CI pins 14.44), the windows-msvc job's Skia
  flags: `-p div-platform -p divui -p div-ffi --features "div-platform/windows
  divui/windows divui/async divui/testing div-ffi/windows" --lib --test
  match_media_events`: 146, 19, 143 and 7 tests passed, including the hook
  test, which sends `WM_SETTINGCHANGE` to a message-only window. The
  window-opening integration tests were left to CI.

### 23 November 2026: a listener that removes the pointer's target

Problem: T3c-2 recorded a Div error that Chromium doesn't have. When a
`pointerdown` listener removed its own target, the rest of the pointer step
used the removed node's handle and `dispatch_pointer_input` returned
`StaleHandle`. The two affected WPT rows
(`pointerup_after_pointerdown_target_removed.html` `?mouse` and `?pen`) passed
but carried a `div_harness_error`.

Change (`crates/divui/src/pointer_dispatch.rs`):

- Dispatch records the target's path before the first event. Each later part
  of the step goes to the innermost element of that path still in the
  document.
- A mouse release sends no `click` once the pressed element has left the
  document.
- A tap still clicks the surviving element.

Source: pinned Chromium 147.0.7727.50, `PointerEventManager::SendMousePointerEvent`,
`MouseEventManager::DispatchMouseClickIfNeeded` and
`MouseEventManager::HandleRemoveSubtree`, with the stable
`BoundaryEventDispatchTracksNodeRemoval` feature. All are BSD-licensed.

Oracle: `tests/fidelity/fixtures/pointer-removed-target-v1`. `capture.py`
drives `page.html` in Chrome-for-Testing on Linux over CDP and writes the
result only if three runs agree. It has 8 mouse scenarios (`pointerdown`,
`mousedown`, `pointerup` or `mouseup`, removing the target or its parent) and
4 tap scenarios. `crates/divui/tests/pointer_removed_target.rs` compares the
event sequence and final focus of all 12.

Div results for pointerevents: **3 pass, 0 fail, 0 unsupported, 0 not run**,
unchanged. The two `div_harness_error` entries are gone.

Remaining gaps, each failing the comparison if it closes:

- `focusout-on-removal` (framework): Chromium fires `focusout` when the focused
  element is removed. Div clears focus without an event.
- `hover-after-dom-change` (framework, existing): after a mouse release that
  removed the target, Chromium fires `pointerover` and `mouseover` at the
  surviving element without new input. Div fires them at the next pointer
  event.

Not covered: a target that a listener moves instead of removing, a pressed
element removed and reinserted, enter and leave events (the recording has
none), and pen input in the new oracle. WPT's `?pen` row covers pen for the
removal itself.

Verification, on this branch at `18ba5303` plus the fix:

- WSL debug with CI's Skia flags, `cargo test --locked -j 4 -p divui --features
  testing` and `-p div-wpt-script`: 60 binaries, 404 passed, 0 failed, 8 ignored.
- `tools/wpt_script/ledger.py --check` and `codegen.py --check` pass, and so do
  their 68 unit tests.
- `generate_renderer_contract.py --check`: profiles=4 focused=40 style=235
  unclassified=0 javascript=393.

### 23 November 2026: T3e, the web-animations ledger (RFC 0005 step 2)

Problem: T3d measured the CSS motion suites, but nothing measured
`web-animations`, the suite for A3 and the last one in RFC 0005 step 2.

Change (`tools/wpt_script/`, `tests/wpt-script/`, CI's sparse checkout; no
`divui` or engine change):

- The recorder runs `web-animations` with Chromium's web test switches and
  three repeats, and checks each run against Chromium's own record.
- 13 more helper libraries are pinned by sha256 and analysed from source.
  Analysis can't make a row translatable.
- 19 Web Animations names join the capability table as A3, with no `divui`
  call.
- CI's sparse checkout adds `scroll-animations/scroll-timelines`, because one
  page loads its helper.

Ledger. **No subtest in this suite is translatable yet, so T3e generates no
tests.**

| Suite | Runs (agree, disputed) | Files excluded | Subtests | Translatable | Blocked | Disputed | Untranslatable |
| --- | --- | --- | --- | --- | --- | --- | --- |
| web-animations | 133 (127, 6) | 55 | 3,979 | 0 | 3,922 | 53 | 4 |

Div results for the suite: **0 pass, 0 fail, 0 unsupported, 0 not run.**

What the 3,922 blocked rows may need. A row can need several, and member names
can match more than one interface, so these are upper bounds:

| Need | Rows |
| --- | --- |
| A3, Web Animations API | 3,918 |
| A helper the runtime doesn't implement (`T3:helper-intrinsic`) | 3,826 |
| `getComputedStyle` and its values (`gap:computed-style`, S21) | 3,068 |
| Script exceptions (`gap:script-exceptions`) | 491 |
| Attribute reflection (`gap:attribute-reflection`) | 186 |
| A1, CSS Transitions from style rules | 173 |
| Child documents (`gap:child-documents`) | 154 |
| A2, CSS Animations | 45 |
| A surface with no table entry yet | 3,851 |

The last row also counts helper constants and members of test data, so it is
not a list of missing web APIs.

- The 53 disputed rows are six pages. `TestExpectations` lists four as
  failing. Two differ from Chromium's baseline on subtests about child
  documents (`animate-no-browsing-context.html`, `timelines.html`). Their
  cause isn't diagnosed, and they stay disputed.
- The 4 untranslatable rows are `keyframe-exceptions.html` (3), whose script
  tag names a file the pinned checkout doesn't have, and
  `applying-interpolated-transform.html` (1), whose subtest has no declaration
  on the page.
- Existing ledgers: no status, count or Div result changes. The new A3 names
  add A3 to 59 blocked rows (46 in `css-animations`, 13 in `css-transitions`).

Verification:

- Two full recordings agree on every status. The only difference is a
  wall-clock number in one failure message, on a page `TestExpectations`
  lists as failing.
- `python -m unittest discover -s tools/wpt_script -p "test_*.py"`: 68 tests
  pass.
- `ledger.py --check` and `codegen.py --check` pass against a checkout with
  only the paths CI's sparse checkout fetches.

### 22 November 2026: T3d, the transitions, animations and easing ledgers (RFC 0005 steps 2 and 3)

Problem: nothing measured how much of `css/css-transitions`,
`css/css-animations` and `css/css-easing` Div can run, or what stops the rest.
That count is where the motion work (A1, A2, A3) starts.

Change (`tools/wpt_script/`, `tests/wpt-script/`, CI's sparse checkout; no
`divui` or engine change):

- The recorder runs the three suites with Chromium's web test switches and
  three repeats, and checks each run against Chromium's own record.
- `idlharness` pages are excluded, as RFC 0005 section 4 says: they need
  wptserve's IDL files.
- 19 helper libraries are pinned by sha256 and analysed from source (RFC 0005,
  "Analysed helpers"). Analysis only names what blocks a row. It can't make a
  row translatable.
- The capability table names the blocking features. Each new non-CSS row has
  no `divui` call, and a CSS property maps only where `StyleProperty` has it.

Ledger. **No subtest in these suites is translatable yet, so T3d generates no
tests.**

| Suite | Runs (agree, disputed) | Files excluded | Subtests | Translatable | Blocked | Disputed | Untranslatable |
| --- | --- | --- | --- | --- | --- | --- | --- |
| css-transitions | 116 (112, 4) | 74 | 3,215 | 0 | 2,611 | 563 | 41 |
| css-animations | 112 (111, 1) | 135 | 1,207 | 0 | 1,202 | 5 | 0 |
| css-easing | 12 (12, 0) | 1 | 156 | 0 | 151 | 0 | 5 |
| Total | 240 (235, 5) | 210 | 4,578 | 0 | 3,964 | 568 | 46 |

Div results for the three suites: **0 pass, 0 fail, 0 unsupported, 0 not run.**

What the 3,964 blocked rows may need. A row can need several, and member names
can match more than one interface, so these are upper bounds:

| Need | Rows |
| --- | --- |
| A helper the runtime doesn't implement (`T3:helper-intrinsic`) | 3,864 |
| `getComputedStyle` and its values (`gap:computed-style`) | 3,577 |
| A3, Web Animations API | 2,789 |
| A1, CSS Transitions from style rules | 2,535 |
| A2, CSS Animations | 1,377 |
| A surface with no table entry yet | 2,183 |

- 560 of the 568 disputed rows are one page,
  `properties-value-inherit-001.html`, which `TestExpectations` lists as
  `[ Failure Pass ]`.
- The 46 untranslatable rows are two pages whose script tag names a file the
  pinned checkout doesn't have (`transitionevent-interface.js`,
  `support/util.js`).
- pointerevents: 17 rows that loaded the shared CSS helpers move from
  `untranslatable` to `blocked` (40 to 23, and 2,002 to 2,019). Its 3
  translatable rows and Div results don't change, and neither do the
  resize-observer and animation-frames ledgers.

Verification:

- `python -m unittest discover -s tools/wpt_script -p "test_*.py"`: 68 tests
  pass.
- `ledger.py --check` and `codegen.py --check` pass against a checkout with
  only the paths CI's sparse checkout fetches.

### 22 November 2026: T3c-2, computed subtest names and the first pointerevents tests (RFC 0005 step 3)

Problem: 1,813 pointerevents rows (and resize-observer's 5 `fragments.html`
rows) were `untranslatable` only because the page computes the subtest's
name while it runs, and no pointerevents page could run in Div.

Change (`tools/wpt_script/`, `tests/wpt-script/`; no `divui` change):

- RFC 0005 "Computed subtest names" replaces `runtime-subtest-name`: such a
  row is translatable only if every surface on its whole page maps to
  `divui`, and the run must report exactly the names Chromium recorded
  (`expect_names`, `codegen.py --record`, `ledger.py --check`). A name that
  differs between Chromium's three repeats leaves the run `disputed`
  (`unstable`). The ledger marks these rows `computed_name`.
- The front end now counts markup event handlers (`<body onload="run()">`)
  and functions passed by name as page code, and the ledger reads `style`
  attributes' properties. Both used to under-report what a page needs.
- The runtime adds `addEventListener` (a function callback, `once` only),
  the `Event` members `type`, `eventPhase` and `target`, `location.search`,
  `String.prototype.substring`, `Array.prototype.push` and `toString`,
  `getEvent`, and `test_driver.Actions` with one pointer: `addPointer`,
  `pointerMove` (viewport or element origin), `pointerDown`, `pointerUp` and
  `send`. Each action is a tick, one per frame, with coordinates resolved at
  `send()` as Chromium's vendor file does. Anything else stops the subtest.

Ledger, before and after. Reclassified rows count as translatable, not as
passing, until they run:

| Suite | Before | After |
| --- | --- | --- |
| pointerevents (2,273) | 0 translatable, 192 blocked, 228 disputed, 1,853 untranslatable | 3 translatable, 2,002 blocked, 228 disputed, 40 untranslatable |
| resize-observer (82) | 5 translatable, 11 unreachable, 61 blocked, 5 untranslatable | 5 translatable, 11 unreachable, 66 blocked |
| animation-frames (10) | 6 translatable, 4 blocked | unchanged |

Of the 1,813 reclassified pointerevents rows, 3 are translatable and 1,810
blocked; the 5 `fragments.html` rows are blocked (`gap:background-shorthand`,
`gap:class-name`, `gap:outline-shorthand`). The 40 untranslatable rows are
`undeclared-subtest` (23) and `unrecognised-helper` (17).

Div results for pointerevents: **3 pass, 0 fail, 0 unsupported, 0 not run**
(`pointerup_after_pointerdown_target_removed.html` `?mouse`, `?pen` and
`?touch`). The mouse and pen runs also record a Div error that Chromium
doesn't have: `divui`'s pointer dispatch returns `StaleHandle` when a
`pointerdown` listener removes its own target. The rows carry it as
`div_harness_error`. animation-frames and resize-observer are unchanged.

What blocks the rest: 75 runs link `pointerevent_styles.css`, which needs the
`background` shorthand and `touch-action` (S15). At most 19 more runs (29
rows) would need only those two properties. That upper bound assumes every
event and input surface `divui` has is mapped, and also heading, list, `br`,
`button` and `onload` markup. The ledger doesn't map that markup yet, so pages
using it stay blocked (for example `markup:h1`, `markup:button`).

Verification:

- `python -m unittest discover -s tools/wpt_script -p "test_*.py"`: 60 tests
  pass, including the whole-page rule, exact names and renamed repeats.
- `ledger.py --check` and `codegen.py --check` pass against the CI-like
  sparse checkout.
- `cargo test -p div-wpt-script` (WSL, debug): all tests pass, including the
  runtime's name, builtin and tick tests.

### 22 November 2026: T3c-1, the pointerevents recording and ledger (RFC 0005 step 2)

Problem: 152 of the suite's 166 runnable pages use `test_driver`, and WPT's
own `testdriver-vendor.js` is empty, so plain headless Chrome can't perform
their input and its results say nothing about Chromium.

Change (`tools/wpt_script/`, `tests/wpt-script/`, CI's sparse checkout; no
`divui` change and no Div run of this suite yet):

- `capture_chromium_subtests.py` runs the suite with the driver Chromium's
  web test server serves, pinned by sha256. Pointer actions go through
  `chrome.gpuBenchmarking.pointerActionSequence`.
- The browser takes the `content/web_test` runner's switches that decide
  these results, and an exact 800x600 viewport, which the tool verifies.
- Each `<meta name=variant>` is its own run: 232 runs from 166 pages.
- Each run is repeated three times and gets an oracle status. This is pinned
  Chrome for Testing rather than `content_shell`, so a run stands for
  Chromium only where it reproduces Chromium's own pinned record.
- The ledger gains the status `disputed`. Disputed rows stay in the
  denominator, aren't generated, and never count as a Chromium pass.

Oracle (Chrome for Testing 147.0.7727.50, Linux, WSL):

| Oracle | Runs | Subtests |
| --- | --- | --- |
| Agrees with Chromium's pinned record | 186 | 2,045 (1,913 pass, 132 fail) |
| Disputed: `TestExpectations` entry | 27 | 80 |
| Disputed: `-expected.txt` mismatch | 9 | 122 |
| Disputed: keyboard action needs `eventSender` | 7 | 18 |
| Disputed: unstable across repeats | 3 | 8 |

Ledger (`tests/wpt-script/ledgers/pointerevents.json`), 2,273 subtests:
**0 translatable**, 192 blocked, 228 disputed, 1,853 untranslatable. Div
has no pass, fail, unsupported or not-run count for this suite yet, because
none of it runs.

- 1,813 of the untranslatable rows are `runtime-subtest-name`: the page
  computes the name while it runs. T3a's rule predates the compiled runtime,
  so T3c-2 has to propose what replaces it.
- 25 files are excluded: 15 `.https.`, 8 manual, 1 generated wrapper and 1
  page without `testharness.js`.

Verification:

- `python -m unittest discover -s tools/wpt_script -p "test_*.py"`: 55 tests
  pass.
- `ledger.py --check` and `codegen.py --check` pass against a checkout with
  only the paths CI's sparse checkout now fetches. The animation-frames and
  resize-observer ledgers don't change.
- `NeverFixTests` has no entry that applies on Linux to any of the 232 runs.

Chromium sources read (permissive, `09d377d9`): `testdriver.js` and
`testdriver-actions.js` (first 30 lines), `pointerevent_support.js`,
`web_tests/resources/testdriver-vendor.js` (first 30 lines and its input
calls), `TestExpectations` and `NeverFixTests` (headers and pointerevents
entries), `external/wpt/config.tmpl.json` (aliases), `wptserve.py`
(`_prepare_config`) and `web_test_browser_main_runner.cc` (its switches).

### 22 November 2026: T3b-2, generated tests for resize-observer (RFC 0005 step 3)

Problem: T3a marked 17 `resize-observer` subtests translatable, but that
count ignored two things. `observe.html` chains its tests with promises, so
a link that can't run stops every later one. And `scrollbars.html`'s sheet
uses a shorthand Div doesn't have.

Change (`tests/wpt-script/` and `tools/wpt_script/`; no `divui` change, no
`Cargo.lock` change). RFC 0005's Amendment 1 is extended in this PR:

- **Strict mode moves from rejected to accepted**, with undefined `this`,
  unmapped `arguments` and a named function expression's read-only name
  exact. Assigning an undeclared name stays rejected in both modes.
- Promises and `async` functions are accepted with ECMA-262's job order.
  Rejection handlers, `reject(value)`, non-promise thenables and other
  `Promise` statics are rejected by name.
- `promise_test` follows testharness.js's queue. `ResizeTestHelper` is a
  native intrinsic of the pinned helper, statement for statement.
- DOM, inline style and `ResizeObserver` hosts call `divui`. A style value
  Div can't parse reports `style-value:<key>=<text>`.
- **Reachability.** Pages stay unmodified. `translate.mjs` reports each
  top-level promise chain, and `ledger.py` marks a subtest behind the first
  blocked link `unreachable` with `blocked-chain:<link>`. Its expected Div
  result is `NOTRUN`, and the expectations pin the run's stop reason.
- **CSS surfaces.** The ledger reports each property a `<style>` sheet
  declares as `css:<name>`, and checks those rows against
  `StyleProperty::from_css_name`. `scrollbars.html` is blocked by
  `gap:background-shorthand`. When the shorthand lands, the check fails until
  its row is updated; the page isn't edited.

Results (WSL with the `rust-parity` environment, this branch on `main`
`090406bc`). Div's four counts cover the generated rows:

| Suite | Subtests | Translatable | Unreachable | Blocked | Untranslatable | Div pass | Div fail | Div unsupported | Div not run |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `html/webappapis/animation-frames` | 10 | 6 | 0 | 4 | 0 | 4 | 0 | 2 | 0 |
| `resize-observer` | 82 | 5 | 11 | 61 | 5 | 5 | 0 | 0 | 11 |

- **5 of `observe.html`'s 16 otherwise translatable rows are reachable
  today.** The chain stops at `test5`'s `new Image()` (`surface:Image`), so
  `guard`, test7–10 and test13–18 don't run.
- The 11 not-run rows are named gaps, not passes.

Verification:

- `cargo test -p div-wpt-script` (WSL, debug): the 7 runtime tests pass, and
  the 7 generated tests match their recorded outcomes. Windows runs it in
  CI's `windows-msvc` job; it was not run locally.
- Mutations:
  - skipping `test5` and `test6` in the generated test makes test7–10 run
    and pass; the test fails on each of them and on the changed stop reason;
  - an unreachable row recorded PASS fails `ledger.py --check`;
  - a reachable row recorded NOTRUN fails `ledger.py --check`.
- `python -m unittest discover -s tools/wpt_script -p "test_*.py"`: 47 tests
  pass.
- `ledger.py --check` and `codegen.py --check` pass against a checkout with
  only the two suites, as CI has.
- `cargo fmt --all --check` passes.

WPT sources read (BSD-3, Chromium `09d377d9`): the six animation-frames
pages, `resize-observer/resources/resizeTestHelper.js`, parts of
`resize-observer/observe.html`, `resize-observer/scrollbars.html`, and parts
of `resources/testharness.js`.

### 22 November 2026: T3b-1, generated tests for animation-frames (RFC 0005 step 3)

Problem: T3a's ledgers name the translatable subtests, but none of them ran
against Div.

Change (`tests/wpt-script/` and `tools/wpt_script/`, CI; no `divui` change).
It follows RFC 0005's Amendment 1 (a compiled subset on a value runtime, the
PR's first commit), which the user accepted on 22 November 2026.

- The new `div-wpt-script` crate holds the runtime the generated tests run
  on (`src/runtime.rs`):
  - JavaScript values, and variables as shared cells, so closures see each
    other's writes. `let` and `const` keep their temporal dead zone.
  - testharness.js: `test`, `async_test`, `step`, `step_func`,
    `step_func_done`, `unreached_func`, `add_cleanup`, `step_timeout`, and
    the `assert_*` functions with its semantics (`assert_equals` is
    SameValue; `assert_true` needs `true`).
  - A virtual clock: frame *n* runs at *n* × 1000/60 ms through
    `TestApp::advance_frame`, timers due by then run first, and a test still
    running after 10 s of virtual time times out.
  - `requestAnimationFrame` and `cancelAnimationFrame` call
    `Document::request_animation_frame` and `cancel_animation_frame`.
  - What has no exact `divui` call stops the subtest as UNSUPPORTED with the
    reason, never a near equivalent. For example, Div's frame handle is
    opaque, so using it as a number reports `numeric-frame-handle`.
  - JavaScript's conversions are exact or rejected: `Number::toString`,
    `StringToNumber` (decimal only), `==` between primitives, sloppy-mode
    `this`, and timers' WebIDL `long` delay are exact. `ToPrimitive`,
    mapped `arguments` and timers the nesting clamp could raise are
    rejected (Amendment 1's table lists each).
- `codegen.py` compiles each page with a translatable subtest into a
  `#[test]` (`tests/wpt-script/tests/<suite>.rs`):
  - the page's markup, in document order, through `Element::create`;
  - its inline scripts, from the pinned Acorn AST (`translate.mjs --ast`),
    into Rust on the runtime.
  - Constructs outside the accepted subset (`try`, `throw`, regular
    expressions, optional chaining, async functions, spread, destructuring,
    a block binding named outside its block, closures over a loop binding,
    and others) compile to UNSUPPORTED with the construct's name.
- Expectations: the generated test compares each translatable subtest with
  `tests/wpt-script/expected/`. A regression fails, and so does a pass that
  is not recorded yet.
  - `DIV_WPT_SCRIPT_RECORD=<dir> cargo test -p div-wpt-script` writes the
    results; `codegen.py --record <dir>` makes them the expectations.
  - The ledgers gain a `div` column and counts.
- CI runs `div-wpt-script` in the Linux and Windows workspace test steps, and
  `test_codegen` and `codegen.py --check` in `python-accountability`.

Results (Windows, this branch):

| Suite | Translatable | Div pass | Div fail | Div unsupported |
| --- | --- | --- | --- | --- |
| `html/webappapis/animation-frames` | 6 | 4 | 0 | 2 (`numeric-frame-handle`: `callback-handle`, `cancel-invoked`) |

The 2 unsupported subtests pass in Chromium; they are named gaps, counted
apart from passes.

Verification:

- `cargo test -p div-wpt-script` (Windows): each of the 6 generated tests
  matches its recorded outcome (4 pass, 0 fail, 2 unsupported), and the
  runtime's 3 conversion tests pass.
- Mutations: a no-op `cancelAnimationFrame` in the runtime fails
  `cancel_pending`, and a PASS recorded as FAIL fails `callback_timestamp`.
- `python -m unittest tools.wpt_script.test_ledger tools.wpt_script.test_codegen`:
  33 tests pass.
- `ledger.py --check` and `codegen.py --check` pass against a checkout with
  only the two suites, as CI has.
- `cargo fmt --all --check` passes; the generated file keeps
  `#[rustfmt::skip]` on each test.

### 21 November 2026: T3a, the WPT script ledgers (RFC 0005 step 2)

Problem: WPT measures `requestAnimationFrame`, `ResizeObserver`, Pointer
Events and the animation features mostly with `testharness.js` scripts,
which Div's WPT tooling rejects. Nothing counted which of those subtests Div
could run, or what blocks the rest.

Change (`tools/wpt_script/`, `tests/wpt-script/`, CI only; no Rust):

- `capture_chromium_subtests.py` loads each page of a suite in pinned
  Chromium 147.0.7727.50, headless, from a local server whose
  `testharnessreport.js` stores the harness's results. It records every
  subtest's name, status and message in `tests/wpt-script/chromium/`. These
  recordings are each suite's denominator: names built at run time (loops,
  helpers, the title as default name) cannot be listed statically.
  - Pages that need wptserve (`.sub.`, `.https.`, generated `.any.js` and
    `.window.js` wrappers), a person (`-manual`) or are references are
    recorded as excluded, with the reason.
- `translate.mjs` (pinned Acorn 8.14.0) finds each page's subtest
  declarations (`test`, `async_test`, `promise_test`, single-page tests and
  the intrinsic `ResizeTestHelper`) with their static names, and the web
  surfaces each one reaches:
  - its own call, its enclosing top-level function, the top-level helper
    functions those call, and the page's own top-level code;
  - not another subtest's body, so the page's `test0().then(test1)…`
    chain does not mix them up.
  - Names built at run time and dynamic code (`eval`, `Function`, `with`,
    `document.write`) make a subtest untranslatable, with the reason.
- `ledger.py` adds the page's markup (tags, `<style>`, `style` attributes,
  linked stylesheets), resolves helper scripts against `intrinsics.json` by
  path and sha256, and maps every surface through `capabilities.json` to
  its plan feature and its `divui` call, or to null.
  - No surface maps to a near equivalent. Gaps without a plan row are named
    `gap:…` (for example `gap:query-selector`).
  - Generic member names (`.width`, `.left`, `.length`) are labelled by the
    use the suites make of them; T3b's codegen types each use and rejects
    any other.
  - It fails if the recording no longer matches the suite's pages, and
    records Chromium's own `-expected.txt` result next to the recorded one
    when they differ.
- CI (`python-accountability`) fetches the two suites in a sparse checkout
  of their own (`WPT_SCRIPT_ROOT`) and runs the unit tests and
  `ledger.py --check`. It is kept apart from `CHROMIUM_WPT_ROOT`: the CSS
  porter resolves `/resources/` and `/common/` files that exist under that
  root, so adding them there would change pinned porter output.

Results at Chromium `09d377d9` (`tests/wpt-script/ledgers/`):

| Suite | Subtests | Chromium passes | Translatable | Blocked | Untranslatable | Excluded pages |
| --- | --- | --- | --- | --- | --- | --- |
| `html/webappapis/animation-frames` | 10 | 10 | 6 | 4 | 0 | 1 (manual) |
| `resize-observer` | 82 | 79 | 17 | 60 | 5 | 9 |

T3b-2 corrected `resize-observer`'s 17: 5 are translatable, 11 of
`observe.html`'s can't be reached on the unmodified page, and
`scrollbars.html`'s 1 is blocked by `gap:background-shorthand`.

- **Chromium's failures.** The 3 it fails are `fragments.html`'s, as its
  `-expected.txt` says.
- **The largest blockers in `resize-observer`:**
  - `querySelector` (36 subtests);
  - SVG markup: V1 shapes (28) and V2 `<text>` (20);
  - `document.fonts.ready` (20);
  - error events (9).
- **Untranslatable.** The 5 are `fragments.html`'s, whose names are built in
  a loop.
- **Excluded pages.** 3 references, 4 pages without `testharness.js`, the
  cross-origin frame (`.sub.`) and `idlharness`.

Verification:

- `python -m unittest tools.wpt_script.test_ledger`: 18 tests pass.
  - With the top-level-only shadowing rule replaced by the page-wide one,
    `test_a_local_named_test_does_not_hide_declarations` fails.
  - Without the rule that keeps subtests' bodies apart,
    `test_subtests_do_not_take_each_others_surfaces` fails.
- `python tools/wpt_script/ledger.py --check` passes on Windows, against a
  checkout with only the two suites' directories (as CI has).
- The recordings were made in WSL with the pinned Linux build
  (`~/openui-oracle/chrome/linux-147.0.7727.50`).
- No Rust, engine or census change (D4 does not apply).

### 19 November 2026: Chromium's text raster as the Windows default (RFC 0012)

Problem ([RFC 0012](../../alpha/rfcs/0012-windows-text-raster.md)): Chromium
147 on Windows draws text with ClearType LCD antialiasing, sRGB gamma and
contrast 1.0, but a Div app on Windows drew grayscale text
(`RasterConfiguration::default()`). Also, `App::run_document` built later
windows with `EngineOptions::default()` instead of the app's choice.

Change (framework; the engine preset `chromium_windows_lcd()` merged in
#93):

- **`div-platform`**: `system_raster_configuration() ->
  Option<RasterConfiguration>` (new `text_raster.rs`).
  - It reads what Chromium reads, from `font_render_params_win.cc` and
    `font_util_win.cc` (BSD, 147.0.7727.50, each header checked first):
    - `SPI_GETFONTSMOOTHING` and `SPI_GETFONTSMOOTHINGTYPE`;
    - the primary display's `PixelStructure` under
      `HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<display>`, else
      `SPI_GETFONTSMOOTHINGORIENTATION`;
    - DirectWrite's gamma and enhanced contrast, clamped to Skia's
      ranges, only when that key exists.
  - It reads once per process (`OnceLock`) and returns `None` off Windows.
  - ClearType with RGB order and no key is exactly the preset.
  - Smoothing off gives aliased text, and standard smoothing or no order
    gives grayscale; both use sRGB gamma and contrast 1.0. BGR keeps LCD with
    BGR geometry. A tuning key takes DirectWrite's values. These paths are
    unmeasured, as the RFC says.
  - The `windows` feature now takes the `windows` crate with
    `Win32_Graphics_DirectWrite`, and `windows-sys` gains
    `Win32_Graphics_Gdi`. `Cargo.lock` is unchanged.
- **`divui`**: `AppBuilder` and `AppSpec` keep the app's engine options as
  `Option<EngineOptions>`. The public API is unchanged.
  - With no explicit choice, a windowed document uses the system
    configuration, else the default (`windowed_engine_options`).
  - An explicit `engine_options(...)` always wins,
    `EngineOptions::default()` included.
  - `DIV_RENDER_PNG` renders keep the default, as do `HeadlessApp`,
    `Document::new`/`with_options` and `divui::testing`.
  - `App` remembers the explicit choice. `run_document` now gives it to
    windows opened later; before, they got `EngineOptions::default()`.
  - `App::from_document` takes the document's raster configuration as the
    app's choice.

Tests:

- `div-platform` (`text_raster.rs`, 6 tests): ClearType RGB with no key is
  the preset; BGR; grayscale; aliased; the clamped tuning values; `None`
  off Windows.
- `divui` unit test `windowed_engine_options_follow_rfc_0012`: explicit
  wins, the system without one, the default for `DIV_RENDER_PNG` and with
  no system value.
- `native_raster_options.rs`, new
  `an_app_without_engine_options_uses_the_system_raster_configuration`:
  `App::builder().build()` takes the system configuration, which Windows
  must report. An explicit default stays default, and `HeadlessApp` stays
  default. Before the change it fails on Windows, where the app got the
  default.

Verification (this branch on main `992d4d68`):

- Windows (MSVC, this machine: ClearType on, RGB, no per-display key):
  - `system_raster_configuration()` returns exactly
    `RasterConfiguration::chromium_windows_lcd()`.
  - `cargo test --locked -p div-platform --features windows --lib
    text_raster` passes 6 tests.
  - `cargo test --locked -p divui --features "windows testing" --test
    native_raster_options` passes 5. With the `divui` change reverted, the
    new test fails: the app got `RasterConfiguration::default()` where the
    system reports the preset.
  - The CI `windows-msvc` test set, run without its `dialog` and `menu`
    features (their tests drive real native dialogs and menus), first
    failed one test: `render_png_hook_matches_the_headless_first_frame`.
    That test builds a windowed app with no options and calls the render
    hook without `DIV_RENDER_PNG`, so on Windows it now gets LCD text. It
    now chooses the default explicitly.
  - Rebased on main 2c25496a, the same set (`div-platform`, `divui`,
    `div-ffi`, `div-macros`, `div-components`, `div-cli`) runs 65 suites:
    654 tests pass, none fail, 13 are ignored.
  - Rebased on main 848551a6, which adds the fidelity lane's Windows
    web-font fixture (#100, step 3 of RFC 0012's migration):
    `native_webfont_fontations` (1 test) and `native_raster_options` (5)
    pass. That fixture's page selects `chromium_windows_lcd` explicitly and
    is exact at 1x; an app that sets no options now gets the same
    configuration on this machine.
- Linux (WSL): `cargo test --locked --no-fail-fast -p divui --features
  "linux testing"` passes 51 of 52 suites (387 tests), and `-p div-platform
  --features linux` passes 16. The one failure, `native_rounded_clip`'s
  pixel comparison, comes from this WSL target's Skia build without CI's
  flags and fails the same way at main.
- `python tools/qualification/generate_renderer_contract.py --check` and
  `cargo fmt --all --check` pass.
- D4: no engine code changed, and the census and coverage use headless
  documents with explicit options, so they cannot move.
- Windowed examples (hello, counter, todo and others) now draw LCD text on
  Windows, as intended. Their only pixel check, `check_cli_template.py`,
  renders through `DIV_RENDER_PNG` and keeps the default.

### 19 November 2026: the wheel limit snaps to device pixels (gap `wheel-limit-unsnapped-client`)

Problem (gap `wheel-limit-unsnapped-client`, from the engine review on
#63): cc limits a wheel scroll to the scroll node's `container_bounds`, a
whole-pixel size, while Div limited it to the unsnapped client size. A
scroller with a fractional client box scrolled a different distance.

Oracle (new, `tests/fidelity/fixtures/wheel-limit-v1/chromium-linux-x11.json`),
captured by `capture_wheel_oracle.py --limit` from pinned Chrome-for-Testing
147.0.7727.50:
- Input: real X11 input through XTEST in WSL, with `--ozone-platform=x11`,
  at 1x and 1.25x with `--force-device-scale-factor` (RFC 0010). Each scale
  is captured twice and the two must match.
- Page: a 300px-wide `overflow-y: scroll` scroller under the pointer, at six
  fractional tops and heights around 100px. One 120px notch scrolls each
  from 0.

Chromium, measured: the limit is the client box pixel-snapped in device
pixels at its position, `round(bottom × scale) − round(top × scale)`, then
divided by the scale. All 12 cases fit this rule:
- At 1x, a box from 160.3 with height 100.4 moves 101, and one from 160.6
  moves 100.
- At 1.25x, a box from 226.6 with height 100.4 moves 126 device px, which
  is 100.8 CSS px.

cc's source agrees: `LimitDeltaToScrollerSize` and
`ResolveScrollGranularityToPixels` both read `scroll_node.container_bounds`
(`cc/input/input_handler.cc`, BSD, read from the local checkout with its
header checked first).

Change (**engine**, its own commit, for engine review):
`Engine::scroll_wheel_by` limits pixel and line scrolls, and sizes a page
step, by the client rect snapped that way. The position is the scroll box's
offset from the root fragment, summed from layout offsets only, as
`rebuild_hit_test` sums the paint offset; scroll offsets don't move it. A
new helper, `scroll_box_with_origin`, finds the scroll box as
`find_scroll_box` does, along with that offset. The capture script's
`--limit` mode is in the same commit.

Tests:
- `the_scroll_limit_is_the_device_pixel_snapped_client_box`
  (`crates/divui/tests/wheel_events.rs`) replays all 12 cases through the
  Linux wheel settings and `dispatch_wheel_event_with_mode`. Nine compare
  exactly. In the other three, at 1.25x, Chromium's offset is 100.8, and
  they stay gap `device-scale-scroll-offset`: Div rounds an element's offset
  to the whole CSS pixel, 101. The test checks that they still differ.
- `the_limit_is_the_client_box_snapped_to_device_pixels`
  (`crates/div-engine/tests/wheel_scroll_limits.rs`): three cases at 1x
  and 1.25x through the engine.
- Before the change, both fail at the first case (100 where Chromium has
  101).

Still unmeasured:
- the horizontal axis (same code);
- a scroller inside a scrolled ancestor whose layout offset is fractional
  (Div leaves scroll offsets out of the position, as Blink's paint offset
  does);
- a scroller inside a transformed ancestor (Blink snaps in the transform's
  space; `wheel-transformed-scroller` remains);
- negative positions, where Div's `floor(x + 0.5)` and Blink's
  `LayoutUnit::Round` can differ at exactly −n.5;
- the page step's snapped size (`wheel-page-unmeasured`);
- Windows, whose 100px notch is below these heights.

RFC 0011 does not apply: this snaps an input-path limit, as cc builds
`container_bounds` in device space, and changes no fragment or paint
position.

Verification (this branch on main `f2c3ab89`; rebased unchanged onto `992d4d68`):

- Linux (WSL): `cargo test --locked -p divui --features "linux testing"
  --test wheel_events` passes 3 tests, and `cargo test --locked -p
  div-engine --test wheel_scroll_limits` passes 4. With the engine change
  reverted, the two new tests fail (100 where Chromium has 101) and the
  others pass.
- Linux (WSL): `cargo test --locked --no-fail-fast -p div-engine` passes 64
  of 65 suites (531 tests). `cargo test --locked --no-fail-fast -p divui
  --features "linux testing"` passes 51 of 52 (386 tests). The two failures,
  `source_layer_raster` and `native_rounded_clip`, are pixel comparisons
  that fail the same way at main in this WSL target, which is built
  without CI's Skia flags.
- `python tools/qualification/generate_renderer_contract.py --check` and
  `cargo fmt --all --check` pass.
- No layout, paint or style code changed: only how far a wheel scroll may
  move. The census has no input, so it and coverage cannot move (decision
  D4).

Engine review: the engine track approved a07b298 on 19 November 2026,
with nothing blocking. It keeps the device-scale part outside RFC 0011, and
will move this call site to stage 2's shared device-pixel snap helper. Its
notes on negative positions and fractional scrolled ancestors are in the
unmeasured list above.

### 18 November 2026: a removed pressed element hands `:active` to its parent

Problem (raised in the engine review of the pointer-state restyle): when
the pressed element was removed while the button was held, Div dropped its
press, so nothing matched `:active`. Chromium had not been measured.

Oracle (new, `tests/fidelity/fixtures/active-chain-removal-v1`), captured
by `capture_active_chain_oracle.py --removal` from pinned Windows
Chrome-for-Testing 147.0.7727.50, through CDP mouse input as
`active-chain-v1`. Eight scenarios on the `active-chain-v1` page, each on a
fresh page: remove the pressed element, remove an ancestor of it, reinsert
the removed element, move it, press again after the removal, remove a
pressed button, and hide the pressed element or an ancestor with
`display: none`. Each step records the `:active` ids, whether the taken-out
element matches `:active`, and the pointer and mouse events' targets. The
script captures the set twice, and the two must agree.

Chromium, measured:

- Removing the pressed element, or an ancestor of it, gives `:active` to
  the removed subtree's former parent and its ancestors. Inserting the
  element again does not take it back. The release fires `pointerup` and
  `mouseup` at the element under the pointer and no `click`.
- Moving the pressed element with `append` acts as a removal.
- Hiding it, or an ancestor, with `display: none` changes nothing: the
  chain stays, and the release clicks the element under the pointer.
- Timing: right after a removal in a settled page, Blink's remaining
  ancestors briefly match nothing. The chain reaches the parent at its next
  frame. The oracle reads every step after two frames. Div answers
  `:active` synchronously, so it shows the settled chain at once.

Change:

- **Engine** (its own commit, for engine review): `Engine` keeps each
  pressed pointer's `:active` element (`active_chains`) apart from the
  press target that a release activates (`active_pointers`). Removing or
  detaching that element moves it to the removed subtree's former parent.
  `active_pointers` still drops the removed target, so the release clicks
  nothing, as before. `is_active` reads `active_chains`.
- **Capture script** (`tools/qualification/capture_active_chain_oracle.py`,
  engine-owned, in the engine commit): the `--removal` scenarios.

Tests:

- `removing_the_pressed_element_matches_chromium`
  (`crates/divui/tests/active_chain.rs`) replays all eight scenarios through
  the public API and compares every step's `:active` ids, the taken-out
  element's `:active` and the event targets. Before the change, the
  removal scenarios differ: nothing is `:active` after the removal.
- `a_removed_active_element_hands_the_chain_to_its_former_parent`
  (`div-engine`, `interaction.rs`): detach, reinsert, release, and the
  removal of an ancestor.

Gap: `moved-node-press-state` (engine). Moving a connected element with
`append_or_move_child` or `insert_before` keeps its press, so the chain
stays on the moved element and the release clicks. Chromium handles the
move as a removal. The oracle test names the scenario and checks that it
still differs. Moves also keep focus, capture and hover in Div, all
unmeasured.

Verification (this branch on main `86b0f987`):

- Linux (WSL): `cargo test --locked -p divui --features "linux testing"
  --test active_chain` passes 3 tests. With the engine change reverted,
  `removing_the_pressed_element_matches_chromium` fails at the first
  removal (no `:active` element where Chromium has `body`, `outer` and
  `middle`), and so does the engine unit test.
- Linux (WSL): `cargo test --locked -p div-engine --lib` passes 118 tests.
- Linux (WSL): `cargo test --locked --no-fail-fast -p div-engine` passes 64
  of 65 suites (530 tests). The failure is `source_layer_raster`'s 1.25x
  pixel (163 against 160), which is this WSL target's Skia build without
  CI's flags (see the 18 November gutter entry).
  `rule_cascade.rs`'s `removing_the_pressed_node_restyles_its_ancestors`
  expected the ancestors to lose `:active` at the removal. It is now
  `removing_the_pressed_node_keeps_its_ancestors_active_until_the_release`,
  which asserts the measured behavior, that the reinserted node is no
  longer `:active`, and the restyle at the release.
- Linux (WSL): `cargo test --locked --no-fail-fast -p divui --features
  "linux testing"` passes 51 of 52 suites (385 tests). The failure,
  `native_rounded_clip`'s pixel comparison, fails the same way with this
  change set aside at main `86b0f987`, from the same local Skia build.
- `python tools/qualification/generate_renderer_contract.py --check` and
  `cargo fmt --all --check` pass.
- No layout, paint or style code changed, only which element `:active`
  matches while a press is held. Neither the census (sheet-free, no input)
  nor coverage can move (decision D4).

Engine review: the engine track approved 77abf23 on 18 November 2026 with
no blocking changes. Its suggestion, a `#pressed:active` rule in the
`rule_cascade` test so the removal restyle stays covered, is in the
follow-up engine commit. It takes `moved-node-press-state` as an engine
filler.

### 18 November 2026: the native root's scrollbar gutters count once (#57 review notes)

Problem (non-blocking notes 1 and 2 of the engine review on #57):

1. `Engine::resize_observation` clamps the native root's border box to the
   root scroll area's client rect, which leaves the viewport's scrollbars
   out. It then still subtracted the root fragment's `element_scrollbars`
   gutters. Layout gives the native root none today, but if it ever did
   (for example from `scrollbar-gutter` on the root), the scrollbars would
   be subtracted twice.
2. No test covered a browser-shaped document's `<html>`.

Change (`crates/div-engine/src/resize_observation.rs`, sent to the engine
track for review): for the native root (the root of a document with no
`<html>`), the gutters are zero after the clamp. Every other element keeps
its gutters.

Tests:

- `the_native_root_counts_its_scrollbar_gutters_once` gives the retained
  root fragment of an `overflow: scroll` root a 15px right and bottom
  gutter, then measures it. Before the change the content box is 278 ×
  198; after it, 293 × 213, the client rect less border and padding, as
  without the gutter.
- `a_browser_shaped_html_matches_chromium_at_1x` builds the oracle's page
  as root, `<html>`, `<body>` and the child, and checks every 1x case of
  `tests/fidelity/fixtures/resize-observer-root-v1/chromium-windows.json`.
  All three match pinned Windows Chromium 147 exactly, inline and block:
  border box 200 × 32, 185 × 32 and 185 × 512. So the gap `root-block-size`
  belongs to the native root alone. This test covers behavior that
  already held and passes before and after the change.

Gap found while probing, not changed: `root-scrollbar-gutter-unmeasured`
(engine). With `scrollbar-gutter: stable` (or `stable both-edges`) and
`overflow: hidden` or `auto`, the native root reserves no viewport gutter
(client 320 × 240 at a 320 × 240 viewport). Whether Chromium reserves one
for `<html>` there has not been captured.

Verification (this branch on main `c65b1245`):

- Linux (WSL): `cargo test --locked -p div-engine --lib resize_observation`
  passes 9 tests. With the change reverted,
  `the_native_root_counts_its_scrollbar_gutters_once` fails (278 × 198
  against 293 × 213).
- Linux (WSL): `cargo test --locked --no-fail-fast -p div-engine` passes
  64 of 65 suites (529 tests). The failure is
  `original_edge_and_all_eight_controls_keep_real_property_boundaries`
  (`tests/source_layer_raster.rs`): at 1.25x one pixel is 163 where
  Chromium is 160. It fails the same way in this WSL checkout with
  `resize_observation.rs` at main `c65b1245`, so it predates this change.
  The Linux CI job passed this suite on #89.
- Linux (WSL): `cargo test --locked -p divui --features "linux testing"
  --test resize_observer` passes 11 tests, the native root's oracle test
  included.
- `python tools/qualification/generate_renderer_contract.py --check` and
  `cargo fmt --all --check` pass.
- No layout, paint or style code changed (decision D4).

Engine review: the engine track approved 2f2b4f4 on 18 November 2026 with
no changes. It takes `root-scrollbar-gutter-unmeasured` as an engine
filler, to measure Chromium before acting.

### 17 November 2026: focusable scrollers and the focus starting point (Tier 0 item I2b)

Problem: Div's sequential focus order had no scrollers, Tab with nothing
focused always started at the first element, and a press focused only its
own target. Chromium puts a scroller with no focusable descendant in the
Tab order and starts Tab from the last click (the I2a oracle's gap
`keyboard-focusable-scrollers`, steps `focus_order` 0 to 9 and
`click_scroller` 1 and 2).

Sources: Chromium 147.0.7727.50's `focus_controller.cc` (Apple/Nuanti
2-clause BSD, header checked alone first), and the already checked
`event_handler.cc` and `mouse_event_manager.cc`, from the local checkout.
They give the press's starting point and focus walk, and the search from a
start that is not in the order. `element.cc` and `document.cc` are LGPL and
were not read: which scrollers count, and where a removed start leaves
navigation, are measured. The code is Div's own.

Oracle (new, `tests/fidelity/fixtures/focus-navigation-v1`), captured by
`tools/qualification/capture_focus_navigation_oracle.py`, which reuses the
keyboard scrolling capture's browser and input code with its own page:

- Real XTEST keys and buttons to pinned Chrome-for-Testing 147.0.7727.50
  on X11 in WSL, at scale 1, captured twice and required to match (68 s).
  18 scenarios.
- They cover eleven scrollers (content that fits, `overflow: scroll` that
  fits, `overflow: hidden`, overflow only on a hidden axis, overflow by
  0.4px and 0.6px, and children with `tabindex="-1"`, `disabled`,
  `display: none` and `tabindex="0"`),
  script focus of each, Tab and Shift+Tab after clicks, removals, `blur()`
  and a cancelled `mousedown`, and a focused scroller starting the
  keyboard scroll chain.
- No Windows capture: it needs the foreground and the user's approval for
  each run, and Blink's focus navigation does not depend on the platform.
  The test replays the Linux capture on every host.

Change, as the new [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 8 specifies:

- **Engine** (its own commit, for engine review; `focus_navigation.rs`,
  `interaction.rs`):
  - A scroller that can scroll on an axis that overflows is focusable. Any
    unsnapped overflow counts: 60.4px of content in 60px is focusable in
    Chromium, though its snapped scroll range is 0. Without `tabindex` it
    is keyboard focusable only when no descendant is (one linear walk:
    a focusable descendant without a negative `tabindex`), and it is
    never mouse focusable.
  - `Engine::press_focus` (new) and `PressFocus`: the nearest mouse
    focusable element from the target, else clear, else keep when the
    press hit a scrollbar.
  - The sequential focus navigation starting point: set by every press
    before dispatch and by every focus; on removal it keeps the removed
    subtree's position. `next_focus_target` starts from the focused
    element, else from it.
  - `has_focus_semantics` (new, public) lets `divui` update layout before
    a script `focus()` that only a scroller rule could allow.
    `advance_focus` updates layout, and so does `focus_with_origin` for
    such an element; `can_focus` and `next_focus_target` read the latest
    layout, as their documentation says.
- **`divui`:** both press paths use `press_focus`. `focus()` updates
  layout first for an element without `tabindex`, control or `summary`
  semantics.
- **Behavior changes:**
  - A press on content that cannot take focus now clears focus, unless it
    hit a scrollbar. Before, focus stayed (RFC 0007, Affected tracks).
  - Scrollers take focus from Tab and `focus()`. Div's accessibility tree
    reports them focusable, since it reads the same rule; Chromium's
    accessibility tree was not measured.
  - Tab and Shift+Tab with nothing focused start after the last click or
    the last focused element, not at the first element.
- **Renderer contract:** `author-style-inventory.json` now lists
  `focus_navigation.rs` for `overflow-x` and `overflow-y`, which it reads
  from the scroll area. It also lists it for `left`, `top`, `right`,
  `bottom`, `width`, `height` and `position`, because the scanner matches
  any `.field` token: geometry fields and `Iterator::position`, not CSS
  reads.

Gaps:

- `keyboard-focusable-scrollers`: closed. Its I2a oracle steps match on
  both platforms and are removed from that test's gap list.
- `focus-navigation-windows-unmeasured`: no Windows focus capture, so
  every rule this oracle measures, the fractional overflow rule included,
  is measured on Linux only. The I2a oracle's Windows focus steps match.
- `scrollbar-press-transformed` (engine, unmeasured): the scrollbar test
  ignores transforms.

Verification (Windows, MSVC, this branch rebased on main `4815e9da`):

- `cargo test --locked -p divui --features "windows testing" --test
  focus_navigation_oracle` passes: every step of the 18 scenarios matches.
  Before the change (engine and `divui` source reverted to `b0c443c7`), 61
  compared fields differ, in 34 steps of 14 scenarios.
- `--test keyboard_scroll_oracle` passes 2 tests with the I2b gap steps
  removed from its list.
- `cargo test --locked -p div-engine --test focus_navigation` passes 4 new
  tests: which scrollers are focusable, fractional overflow included; the
  order around a focusable
  descendant; a press's focus, including a scrollbar; the start after a
  press, a removal and a blur.
- `cargo test --locked --no-fail-fast -p div-engine` passes (66 suites, 534
  tests, 3 ignored).
- The full framework test set passes (65 suites, 666 tests, including
  `div-components`), as do the example builds and the `divui` doc tests.
  No existing test relied on the old press focus.
- Linux (WSL, X11 features): `cargo test --locked -p divui --features
  "linux testing" --test focus_navigation_oracle --test
  keyboard_scroll_oracle` passes 3 tests, and `cargo test --locked -p
  div-engine --test focus_navigation --test keyboard_scroll` passes 9.
- `python tools/qualification/generate_renderer_contract.py --check` and
  `cargo fmt --all --check` pass.
- No layout, paint or style code changed, so the census and coverage are
  not affected (decision D4).

Engine review: the engine track approved 7df62ff6 on 17 November 2026
(read, not built), asking for one change, made: the focusable-descendant
check was recursive, exponential in the depth of nested scrollers, and is
now one linear walk with the same answer. On its suggestion, `advance_focus`
and `focus_with_origin` update layout first. It expected fractional
overflow not to count, as Blink's pixel-snapped sizes would suggest; the
oracle's new `fractionDown` scroller (60.4px of content in 60px) shows
Chromium makes it focusable, so Div compares the unsnapped rectangles. The
engine track approved that change, 5fd659b, on 17 November 2026 (read, not
built). Rebased on main `4815e9da`, the two engine commits are `724d181`
and `88274be`, unchanged.

### 17 November 2026: keyboard scrolling and `keypress` (Tier 0 item I2a)

Problem: arrows, Page Up, Page Down, Home, End and the space bar did nothing
outside form controls, and Div dispatched no `keypress` (sample-markdown
MD-23, the last Tier 0 item). Apps and libraries cancel `keypress` to stop a
character or the space bar's scroll.

Sources: Chromium 147.0.7727.50's `keyboard_event_manager.cc`,
`scroll_manager.cc`, `mouse_event_manager.cc` (Chromium BSD),
`event_handler.cc` (Apple 2-clause BSD) and `scrollable_area.cc`
(Google/Apple 3-clause BSD), from the local checkout, each header checked
alone first. `element.cc` is LGPL and was not read, so scroller focusability
is measured only. The code is Div's own.

Oracle (new, `tests/fidelity/fixtures/keyboard-scroll-v1`), captured by
`tools/qualification/capture_keyboard_scroll_oracle.py` from pinned
Chrome-for-Testing 147.0.7727.50:

- **Input.** Real key input, not CDP:
  - Windows: `WM_KEYDOWN` and `WM_KEYUP` posted to Chrome's window, which
    must be in the foreground (probes showed no unfocused method reaches
    the page). The script checked the foreground before every input and
    would have aborted on a change. It ran once (196 s), with the user's
    approval for that run.
  - Linux (WSL, X11): XTEST, with modifiers (224 s).
- **Coverage.** Scales 1 and 1.25. 19 scenarios on Linux, 17 on Windows:
  posted Win32 modifiers do not reach Chrome, so the two modifier
  scenarios are Linux only. They cover the document, nested scrollers,
  `overscroll-behavior`, `scroll-padding`, a horizontal-only scroller, a
  removed pressed node and its removed ancestor, focused controls and text
  controls, cancelled `keydown`, `keypress` and `mousedown`, sequential
  focus, which keys have a `keypress`, and modifiers.
- **Repeatability.** Each capture was taken twice, and the two had to
  match. The captures differ only in Delete's `keypress` (Linux has one).

Change, as the new [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 7 specifies:

- **Engine** (its own commit, for engine review):
  - `Engine::scroll_logical(direction, granularity)` and
    `ScrollGranularity` (new). They follow `ScrollManager::LogicalScroll`:
    the chain starts at the focused element, else the pressed node, else
    the root. The first scroller that can scroll on the axis and moves
    ends it. A line is 40 px, a page is 87.5% of the pixel-snapped scroll
    snapport, and a document is the whole range.
  - `Engine::mouse_press_node` (new): set on every press, before
    dispatch. When it is removed or detached, its former parent takes
    its place.
  - `scroll_button.rs`: the page-step arithmetic is shared, unchanged.
- **`divui`:**
  - `keypress` follows a keydown that was neither cancelled nor handled.
    It fires for a key that commits text without Ctrl or Meta, for the
    space bar, and for Enter (also with Shift or Ctrl). On Linux, Delete
    has one too.
  - Its default commits the text. On an unconsumed space it pages down,
    or up with Shift.
  - An unhandled keydown scrolls by `MapKeyCodeForScroll`'s rules.
  - `Event::key` reports a `keypress`'s key.
- **Behavior changes:**
  - Committed text now waits for `keypress`, so cancelling `keypress`
    stops it.
  - Arrows, Home and End scroll when no text control has focus. Before,
    they went to the editing commands, which did nothing there.
  - A radio with no other radio to move to lets arrows and Page keys
    scroll.
  - `dispatch_key_event` now runs the `keypress` step too, with no
    committed text.
  - Components that handle these keys themselves must cancel the
    `keydown` (RFC 0007, Affected tracks).

Gaps, named in the oracle test, each still differing:

- `keyboard-focusable-scrollers` (engine and framework, I2b, next):
  `focus_order` 0 to 9 and `click_scroller` 1 and 2.
- `overscroll-behavior` (engine): Div's style system has no
  `overscroll-behavior` (`contain` 2 and 3).
- `edit-caret-reveal` (framework): typing into the textarea scrolled off
  the top does not reveal the caret (`textarea` 4).
- `device-scale-scroll-offset` (engine, RFC 0011): at 1.25 Chromium's
  scrollbars are 19 device pixels and offsets land on device pixels: 14
  steps at 1.25 only on Windows, 15 on Linux.
- Not visible in the settled oracle: `keyboard-smooth-scroll` (engine;
  Chromium animates) and `keyboard-scrollend` (framework; Div has no
  `scrollend`).
- `windows-modified-keys-unmeasured`: Windows uses the modifier rules
  measured on Linux.
- `touch-press-node` (engine and framework, unmeasured; raised in the
  engine review): Div sets the press node on every pointer's press, touch
  included. Blink sets it only in the mouse press path, which touch
  reaches through a tap's synthesized `mousedown`, at the tap.
- `overscroll-behavior` lands separately: the engine track owns the
  `div-style` property and has queued it as an engine filler.

Engine review: the engine track approved 966cb58e on 17 November 2026
(read, not built; the refactor of the page step was checked to be the same
arithmetic). Rebased onto main `b7352743`, it is 44442d16 with the same
`git patch-id`.

Verification (Windows, MSVC, this branch rebased on main `b7352743`):

- `cargo test --locked -p divui --features "windows testing" --test
  keyboard_scroll_oracle` passes 2 tests, both new:
  - the Windows capture replayed through a document at both scales. It
    compares every step's events, `scroll` targets, offsets, focus and
    control values; only the named gap steps differ, and each still does;
  - the two captures differ only in Delete's `keypress` and the
    Linux-only modifier scenarios.

  Before the change (divui reverted, engine kept), 100 steps differ.
- `cargo test --locked -p div-engine --test keyboard_scroll` passes 5
  new tests: the document's line, page and document steps; the chain from
  the pressed node; the removed pressed node; the scroll snapport's
  padding; a hidden axis.
- `cargo test --locked --no-fail-fast -p div-engine` passes (65 suites, 530
  tests, 3 ignored).
- The full framework test set passes (62 suites, 656 tests, including
  `div-components`), as do the example builds, the `divui` doc tests (11)
  and `cargo fmt --all --check`.
- Linux (WSL, X11 features): `cargo test --locked -p divui --features
  "linux testing" --test keyboard_scroll_oracle` passes 2 tests, replaying
  the Linux capture, modifiers and Delete's `keypress` included.
- No layout, paint or style code changed, so the census and coverage are
  not affected (decision D4).
- Renderer contract: `keyboard_scroll.rs` reads `direction`,
  `writing-mode`, `overflow-x` and `overflow-y`, so the regenerated
  `docs/renderer/generated/author-style-inventory.json` lists it as a
  reader of those four. `python tools/qualification/generate_renderer_contract.py
  --check` passes on main `4b20edd6` (CI's first run reported this drift).

### 17 November 2026: the Pointer Events oracle on Linux (follow-up `pointer-events-linux-derived`)

Problem: the Pointer Events oracle (`pointer-events-v1`, #68) had only a
Windows capture. Since click counting (#75), Div fires `contextmenu` on the
secondary button's press on Linux, so on Linux the test moved each expected
`contextmenu` from the Windows capture's release to the press. That
expectation was derived from the click-count capture, not measured.

Capture: `tools/qualification/capture_pointer_events_oracle.py` now also
runs on Linux. It writes `chromium-linux-x11.json` from pinned
Chrome-for-Testing 147.0.7727.50:

- **Browser.** The browser is headed on Ozone's X11 platform (WSLg). CDP
  fixes the 400×300 viewport at scale 1.
- **Input.** The mouse is real X11 motion and buttons through XTEST. Touch
  goes through CDP, as on Windows.
- **Setup per scenario.**
  - Each scenario gets a fresh browser.
  - One motion measures the viewport's origin on the screen, and a second
    motion checks it.
  - The pointer is then parked outside the window and the page is loaded
    again, so the page starts with the pointer outside it, as under CDP.
  - A press or release moves the pointer only when its point changes,
    because a motion to the same point still reaches the page as a move.
  - The page cancels `contextmenu` after the logger has recorded it, so no
    native menu takes the pointer.
- **Repeatability.** The capture runs twice and the two must match. A
  third run reproduces the fixture byte for byte.
- **Windows unchanged.** The Windows path is the same; its recapture
  reproduces `chromium-windows.json` byte for byte.

Result: the Linux and Windows captures match everywhere except where
`contextmenu` falls. Linux fires it right after the press's `mousedown`,
with the buttons held then. Windows fires it after the release's
`auxclick`. This is exactly what the derived expectation assumed, in
`chorded` steps 2 and 3 and `right_click` steps 1 and 2. Pressure is left
out of the comparison, as before.

Change:

- `pointer_events_oracle.rs` compares each host with its own platform's
  capture and checks the capture's `platform`.
- The `contextmenu` move is gone.
- A new test, `captures_differ_only_in_contextmenu`, pins how the two
  captures differ:
  - the same inputs and final focus;
  - the same events apart from `contextmenu`;
  - on Linux, `contextmenu` after `mousedown`; on Windows, after
    `auxclick`;
  - only in those four steps.
- No product code changed.

Verification, on main `d74065cb`:

- Linux (WSL, X11 features):
  - `cargo test --locked -p divui --features "linux testing" --test
    pointer_events_oracle` passes 3 tests.
  - With `CONTEXTMENU_ON_PRESS` forced to `false`, the Linux comparison
    fails in exactly the four steps.
- Windows (MSVC): the same test with `--features "windows testing"` passes
  3 tests.
- `cargo fmt --all --check` passes.
- Recapture: `python3 tools/qualification/capture_pointer_events_oracle.py
  --chrome <pinned Linux chrome>` in WSL reproduces the Linux fixture byte
  for byte. Run with Windows Python, the same script reproduces the Windows
  fixture byte for byte.

### 17 November 2026: click counting, `detail` and `dblclick` (Tier 0 item 0.12 D)

Problem: Div left `detail` at 0 and never dispatched `dblclick`, and
`TestApp::double_click` was two single clicks. The editor's double-click
word selection and any multi-click UI need both (the known gap of 0.12 C).

Oracle (new, `tests/fidelity/fixtures/click-count-v1`), captured by
`tools/qualification/capture_click_count_oracle.py` from pinned
Chrome-for-Testing 147.0.7727.50:

- **Input.** Real platform input, not CDP:
  - Windows: `WM_MOUSEMOVE` and `WM_[LMR]BUTTON{DOWN,UP}` posted only to
    the harness's own window; the real cursor never moves;
  - Linux (WSL, X11): XTEST;
  - touch: CDP on both.
- **Coverage.** Scales 1 and 1.25 (`--force-device-scale-factor`). There
  are 21 mouse scenarios (time, distance, button, modifier, drags, long
  presses, moves between clicks, the box's anchor) and 13 touch scenarios
  (time, distance, a cancelled `pointerdown`).
- **Repeatability.** Each capture was taken twice, and the two had to
  match.
- **Plan correction.** The approved plan's guess was that on Windows a
  move ends a series so that the release clicks with count 0. Both
  captures show otherwise: drags and moving long presses click with
  count 1. Other findings:
  - the Linux count cycles (the fourth press counts 1), rather than
    stopping at 3;
  - Linux fires `contextmenu` on the secondary button's press, Windows on
    its release.

Change, as the new [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 6 specifies:

- **`divui::click_count`** (new):
  - `ClickCounter`, with Chromium's `Windows` and `Linux` rules;
  - `DoubleClickSettings`;
  - `TapCounter`, for the gesture detector's repeated taps.
  The module documentation names each rule's Chromium source; the code is
  Div's own.
- **Documents count.** Each document counts its mouse and pen input in
  device pixels, by the host's rule.
  - `PointerInput::click_count` (new; `with_click_count`) overrides the
    count of a press or release, and gives a touch release its tap
    count.
  - `MouseInfo::detail` (new) is the count on `mousedown`, `mouseup`,
    `click`, `auxclick` and `dblclick`, and 0 elsewhere.
  - A main-button or tap `click` whose count is 2 is followed by
    `dblclick`.
- **Behavior change:** on Linux, `contextmenu` now follows the secondary
  button's `mousedown`, as in Chromium; Windows is unchanged. The Pointer
  Events oracle (#68) is a Windows capture, so on other hosts its test now
  moves each expected `contextmenu` to the press, with the press's buttons,
  as the Linux click-count fixture shows. Its fixture is unchanged.
- **`div-platform`:** `DoubleClickLimits::system()` (new) reads
  `GetDoubleClickTime`, `SM_CXDOUBLECLK` and `SM_CYDOUBLECLK` on Windows.
  It adds the `Win32_UI_Input_KeyboardAndMouse` feature of `windows-sys`.
  Elsewhere it returns Chromium's Linux limits; GTK settings are ignored,
  as approved. The native runtime passes them to the document before
  every pointer input.
- **`TestApp`:**
  - `click`, `right_click` and `tap` give each press count 1, as
    Playwright's `click` does, so a test does not depend on the clock;
  - `double_click` is a real double click (1, 2, then `dblclick`);
  - `triple_click` (new) counts 1, 2, 3.
- The C ABI is unchanged.

Approximations, documented in the code:

- Native input is timed when Div receives it, not by the message's own
  time.
- A touch that moved beyond the tap slop forgets the previous tap. No
  oracle scenario covers this case.

Follow-up:

- **`pointer-events-linux-derived`** (framework, a filler after this
  PR): the Pointer Events oracle has only a Windows capture. On Linux its
  test moves each `contextmenu` to the press, so that part is derived from
  the click-count capture, not measured. The filler captures
  `pointer-events-v1` on Linux (XTEST, pinned Chrome-for-Testing in WSL),
  compares each OS against its own capture and drops the move. Closed on
  17 November 2026 by the Linux Pointer Events capture (entry above).

Verification (Windows, MSVC, this branch on main `85fa7380`):

- `cargo test --locked -p divui --features "windows testing" --test
  click_count_oracle` passes 4 tests. All are new:
  - both fixtures through the counters on a simulated clock (the
    capture's sleeps plus its 10 ms per step), at both scales;
  - the Windows fixture through a document, comparing every pointer and
    mouse event but the boundary events (type, `detail`, `button`,
    `buttons`);
  - the document's own counter on the real clock.

  Before the change, `detail` is 0 and there is no `dblclick`, so the
  fixtures fail.
- `cargo test --locked -p divui --features "windows testing" --lib
  testing::` passes 8 tests, including the new `click_helpers_count_clicks`.
- Mutants, each failing the oracle test:
  - Linux without the cycle;
  - Windows without the move reset;
  - Windows anchored at the previous press;
  - no `dblclick`;
  - `detail` on every event;
  - `contextmenu` on press on Windows.
- These also pass:
  - the full framework test set (60 suites, 642 tests);
  - the example builds;
  - the `divui` doc tests;
  - `cargo fmt --all --check`;
  - the Linux `cargo check` of `div-platform`, `divui` and `div-ffi`
    with their tests (WSL).
- Linux (WSL, X11 features): `cargo test --locked -p divui --features
  "linux testing" --test click_count_oracle` passes 4 tests. Here the
  document replays the Linux fixture, so `contextmenu` follows the
  press. `--lib testing::` passes 8 tests. `--test pointer_events_oracle`
  passes 2 tests; before the test's host adjustment, it failed in four
  secondary-button steps (`chorded` 2 and 3, `right_click` 1 and 2), only
  in where `contextmenu` falls.

### 17 November 2026: licence incident, `mouse_event.cc` (disclosure)

While planning click counting, one command printed lines 385–450 of `third_party/blink/renderer/core/events/mouse_event.cc` (`MouseEvent::DispatchEvent`), from the local Chromium checkout at 147.0.7727.50, in the same command as the header check. An earlier cross-checkout `git grep` for `kDblclick` had also printed two lines of it. The header is the GNU Library General Public License v2, which is copyleft.

- **Use.** Nothing from the file is ported. The one fact it shows, that `dblclick` follows a trusted `click` whose `detail` is 2, will be established by the click-count oracle (real platform input to pinned Chromium) and the UI Events specification.
- **Rule since then:**
  - `head -n 30` runs alone, as its own command, before any other read of a Chromium file, including the local checkout.
  - The header check is never combined with a content print.
  - Greps are limited to files whose header has already been checked.

### 17 November 2026: test input and explicit waits (Tier 0 item 0.12 C)

Problem (sample-git gap 29, interactions T1, dogfood gap 4):

- `TestApp::click`, `type_text` and `press` ran every task to the end
  (`run_until_idle`), so a test could not click Cancel while a fetch ran.
- There were no pointer, drag, wheel or touch helpers.
- `run_until_idle` left `scroll` events undelivered.

Change, as [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 5 specifies:

- **Behavior change:** `TestApp` input returns once its events are
  dispatched and the UI work they made ready has run
  (`TestApp::run_ready_tasks`, the microtask checkpoint's equivalent). It
  no longer waits for timers or background work. `mount` still runs until
  idle.
- **Waits.**
  - `wait_for(condition)` and `wait_for_with(timeout, condition)` run
    tasks, timers and frames until the condition holds. While waiting, a
    frame runs every 16 ms of real time, by the time that passed.
  - On timeout they panic. `testing::DEFAULT_WAIT_TIMEOUT` is 1 s, as
    Testing Library's `waitFor`.
  - `find_by_text` and `find_by_id` wait for their element, then panic if
    it never appears.
  - `find_id` is the non-waiting twin of `by_id`, as `find_text` is of
    `by_text`.
- **Input helpers.** Each goes through `Document::dispatch_pointer_input`,
  as native input does:
  - `pointer_move`, `pointer_down` and `pointer_up`, with a button and
    modifiers;
  - `pointer(PointerInput)`;
  - `right_click`, `double_click` and `drag(from, to, steps)`;
  - `wheel(at, delta)`;
  - `touch_down`, `touch_move`, `touch_up` and `touch_cancel`;
  - `tap` and `touch_drag`, with `testing::FIRST_TOUCH` (id 2);
  - `center_of(element)`.

  `click` still moves the mouse to the element first.
- `run_until_idle` also delivers the pending `scroll` events (T1).
  `advance_frame` already delivered them, at the start of the frame, as
  Chromium does.
- The `default` template's test uses `find_by_text`. The showcase snippet
  (`site/`, website track) awaits no work, so it is unchanged. No
  components test uses `TestApp`.

Known gap: Div dispatches no `dblclick` and leaves `detail` at 0, because
`PointerInput` has no click count. `double_click` is two `click` events
until that lands.

Verification (Windows, MSVC, this branch on main `b0f59f94`):

- `cargo test --locked -p divui --features "windows testing" --lib
  testing::` passes 7 tests. Five are new:
  - input returns while the fetch runs, Cancel drops it, and
    `find_by_text` waits for the loaded row;
  - the timeout messages;
  - waits run frames;
  - the pointer, touch and right-click sequences, with capture during a
    drag;
  - wheel scrolling with its `scroll` events at idle and in a wait.
- Mutants:
  - input that runs until idle fails the fetch test;
  - `run_until_idle` without pending events fails the wheel test;
  - waits without frames fail the frames and wheel tests.
- These also pass:
  - the full framework test set (58 suites, 636 tests);
  - the example builds;
  - the `divui` doc tests;
  - `cargo fmt --all --check`;
  - the Linux `cargo check` of `div-platform`, `divui` and `div-ffi`
    with their tests (WSL).
- The `default` template's generated app (`check_cli_template.py`) is
  left to CI's `cli-template` job: locally its fresh target builds Skia
  from source, which fails here before reaching Div.

### 16 November 2026: Pointer Events (Tier 0 item 0.12 B)

Problem (RFC 0007 section 4, I1; sample-canvas gap 16): a mouse fired only
`mouse*` events, a touch only `pointer*` events, with the mouse as pointer
0. There was no `pointerover`/`pointerout`, no chorded buttons, no
`auxclick` or `contextmenu`, no `button`/`buttons` beyond a flat field, no
offset point, and `setPointerCapture` took effect at once and was never
reported. Focus moved on `pointerdown`, so cancelling `pointerdown` could
not keep focus where it was (gap 16).

Oracle: `tools/qualification/capture_pointer_events_oracle.py` records
pinned Windows Chrome-for-Testing 147.0.7727.50 under CDP mouse and touch
input for 17 scenarios at 1x, each captured twice, into
`tests/fidelity/fixtures/pointer-events-v1/chromium-windows.json`. Moves
send the held button: Chromium's `EventHandler` (`event_handler.cc`, BSD)
releases capture on a move without one. No scenario depends on the device
scale. Chromium sources read: `event_handler.cc`,
`pointer_event_manager.cc` and `gesture_configuration.h` (all BSD) at
147.0.7727.50.

Change:

- **Engine** (reviewed by the engine lane; `interaction.rs`, `lib.rs`):
  capture is pending until the pointer's next event, which reports
  `Got`/`Lost` changes; the caller ends it after an up or cancel
  (`end_pointer_capture`) and hit-tests again (`pointer_rehover`); a
  removed captor loses capture to the root; `set_pointer_capture` fails
  with `PointerNotFound` for an unknown pointer and
  `PointerCaptureDisconnected` for a disconnected node, and does nothing
  without a pressed button. The mouse (id 1) is always active, as in
  Chromium's `PointerEventFactory` (`pointer_event_factory.cc`, BSD), so
  capturing it before its first event does nothing (engine review). Per-pointer
  buttons, the previous target for `out`/`over`, the pressed node on an up,
  and `local_point` (the padding box point through transforms) are new.
- **divui**: the RFC's types (`PointerType`, `PointerId`, `Buttons`,
  `MouseInfo`, `PointerInfo`, `Point`), `Event::mouse_info` and
  `pointer_info`, and `Document::dispatch_pointer_input(PointerInput)`,
  which fires in Chromium's order: capture changes; `pointerout`,
  `pointerleave`, `pointerover`, `pointerenter`, then the mouse's four;
  the pointer event, then the compatibility mouse event; on release
  `lostpointercapture`, then `click` for the main button or `auxclick`
  (and `contextmenu` for the secondary one), then the boundary events of a
  fresh hit test if capture ended. A second button is a `pointermove`. A
  cancelled `pointerdown` suppresses `mousedown`, `mousemove` and `mouseup`
  until release, and focus follows an uncancelled `mousedown` (gap 16). A
  touch is captured by its `pointerdown` target, leaves its hover path when
  lifted, and a tap (moved at most 15px, Chromium's
  `max_touch_move_in_pixels_for_click_`) fires the compatibility mouse
  events and a `click` with the touch's id.
- `Element::set_pointer_capture`, `release_pointer_capture` and the new
  `has_pointer_capture` take a `PointerId`.
- The platform layer gives the mouse id 1 and each touch the lowest free id
  above 1; back and forward buttons are numbered.

Behavior changes:

- The mouse is pointer 1 (was 0) and fires `pointer*` events before its
  `mouse*` events; a touch fires `pointer*` events and, for a tap,
  `mouse*` events.
- A release with no press (a bare `MouseEventType::Up`) no longer clicks,
  as in Chromium. Tests that clicked that way now press first.
- `MouseButton` is the web's: `Main`, `Auxiliary`, `Secondary`, `Back`,
  `Forward`, `Eraser`; `Left`, `Middle` and `Right` are deprecated
  constants. `div-components`' `roving_focus.rs` still uses `Left`, which
  works and warns; the components track owns it.
- `Document::dispatch_pointer_event` and `dispatch_pointer_leave` are
  deprecated wrappers over `dispatch_pointer_input`.
- `set_pointer_capture` for an inactive pointer fails (was a no-op); the
  mouse is always active.
- The C ABI keeps its functions and ids (frozen for v0.2). Its pointer
  listeners now receive only `pointer*` events: the mouse→pointer listener
  aliases are gone because the mouse fires both. Its direct
  `divui_document_dispatch_pointer_event` ends capture after an up or
  cancel.

Named gaps (steps of the replay that must keep differing until fixed):

- `click-common-ancestor` (framework, I2): a release elsewhere clicks the
  common ancestor in Chromium; Div clicks the release target.
- `hover-after-dom-change` (framework): Chromium updates hover and fires
  `lostpointercapture` after a captor's removal without input; Div waits
  for the next event.
- `touch-action-pan` (engine and framework): a touch move pans and cancels
  the pointer in Chromium; Div has no `touch-action`.
- `local_point` for a fragmented box uses its first fragment, and an
  inline-level target gets the client point; Chromium maps both through
  their boxes (engine review note; no sample needs it yet).
- Not compared: pressure (CDP's mouse reports 0, so a unit test pins the
  RFC's 0.5 while pressed) and the screen point (equal to the client point
  headless; a window's position is not applied yet).

WPT `pointerevents` and `uievents/mouse` are testharness script with
`test_driver` actions, so their cases wait for RFC 0005; the oracle covers
the same sequences.

Verification (Windows, MSVC, this branch on main `c174f970`):

- `cargo test --locked -p divui --features "windows testing" --test
  pointer_events_oracle` passes 2 tests. The replay matches all 17
  scenarios outside the 5 gap steps, which still differ; the other pins the
  RFC's pointer defaults.
- `cargo test --locked -p div-engine --test pointer_capture` passes 5
  tests: pending capture, the capture errors (with the always-active
  mouse, which fails without it), a removed captor, a cancel, and local
  points through a border and a transform.
- Mutants: ignoring a cancelled `pointerdown` fails 5 replay steps and the
  gap 16 focus check; boundary events without the input's `button` fail 4
  steps. On main the replay does not compile (no Pointer Events API).
- The full framework test set passes (58 suites, 630 tests), as do the
  example builds, the engine suite (62 suites, 507 tests), `cargo fmt --all
  --check` and the Linux `cargo check` of `div-platform`, `divui` and
  `div-ffi` with their tests in WSL.

### 16 November 2026: `scroll_metrics.c` against real-zoom Chromium

Problem: `examples/c/scroll_metrics.c` asserts the same Chromium values at
1x, 1.25x, 1.5x, 2x and 3x, but the values above 1x came from CDP-emulated
device scale. Since #59 non-1x oracles use `--force-device-scale-factor`
(RFC 0010; the fidelity lane's addendum lists this example).

Oracle: `tools/qualification/capture_scroll_metrics_oracle.py` loads the
example's `viewport`, `nested` and `integer_api` scenarios as HTML in pinned
Windows Chrome-for-Testing 147.0.7727.50, launched with
`--force-device-scale-factor` at each scale. It records `clientWidth`,
`clientHeight`, `scrollWidth`, `scrollHeight`, `scrollLeft`, `scrollTop` and
`getBoundingClientRect()` at every step, captures each scale twice and
requires them to match. The output is
`tests/fidelity/fixtures/scroll-metrics-v1/chromium-windows.json`.
`integer_table_api` runs at 1x only and is not recaptured.

Result: at 1x, 2x and 3x Chromium matches every value the example asserts.
At 1.25x and 1.5x it lays out in device pixels and 25 values differ. They
are named gaps, not tolerances, all owned by the engine under RFC 0011:

- **`device-scale-scroll-offset`** (already named by the 0.9 wheel work, #63):
  scroll offsets end on whole device pixels. The root's 22×21 range ends at
  22.4, 21.6 at 1.25x and 22.667, 21.333 at 1.5x; the nested scroller's 50
  at 1.25x is 50.4 (−49.6 in RTL). Div keeps CSS pixels.
- **`device-scale-border-width`**: zoomed border widths are floored to whole
  device pixels (`StyleBuilderConverter::ConvertBorderWidth` in
  `style_builder_converter.cc`, BSD; licence header checked first, read from
  the local checkout). A 3px border is 2.4px at 1.25x and 2.667px at 1.5x, so
  the nested child is at (2, 12.4) and (2.667, 12.667), not (3, 13); a 2px
  border is 1.6px at 1.25x, so the transformed scroller's bounds are
  207.4×251.1, not 209×253.5.
- **`device-scale-scrollbar-thickness`**: the classic scrollbar is
  round(15 × scale) device pixels, 15.2px at 1.25x and 15.333px at 1.5x
  (measured with a full-size child of the scroller). The 100.5px scroller's
  client width is then 85.3px or 85.167px, which `clientWidth` reports as 85,
  where Div reports 86 (and 65 for 66).

`crates/divui/tests/scroll_metrics_oracle.rs` runs the three scenarios
through `divui` at each scale and compares every captured value. A value in
its `GAPS` table must still differ, and every other value must be equal.
The C example keeps Div's values; its comments now cite the fixture and the
gaps instead of claiming Chromium has those values at all five scales.

Verification (Windows, MSVC, this branch on main `79b8c25d`): `cargo test
--locked -p divui --features "windows testing" --test scroll_metrics_oracle`
passes. Moving the 1.5x scrollbar entry out of `GAPS` fails it with the four
85 versus 86 values. `clang-format --dry-run --Werror` (18.1.8) passes on
the C example. `python tools/qualification/capture_scroll_metrics_oracle.py
--chrome <pinned chrome>` reproduces the fixture byte for byte. The full
framework test set passes (57 suites, 628 tests), the example apps build,
`cargo fmt --all --check` passes, and the Linux `cargo check` (WSL) passes.

### 16 November 2026: native wheel direction, pixels per notch and `deltaMode` (Tier 0 item 0.9)

Problem (sample-interactions F2, I3): native wheels scrolled the wrong way,
since `div-platform` passed winit's deltas (positive when the content moves
down) as the DOM's (positive when the view does). A notch was 40px on every
platform, Shift+wheel did not scroll horizontally, and events had no
`deltaMode`.

Oracle: `tools/qualification/capture_wheel_oracle.py` gives pinned Chrome
147.0.7727.50 real platform wheel input, not CDP's, which takes deltas in
CSS pixels and skips the conversion under test.
- Windows: `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` are posted to the headed
  browser window, with the key state in `wParam`. The window is made topmost,
  because Chromium drops a wheel whose point is not over its own window.
- Linux: X11 wheel buttons 4 to 7 and the Shift key go through XTEST, with
  `--ozone-platform=x11` on WSLg's X server.
- Each OS is captured at 1x and 1.25x with `--force-device-scale-factor`
  (RFC 0010), and every capture runs twice and must match.
- Output: `tests/fidelity/fixtures/wheel-events-v1/chromium-windows.json` and
  `chromium-linux-x11.json`.

Chromium's behavior, from the capture:
- **Notch size:** 100px per notch on Windows (the default 3 lines, at 100/3
  px a line, and 3 characters horizontally) and 120px on Linux, in CSS px at
  both scales. `deltaMode` is 0, and `button` and `buttons` are 0.
- **Shift+wheel:** the event keeps `deltaY`, and the page scrolls
  horizontally.
- **Scroll limit:** a scroll moves at most the scroller's client size, so 3
  notches (300px) moved a 290px-high scroller 290px.

Ctrl+wheel is not captured: Chromium zooms the page, and whether the zoom
lands before the event varies between runs.

Change:
- `div-platform` (`wheel.rs`): `WheelSettings` converts winit's line deltas
  as Chromium does: `WebMouseWheelEventBuilder::Build` on Windows, which
  reads `SPI_GETWHEELSCROLLLINES`/`CHARS` on every event, and
  `kWheelScrollAmount` on Linux. Touchpad pixel deltas are flipped.
  `PlatformEvent::Wheel` carries `delta_mode`, and Windows' page setting
  reports pages.
- `divui`:
  - `Event::delta_mode` (`WheelDeltaMode`), and wheel events report button 0.
  - `Document::dispatch_wheel_event_with_mode` scrolls horizontally for
    Shift with no horizontal delta (`MouseWheelEventQueue`), and doesn't
    scroll for Ctrl (`GetPlatformSpecificDefaultEventAction`; Div has no
    page zoom).
- **Engine** (`crates/div-engine/src/lib.rs`, in its own commit for the
  engine track's review): `scroll_wheel_by` and `scroll_wheel_from_target_by`
  take a `WheelGranularity`. Following cc's `LimitDeltaToScrollerSize`
  (`kLimitScrollDeltaToScrollerSize`) and `ResolveScrollGranularityToPixels`,
  each scroller in the chain limits a pixel or line scroll to its client
  size, resolves a line as 40px, and resolves a page as
  0.875 × min(scroller, viewport). These are clean-room readings of the
  BSD-licensed `cc/input` sources.
- **C ABI:** the frozen v0.2 raw event has no delta mode, so Windows'
  page-mode notch reaches C as one pixel.

Named gaps, each with its owner. The last three are from the engine
track's review of the engine commit on #63.
- **`device-scale-scroll-offset`** (engine, RFC 0011): at 1.25x Chromium
  snaps scroll offsets to device pixels, so a 50px scroll ends at 150.4. Div
  keeps CSS pixels.
- **`wheel-cancelable`** (framework): with only passive listeners,
  Chromium's wheel events are not `cancelable`. Div's are, though passive
  listeners still cannot cancel them.
- **Untested sources** (framework): X11 smooth-scroll valuators, Wayland,
  and Ctrl with precise touchpad deltas follow source reading only.
  `wheelDeltaX`/`wheelDeltaY` and `deltaZ` are not exposed.
- **`wheel-limit-unsnapped-client`** (engine, RFC 0011): cc limits a scroll
  to `container_bounds`, a whole-pixel `gfx::Size` (device pixels under
  zoom for DSF). Div limits to the unsnapped `LayoutUnit` client size, so
  the two differ for fractional client sizes.
- **`wheel-page-unmeasured`** (framework, follow-up filler): the page
  values, for an element (whole versus fractional offsets) and for the root
  viewport (which size stands for its `container_bounds`), come from cc's
  source, not the oracle. A page wheel needs Windows' "one screen at a time"
  setting. The capture script won't change that system setting, so the
  filler's real-input capture runs with it set by hand.
- **`wheel-transformed-scroller`** (engine, already a gap): cc limits and
  resolves deltas in the scroller's local space, after the inverse screen
  transform. Div applies wheel deltas untransformed (before this change
  too), so a transformed or zoomed scroller differs.

Verification (Windows, MSVC, on main `eed71eab`):
- `cargo test --locked -p divui --features "windows testing" --test wheel_events`
  passes 2 tests. One replays both oracles at both scales, the other covers
  Ctrl, lines and pages from source.
- `cargo test --locked -p div-engine --test wheel_scroll_limits` passes 3
  tests.
- `cargo test --locked -p div-platform --features windows --lib wheel`
  passes 3 tests.
- Mutants: the old direction fails the oracle test; no Shift swap fails both
  divui tests; no clamp fails 2 engine tests.

### 16 November 2026: listener handles and options (Tier 0 item 0.12 A)

Problem (sample-editor gap 29): `Element::on` and `on_capture` returned
nothing, and `remove_event(type)` removed every listener of the type, so a
port could not remove one listener, as `removeEventListener` does. There
were no listener options: `once` and `passive` were missing.

Change, following Chromium 147's `EventTarget` (`event_target.cc`, BSD:
`FireEventListeners` and `SetDefaultAddEventListenerOptions`):

- `on` and `on_capture` return a `ListenerHandle`. It is a plain handle:
  dropping it leaves the listener, and `remove()` removes exactly that
  listener. Removing it again, after its element was removed or after the
  document is gone does nothing. `el.on(..)?;` still compiles.
- `Element::on_with(type, ListenerOptions { capture, once, passive },
  callback)`:
  - a `once` listener is removed before it is called, as Chromium does,
    so a nested dispatch doesn't reach it;
  - `prevent_default` inside a passive listener does nothing (the DOM's
    "in passive listener flag");
  - `passive: None` is the web's default: `touchstart`, `touchmove`,
    `wheel` and `mousewheel` listeners on a top-level node are passive.
    Documents made through `divui` are native, so their root stands for
    the window, document, `<html>` and `<body>`.
- **Behavior change:** a `wheel`, `mousewheel`, `touchstart` or
  `touchmove` listener added with `on` to `Document::body()` can no longer
  cancel the event, as in Chromium; pass `passive: Some(false)` to cancel.
- A listener removed during a dispatch is not called for the rest of it;
  one added during a dispatch waits for the next, as before.
- `remove_event(type)` stays and is documented as having no web
  equivalent. Listeners added through the C ABI keep their behavior (the
  ABI is frozen for v0.2).

WPT `dom/events` (`AddEventListenerOptions-once.any.js`,
`AddEventListenerOptions-passive.any.js`) is testharness script, so its
cases wait for RFC 0005; the tests cover the same behavior.

Verification (Windows, MSVC, this branch on main `33f1828d`): `cargo test
--locked -p divui --features "windows testing" --test listener_options`
passes 7 tests: a handle removes exactly its listener, dropping a handle
keeps the listener, removal during dispatch, `once`, passive
`prevent_default`, Chromium's passive defaults on the root (and `passive:
Some(false)` overriding them), and `capture` in the options. Disabling each
of `once` removal (the nested dispatch then recurses until the test aborts),
the passive flag, the passive default and removal by id fails the tests that
cover it. The full framework test set, the example builds and `cargo fmt
--all --check` pass.

### 16 November 2026: observing the native root leaves the viewport scrollbars out

Problem (from the 0.11 engine review on #52): observing a native
document's root (`Document::body`) reported the viewport's size, with the
viewport's scrollbars inside its content box. Div lays out the root's
content inside them: at a 200 × 100 viewport with `overflow: scroll`, a 1px
border and 5px padding, `clientWidth` is 185 and an auto-width child is 173
wide, but the entry said 200 and 188.

Change: `Engine::resize_observation`
(`crates/div-engine/src/resize_observation.rs`, reviewed by the engine
track) observes the native root as the browser's
`<html>`: its border box is the root scroll area's client rect. RFC 0007
section 3 and `ResizeObserver::observe` say so. The same PR fixes review
notes 3 (RFC wording: each entry has one size because Chromium 147 reports
one stitched size) and 4 (a blank line). The engine track takes note 1, a
broader Chromium oracle for device-pixel snapping.

Oracle: `tools/qualification/capture_resize_observer_root_oracle.py`
captures pinned Windows Chrome-for-Testing 147.0.7727.50 observing `<html>`
styled as the test styles the root, with `overflow` `visible`, `scroll` and
`auto` (500px of content), at 1x and 1.25x, into
`tests/fidelity/fixtures/resize-observer-root-v1/chromium-windows.json`.
Each scale launches Chrome with `--force-device-scale-factor`: with CDP
emulation alone Chrome 147 laid out at 1x scale at 1.25x
(`devicePixelContentBoxSize` 173 rather than 217). This is the real-zoom
oracle of RFC 0010 (#55).

Result: at 1x the inline border, content and device-pixel sizes and the
content rect's origin match Chromium exactly (200, 188 and 188; 185, 173
and 173 with a scrollbar). Two named gaps remain, pinned with their numbers
in `the_native_root_matches_chromium_for_the_root_element`:

- `root-block-size`: the root's block size is the viewport's, less a
  horizontal scrollbar. Its border box is 100, 85 and 100 where `<html>`'s
  `auto` height gives 32, 32 and 512.
- `device-scale-edges`: at 1.25x Chromium lays out at device scale, so a
  1px border takes 0.8px and the scrollbar 15.2px. The inline content box
  is 188.390625 and 173.1875 (236 and 217 device pixels) against Div's 188
  and 173 (235 and 216). This is engine-wide.

Verification (Windows, MSVC, this branch on main `82d27bec`):
`cargo test --locked -p div-engine --lib resize_observation` passes 7 tests
and `cargo test --locked -p divui --features "windows testing" --test
resize_observer` passes 11. With the fix disabled,
`the_native_root_leaves_the_viewport_scrollbars_out` and
`the_native_root_matches_chromium_for_the_root_element` fail (`scroll` at
1x reports 200, 100, 188, 88, 188 and 88 instead of 185, 85, 173, 73, 173
and 73). A second capture reproduces the fixture byte for byte. The full
framework test set, the example builds, `cargo fmt --all --check` and the
regenerated style inventory (unchanged) pass.

### 16 November 2026: dropping a `TextNode` no longer removes it

Problem (reported by the fidelity lane): a text node made with
`Element::create_text_child` never painted, even directly under `body`,
while `set_text` with the same text did. `create_text_child` returned a
`TextNode` whose `Drop` removed the node from the document
(`remove_on_drop`, set only there), so `element.create_text_child("…")?;`
removed the text when the statement ended, and a named handle removed it
at the end of its scope. The only code relying on this was `view!`'s
dynamic text (`view_child.rs`), which dropped its handle in `on_cleanup`.

Change: `TextNode` is a plain handle, as `Element` is and as a reference to
a node is on the web. Dropping it never removes the node; `remove()` does.
The `remove_on_drop` flag and `TextNode`'s `Drop` are gone, and the dynamic
text's scope cleanup calls `TextNode::remove()`. No other handle type
removes on drop: `Element`, `WeakElement`, `WeakTextNode` and
`ScrollButton` have no `Drop`, and `divui` has no comment or fragment
handles.

Verification (Windows, MSVC, this branch on main `154d0a72`): `cargo test
--locked -p divui --features windows --test text_child_ownership` passes 3
tests: a dropped or discarded text child stays (text content `droppedkept`),
a text child renders the same PNG as `set_text` with the same text (and not
a blank one), and `view!`'s dynamic text is removed when its scope is
disposed and no longer updates. On main's code the first two fail and the
third passes. With the fix and a cleanup that only drops the handle, the
third fails. The full framework test set, the example builds and `cargo fmt
--all --check` pass.

### 15 November 2026: `ResizeObserver` (Tier 0 item 0.11)

Problem: Div had no way to learn that an element's box changed size, so
components that size themselves to their container (charts, virtual
lists, editors) had to poll `bounding_rect` after each frame (platform
plan item O1).

Change, as [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 3 specifies, with the engine part in this PR as the parent agreed
(the engine track reviews it):

- **Engine** (`crates/div-engine/src/resize_observation.rs`, new; a
  `mod` and re-export in `lib.rs`; `collect_boxes`, `source_parts` and
  `stitched_size` in `element_dimensions.rs` become `pub(crate)`;
  `docs/renderer/generated/author-style-inventory.json` is regenerated with
  `generate_renderer_contract.py`'s `style_inventory`, which lists the new
  file as a reader of three style fields):
  `Engine::resize_observation(handle) -> ResizeObservation`, read-only after
  layout. It reports Chromium 147's `ResizeObserverUtilities` and
  `ResizeObserverEntry` values (`third_party/blink/renderer/core/resize_observer/`):
  - border and content box sizes in the element's writing mode;
  - the content box in device pixels, `SnapSizeToPixel` at the first box's
    paint offset;
  - the physical content rectangle;
  - the flat-tree depth.

  An element without a layout box (`display: none` or `contents`, a
  non-atomic inline, a detached element) reports zeros. The engine keeps
  no observers.
- **Framework** (`crates/divui/src/resize_observer.rs`, new):
  - `ResizeObserver::new(&document, callback)`, with `observe(&element, ResizeObserverBox)`,
    `unobserve` and `disconnect`;
  - `ResizeObserverEntry`, carrying all three sizes and the content rectangle.

  After `begin_frame`'s layout the document gathers targets at depth 0,
  delivers them (document current, untracked), lays out again and gathers
  deeper targets until none is active, as the specification's "update the
  rendering" steps do. Every target is delivered once when first observed
  (the initial last-reported size is -1 × -1, Chromium's
  `kInitialObservationSize`). Entries use the sizes measured when they were
  gathered, as Chromium's entries read the layout of that pass.

  Skipped targets report "ResizeObserver loop completed with undelivered
  notifications." and wait for the next frame. A new or skipped
  observation makes the window want a frame.
- **Errors** (`crates/divui/src/reported_error.rs`, new):
  - `Document::on_error(handler)` adds a handler for `ReportedError`s;
  - with no handler, errors go to standard error;
  - `TestApp::take_reported_errors` collects them in tests.
- **RFC 0007 amended:**
  - `device_pixel_content_box_size` is a plain `ResizeObserverSize` on every
    entry, as Chromium's `ResizeObserverEntry` constructor fills it;
  - `on_error` handlers are added rather than replaced.

Verification (Windows, MSVC, this branch on main `ee7907e8`):

- `cargo test --locked -p div-engine --lib resize_observation` passes 6
  tests:
  - border and content boxes and the content rect, with `box-sizing`;
  - zeros for `display: none` and `contents`, a non-atomic inline and a
    detached element, and a box for `inline-block`;
  - `vertical-rl` swapping the axes;
  - a stable scrollbar gutter leaving the content box;
  - device-pixel snapping at offsets 0 and 10.5 at scale 1, 1.5 and 2;
  - depth.
- `cargo test --locked -p divui --features "windows testing" --test
  resize_observer` passes 10 tests:
  - first delivery and resize, including a content-box-only observation
    reporting the device-pixel size;
  - only the observed box's change;
  - a box-less target delivered once as 0 × 0;
  - entry and observer order;
  - `unobserve`, `disconnect` and drop;
  - a deeper target resized by a callback delivered in the same frame;
  - a shallower one waiting a frame with the loop error;
  - the document current and untracked;
  - the frame request;
  - `TestApp::take_reported_errors`.
- With the depth rule removed (every changed target active), the two
  loop-error tests fail.
- The full framework test set, the example builds and `cargo fmt --all
  --check` pass.

WPT `resize-observer` is testharness script, so its cases wait for the
translation in [RFC 0005](../../alpha/rfcs/0005-wpt-script-translation.md).
SVG targets (bounding-box sizes) follow with RFC 0009.

### 13 November 2026: `requestAnimationFrame` (Tier 0 item 0.10)

Problem: Div had no `requestAnimationFrame`. The nearest hook,
`PlatformEvent::Presented`, ran after a frame was shown and only for
`App::builder()` windows, and a headless test could render at a time but
not step frames. Five of the six sample investigations need it (R1).

Change, as [RFC 0007](../../alpha/rfcs/0007-frames-resize-observer-pointer-events.md)
section 2 specifies:

- `Document::request_animation_frame(FnOnce(f64)) -> AnimationFrameHandle`
  and `cancel_animation_frame`. `Document::begin_frame` runs the callbacks
  after the animations and their events and before style and layout, in
  request order, each with the frame's time. A callback requested during
  the run waits for the next frame, and a cancel stops a callback due
  later in the same frame. Callbacks run with their document current and
  untracked (`runtime::untracked`, new and crate-private).
- A pending callback makes the window want a frame
  (`PlatformApplication::is_animating`), so it works in every window,
  `App::builder()` and `AppSpec` alike. `Document::is_animating` keeps its
  meaning (engine animations only).
- `HeadlessApp::advance_frame(delta_ms)` and `advance_frames`, and the same
  on `TestApp` (which then runs the ready UI tasks). `TestApp::render_png`
  now renders at the last frame's time instead of 0, so stepping frames
  and then rendering does not rewind the animations.

Verification (Windows, MSVC, this branch on `dd8ba04d`): `cargo test
--locked -p divui --features "windows testing" --test animation_frames`
passes 7 tests: request order and frame time, a callback that requests
itself running once per frame, cancel before and during the frame and of
stale or foreign handles, the document current and its animation time
already advanced, no subscription for the effect that runs the frame,
headless `advance_frame(s)`, and `TestApp` stepping a callback that
resizes an element and rendering at that time. With the callback step
removed from `begin_frame`, 6 of the 7 fail; without `untracked`, the
seventh fails. The `divui` unit test
`a_pending_animation_frame_callback_asks_the_window_for_a_frame` passes.
The full framework test set and the example builds pass on Windows, and
`cargo fmt --all --check` passes. WPT `html/webappapis/animation-frames`
is testharness script, so its cases wait for the translation in
[RFC 0005](../../alpha/rfcs/0005-wpt-script-translation.md).

### 13 November 2026: zbus builds with a fresh lockfile (sample-git gap 24)

Problem: `div-platform` pinned `zbus = "=5.13.0"` (the Linux `dialog`
feature) but not the crates zbus asks for with caret ranges, so an app
that resolved its own lockfile took `zbus_macros` 5.19.0 and `zvariant`
5.15.1, and zbus failed to compile on Linux with 8 errors (`E0425`,
`E0433`: `DispatchResult2` missing from `zbus::object_server`). Div's CI
never met this, because every build, the cli-template job included, used
Div's lockfile.

Change: `div-platform` pins `zbus_macros = "=5.13.0"`, `zvariant =
"=5.9.0"` and `zvariant_derive = "=5.9.0"` beside zbus, Linux-only and
behind `dialog`, at the versions in Div's lockfile. `Cargo.lock` gains only
the three dependency lines. `tools/framework/check_cli_template.py` gains
step 5: for the `default` template it discards the seeded lockfile, runs
`cargo generate-lockfile` and `cargo check --all-targets`, and reports
which packages resolved differently from Div's lockfile. It runs in the
existing cli-template job on Ubuntu and Windows. The lockfile hash in
`docs/v02/generated/dependency-policy.json` is regenerated with
`python3 tools/release/generate_v02_contract.py`; without it the first CI
run failed `--check` in windows-msvc and python-accountability.

Verification (this branch rebased on `ac4f40d8`): a scratch app depending
only on `div-platform` with `dialog` (`out/zbus-probe`), with a fresh
lockfile, in WSL (Ubuntu): on `main` `cargo check` fails with the 8 zbus
errors; with the pins the resolve takes `zbus_macros` 5.13.0, `zvariant`
5.9.0 and `zvariant_derive` 5.9.0 and `cargo check` passes (12.6 s).
`cargo check --locked -p div-platform` passes with `linux,dialog,menu`
(WSL) and `windows dialog menu` (Windows, MSVC). The full step 5 runs in
the PR's cli-template jobs.

### 13 November 2026: views as `{expr}` children (dogfood gap 2)

Problem: `view!` turned every `{expr}` child into text through `ToString`,
so `{For(..)}`, `{Show(..)}` and `{children}` could not be children. The
sample app wrapped them in a `Slot` component.

Change: the expression's type decides. Text (`ToString`) is a text node
re-run in an effect, as before; a view (`IntoView`) is built once and
mounted as itself; anything else is a compile error naming the type.
`view!` evaluates the expression into the doc-hidden
`div::__view_child::Value` and method resolution picks the impl (autoref
specialization). A closure's `Fn`/`FnOnce` kind cannot steer that choice,
so the macro reads the expression by its shape: a variable moves into a
held value the closure borrows (text reads it in place, a view takes it
once), another place such as `{props.title}` is borrowed as before (text
only), and any other expression is evaluated by value. Text expressions
that compiled before compile to the same text.

Verification (Windows, MSVC, this branch rebased on `565db46a`): `cargo
test --locked -p divui --test view_children` passes 4 tests (`For` and
`Show` children updating with their signals, a top-level `For`, text from
a signal, a variable, a `Path::display()`, a field behind an `Rc` and an
owned field staying reactive or static as before with no wrapper
elements, and a `ViewNode` variable, a borrowing `Vec<ViewNode>`, an
`Option<ViewNode>` and a `children: Vec<ViewNode>` prop as views). With
the `view!` from before this change the same file fails to compile, with
7 errors of the form "`ViewNode: ToString` is not satisfied". `cargo test
--locked -p div-macros` passes. A `Signal` written as `{count}` reports
"`Signal<i32>` in a `view!` `{..}` child is neither text (`ToString`) nor a
view (`IntoView`)". The full framework test set and the example builds
pass on Windows.

### 13 November 2026: `on:` on a component tag (dogfood gap 1)

Problem: `view!` dropped every `on:` attribute on a component tag, so
`<Button on:click=...>` compiled and the handler never ran. `style:` and
valueless attributes were dropped the same way.

Choice: forward the listener rather than reject it. `on:event=handler` on a
component goes on the element the component returns, or on each top-level
element of a fragment, through the doc-hidden `div::__forward_listener`.
Reasons: it is what the attribute says when the component renders one
element (as in Leptos), it makes `div-components`' `Button` and every other
component clickable without a handler prop in each, so the components
track needs no API change, and bubbling means clicks inside the component
reach it. `Show`, `For` and `DynChild` render a `display: contents`
container, which receives the listener. A component that returns no
element (only text, or nothing) panics when built, naming the component
and event, instead of dropping the listener. `style:` and valueless
attributes on a component are now compile errors naming the component.

Verification (Windows, MSVC, this branch rebased on `774cf940`):
`cargo test --locked -p divui --test component_events` passes 4 tests (a
root element, clicks bubbling from inside it, each element of a fragment,
a `Show` container, the panic). With the forwarding switched off, as on
`main`, all 4 fail (no clicks recorded; no panic). `cargo test --locked -p
div-macros` passes, including new tests of the forwarding and the two
compile errors.

### 12 November 2026: key names and `repeat` (RFC 0003, request 2)

Change: one always-compiled `div::Key` (it was `menu::Key`, behind the
`menu` feature, plus a copy in `testing`; both now re-export it, and it is
in the prelude). `Event::key()` answers as the DOM's `KeyboardEvent.key`
for `keydown` and `keyup`: named keys come from the key code, the space bar
is `Space`, and a character keeps the case it was typed in, falling back to
the code for a letter or digit sent without text. `Event::repeat` is true
for a `keydown` the platform repeats; `App` now passes winit's flag on
instead of dropping it. `Document::dispatch_key(event_type, key, modifiers,
repeat)` sends a key as the native adapter does (code, DOM name as key
text, typed text on keydown), and `TestApp::press` uses it, so headless and
native named keys carry the same key text.

Verification (Windows, MSVC, this branch, rebased on `322db82`):
`cargo test --locked -p divui --features "menu testing windows" --lib --
keyboard native_key_repeat menu:: testing::` passes the new tests (the key
table both ways, dispatched names and repeat on keydown and keyup, key
input with and without key text, a click reporting no key, characters and
Space typing into an input, native repeat reaching keydown listeners) and
the existing menu accelerator and `TestApp` tests. The Windows CI test set
passes.

### 12 November 2026: `Element::contains` (RFC 0003, request 7)

Change: `Element::contains(&self, other: &Element) -> Result<bool, Error>`
answers as the DOM's `Node.contains`: inclusive, true for any descendant,
false for an element of another document. It walks `other`'s parents in the
engine, so a detached subtree answers the same way, and it runs no layout.
Either element being removed is a `StaleHandle` error, as for
`is_same_node`. The components track's dismissable layer can replace its
parent walk with it.

Verification (Windows, MSVC, this branch):
`cargo test --locked -p divui --lib contains` passes
`element::tests::contains_is_inclusive_and_follows_the_parent_chain`
(self, child, grandchild, sibling, reversed order, a detached subtree,
another document, removed elements).

### 10 November 2026: `default` template and test utilities

Change: `div::testing` (`testing` feature, for dev-dependencies) wraps
`HeadlessApp` as `TestApp`: `mount`, lookups (`by_id`, `by_text`,
`find_text`), input (`click` presses and releases the primary button at an
element's centre through hit testing; `type_text`, `press`, `press_with`)
and `run_until_idle`, which every helper runs after its input. Helpers
panic with what was missing. `assert_snapshot!(app, "name")` compares exact
RGBA, decoded with `png` 0.18.1 (already locked), with
`tests/snapshots/name.<os>.png` in the calling crate. A missing or
different snapshot fails unless `DIV_UPDATE_SNAPSHOTS=1`; a mismatch
writes `name.<os>.actual.png` beside it.

`div new` gains the `default` template, now the template used when none
is named: an `App` with a shared `State`, a menu bar (Open... with a file
dialog, Close Window, Quit, the Edit roles, Reset Count enabled reactively,
Check Connection over HTTP, About), a root view and `tests/ui.rs` with a
click test and a snapshot test. A small `src/lib.rs` holds the state and
modules so the tests can import the views. Templates choose their own
`openui` features through a new `{{framework_source}}` placeholder; `new`
also fills `{{app_id}}` and prints template-specific next steps (recording
the first snapshots). `tools/framework/check_cli_template.py` now checks
both templates, and for `default` requires that `cargo test` fails on the
missing snapshot, records it with `DIV_UPDATE_SNAPSHOTS=1`, then passes
against it.

Also fixed: `HeadlessApp::run_ready_tasks` carried `run_until_idle`'s
documentation.

Verification (Windows, MSVC, this branch): `cargo test -p div-cli`
passes 26 tests; `cargo test -p divui --features testing --lib testing`
passes 2. An app generated with `div new demo --framework-path .`
(template `default`) builds; its first `cargo test` passes the click test
and fails the snapshot test with "there is no accepted snapshot" and writes
`first-screen.windows.actual.png`; `DIV_UPDATE_SNAPSHOTS=1 cargo test`
passes and records `first-screen.windows.png` (5,277 bytes); a plain
`cargo test` then passes 2 of 2, and the dev executable renders its first
frame with `DIV_RENDER_PNG`.

Not done: dialog stand-ins for headless tests, promised in the file dialog
entry, are not in this PR.

### 10 November 2026: HTTP client

Change: `div::http` (`http` feature, which enables `async`) sends
requests with `get`, `post`, `put`, `patch` and `delete`, or through a
`Client` built with a timeout, connect timeout, default headers, user agent,
redirect limit and body limit (10 MiB by default). A `Request` adds
headers, percent-encoded query pairs, a timeout and a raw, text or JSON
body. `send().await` runs it on the blocking pool; `send_blocking()` runs it
on the calling thread. A `Response` holds the status, final URL, headers and
whole body, with `text`, `json` and `error_for_status`; every status is a
response until that call. Errors are a small `http::Error` enum (invalid
URL, host not found, connect, timeout, TLS, protocol, body too large,
status, decode, I/O).

It uses ureq 3.5.1 with rustls 0.23.46 on the ring provider, passed to
ureq explicitly so a process-wide provider set by another crate cannot
change it, and rustls-platform-verifier, so certificates are checked
against the Windows certificate store and the Linux system bundle. The
lockfile gains 24 entries: 15 are built on Windows and 17 on Linux, all
MIT, Apache-2.0, ISC or BSD-3-Clause with MSRV 1.85 or lower. The rest are
macOS, Android or wasm32 dependencies that are locked but never built,
including `webpki-root-certs` (CDLA-Permissive-2.0, wasm32 only). ureq's
`json` feature is not used: it names `cookie_store` weakly, which makes
Cargo lock cookie_store, url and ICU 2 (Rust 1.88) without building them;
JSON goes through serde_json instead. New example: `fetch`.

Verification:

- `cargo test -p divui --features http --lib http --test http` (Windows):
  query encoding and status handling; against a local server, a GET sends
  its query, client and request headers and user agent and reads status,
  headers and body; a JSON POST round-trips; a 404 is a response until
  `error_for_status`; a stalled server times out, an oversized body fails,
  a closed port and an invalid URL fail; `https` to a plain server fails in
  TLS; a UI task awaits a request and sets a signal under
  `HeadlessApp::run_until_idle`.
- The same 9 tests pass on Linux (WSL2, `--features "linux http"`).
- With `DIV_NETWORK_TESTS=1` (local only, not CI), `https://example.com/`
  verifies against the system roots and a self-signed host is rejected as a
  TLS error, on Windows and on WSL2.
- The `fetch` smoke fetches from a server inside the example and exits
  (Windows: software and auto; WSLg: software and OpenGL). CI runs the tests in `rust-parity` and
  `windows-msvc` and the smoke on Windows, X11 and Wayland.

### 10 November 2026: app menu, part 2 (drawn menu bar)

Change: where the platform has no native menu bar (Linux), each window
draws the app menu at its top (`openui/src/menu_bar.rs`). The bar is a
`position: fixed` strip with a spacer before the app's root, so the page
lays out below it and scrolls under it. A click, F10, or Alt with a menu's
mnemonic opens a menu; the arrow keys move between items (skipping
separators and disabled items) and menus, Enter or Space chooses, Escape
closes one level, and an item's mnemonic chooses it. While a menu is open
it takes every key, as native menus do; a full-window layer under the
menus closes them on an outside click. Items show check marks, accelerator
text and submenu arrows, and nested submenus open as flyouts. The bar uses
the `MenuBar`, `Menu`, `MenuItem` and `MenuItemCheckBox` accessibility
roles. A chosen item runs after the event's dispatch, with the window
current, through the same path as native menu commands. Elements of an
earlier drawing are detached during dispatch and destroyed afterwards,
because the engine rejects an event whose target was destroyed by its own
listener (`StaleHandle`).

Remaining gaps, owned by this track unless noted:

- Checked and disabled states reach assistive tools only as an
  accessibility description ("checked", "unavailable"): the engine has no
  toggled or disabled state for authored elements. This needs an engine
  RFC (Workstream A).
- The keyboard highlight does not move accessibility focus, so a screen
  reader does not announce items as the arrows move.
- Mnemonic underlines appear only after F10 or Alt, and Alt alone does not
  open the bar.
- The bar has one light appearance; it does not follow a dark theme.

Verification:

- `cargo test -p divui --features "windows menu" --lib menu_bar` (Windows)
  and `--features "linux menu"` (WSL): a headless document with the drawn
  bar is clicked through real hit testing (a top item opens its menu below
  the bar, an item click chooses and closes, an outside click closes
  without choosing, the closed bar renders the same pixels as before
  opening) and driven by keys (F10, arrows, Escape, Alt+mnemonic, item
  mnemonics, disabled items skipped).
- `menus` smoke passes on WSLg with software and OpenGL presentation.

### 10 November 2026: app menu, part 1

Change: `div::menu` (`menu` feature) declares a typed menu bar:
`Menu`, `MenuItem` (accelerator, reactive `enabled` and `checked`,
`on_select`), `Role` (Quit, Close Window, Undo, Redo, Cut, Copy, Paste,
Select All, About) and `Accelerator`. `AppSpec::menu(build)` gives every
window the menu; an effect in the window's root scope re-resolves it when
the signals it reads change. Accelerators are matched before the focused
document sees the key. Edit roles leave their shortcuts to the document,
which already handles them, so key behaviour is unchanged. On Windows,
`div-platform` shows the menu natively with muda 0.19.3
(`WindowCommand::SetMenu`, `PlatformApplication::menu_selected`). muda adds
four crates: muda, keyboard-types, crossbeam-channel and png, all MIT or
Apache-2.0 and MSRV 1.73 or lower. The runtime now reports F1 to F12 key
codes. The drawn Linux menu bar is the next PR; until then Linux has the
shortcuts and roles without a visible bar.

Verification:

- `cargo test -p divui --features "windows menu" --lib menu` (also on
  Linux): resolution to entries, ids, reactive state, accelerator matching,
  and platform labels and shortcuts.
- Windows: `cargo test -p div-platform --features "windows menu" --test
  menu` reads the native bar through Win32 (labels, accelerator text,
  disabled, checked, separators), posts item commands and sees them reach
  the window's app, then replaces and removes the bar.
- Windows, end to end on the `menus` example: View > Large Text toggles its
  check mark and enables Reset Text Size reactively, File > New Window opens
  a window with the same menu, Close Window closes it and Exit ends the app.
- `menus` smoke passes on Windows (software, auto) and WSL (software,
  OpenGL).

### 9 November 2026: file dialogs

Change: `div::dialog::{open_file, save_file, pick_folder}` build native
dialogs; `pick` and `pick_many` show one on a background thread, owned by
the window that was current when it was built. `div-platform::dialog`
shows the Windows common item dialog (`IFileDialog`) and, on Linux, the XDG
desktop portal's file chooser over D-Bus, falling back to `zenity` when no
portal answers. The runtime now gives controllers each window's native
handle (`WindowController::window_opened`).

Design change from RFC 0002: rfd 0.16.1 would add about 50 crates,
including ICU 2.4 (Rust 1.88, above the 1.85 MSRV). The dialogs use the
`windows` 0.62.2 and zbus 5.13 crates already locked through AccessKit, so
the lockfile gains no packages. The RFC row is updated.

Verification:

- Windows: `cargo test -p div-platform --features "windows dialog" --test
  dialog` shows real dialogs and answers them: save returns the suggested
  non-ASCII name in the given folder, open returns the named file, the
  folder picker returns its folder, and cancel returns nothing.
- Windows, end to end: UI Automation clicks the `dialogs` example's "Save
  as..." button; the dialog is owned by the app window and disables it
  while open, and saving to a typed path writes the note.
- Linux (WSL2): unit tests cover portal options, URI decoding and zenity
  arguments; `dialog_portal` runs a stand-in portal on the session bus and
  checks open, save, folder, cancel, failure responses and the zenity
  fallback. CI runs it under `dbus-run-session`.
- No headless test can show a dialog; `div::testing` will add stand-ins.

### 9 November 2026: clipboard service

Change: `div::clipboard::{read_text, write_text}` use the runtime's
existing Win32, X11 and Wayland text clipboard, which `div-platform` now
hands to controllers as a `Clipboard` (`WindowController::clipboard`).
Without a native runtime they use a clipboard private to the UI thread, so
headless code and tests behave the same. No new dependencies.

Also fixed: with no windows open, an exit request waited for the next
Windows message; the runtime now posts itself a wake when it exits.
Controllers hold only a weak handle, so the runtime drops the clipboard
before the display connection it came from. Before that, every Wayland app
crashed on exit (`SIGSEGV`) when openui released its handle after the
connection closed; the WSL example smokes caught it.

Verification:

- Windows: `cargo test -p div-platform --features windows --test
  clipboard` round-trips non-ASCII text and restores the previous clipboard
  (checked against `Get-Clipboard` before and after); `wake` and
  `multi_window` still pass; the `clipboard` example's smoke reads back what
  it wrote (software and auto) and restores the clipboard.
- `cargo test -p divui --lib clipboard`: the private clipboard round-trips
  and is per thread.
- Linux (WSL2, WSLg Wayland): the compositor refuses clipboard access
  without input ("no events received on any seat"), which the test reports
  rather than fails; `wake` and `multi_window` pass. CI runs the round trip
  and the example smoke on X11 (xvfb) and the test on headless Weston.

### 9 November 2026: async runtime

Change: a new `async` feature of `openui` (enabled by `desktop`; headless
builds gain nothing without it) adds `div::task` with `spawn_local`,
`spawn`, `spawn_blocking`, `Task<T>` (dropping cancels, `detach`) and
`task::time::{sleep, interval}`, plus `Signal::sender()` and
`create_resource(source, fetch)` with `get`, `loading` and `refetch`. It uses
async-task, async-executor, async-io, blocking and futures-lite, all already in
the lockfile (no new crates). UI tasks run on the UI thread in bounded turns
of 8 ms; their wakers post one coalesced wake through a new
`div-platform::LoopWaker` and `WindowController::{started, run_tasks}`.
`Signal` and `Memo` are now `!Send`, so a signal moved into `spawn` is a
compile error (a `compile_fail` doctest checks it). `HeadlessApp` gains
`run_until_idle` (fails with `Error::TasksStillRunning` after 30 s) and
`run_ready_tasks`.

Differences from the RFC: a UI task cancelled with its scope never completes
when awaited (its awaiter normally shares the scope); background tasks are
cancelled by dropping their `Task`, not by scope; the optional `tokio` feature
is left for a later PR.

Verification on Windows (working branch):

- `cargo test -p divui --features async --test tasks`: 5 passed (UI task
  awaiting background work, a timer and blocking work; intervals; scope
  disposal and `Task` drop cancel and drop the futures; senders from another
  thread, including to a disposed signal; resources keep only the latest
  result and refetch).
- `cargo test -p divui --features async --doc task`: 2 passed, including
  the `compile_fail` signal-in-`spawn` check.
- `cargo test -p div-platform --features windows --test wake`: a wake
  from another thread reaches an idle loop (5 wakes sent, coalesced).
- `cargo check --workspace --tests`: passed with `!Send` signals.
- The `tasks` example with `DIV_SMOKE_EXIT=1` (software, auto) exits 0 after
  its background computation and resource fetch finish.
- Linux (WSL2, WSLg Wayland): `wake` and `multi_window` passed with software
  and OpenGL. CI now runs the async tests in `rust-parity` and
  `windows-msvc`, and the `wake` test and `tasks` smoke on X11 and Wayland.

### 9 November 2026: multi-window runtime

Change: `div-platform` runs any number of windows on one winit event loop
(`run_windows` with a `WindowController` that queues `WindowCommand`s); each
window keeps its own presenter, AccessKit adapter, IME and input state.
`openui` adds `App::new(id).name(..).window(WindowSpec, view).run()` and the
`window` module (`open`, `current`, `all`, `WindowKey::unique`,
`WindowHandle::{close, focus, set_title, is_open}`,
`WindowSpec::on_close_requested` returning `Close::Allow`/`Prevent`) and
`app::exit()`. The existing `App::builder()...run(view)` and the C ABI's
`run_document` now adopt their `App` as one window of the same runtime, so
`window::open` works from them too; `platform::run(app, options)` remains as a
single-window controller.

Findings:

- With two animating windows on Windows, the OS delivers paint requests to the
  top window first; re-requesting a frame as soon as a window drew starved the
  other completely (window 2 presented 41,590 frames and window 1 none in
  60 s). Frames are now requested in rounds: each window that wants a frame
  draws once per round; minimized windows are left out and a round ends after
  250 ms, so a window that is never asked to paint cannot stop the others.
- The OpenGL presenter assumed its context stayed current. With one context
  per window it now makes its context current before presenting and before
  deleting its GL objects.
- `window::all()` was added to the RFC's API. Dev-mode window geometry restore
  (RFC section 1) moves to its own small PR.

Verification on Windows (working branch):

- `cargo test -p div-platform --features windows --test multi_window` with
  `DIV_BACKEND` = auto, software and opengl: passed. Two windows open, both
  present, opening an open id focuses it and drops the new app, retitling
  works, closing one keeps the other presenting, and the loop exits after the
  last.
- `cargo test -p divui --features windows --lib window`: 5 passed (key
  dedupe, close, close veto with `window::current()`, exit flag reset).
- `DIV_SMOKE_EXIT=1` smokes of `hello` (software, auto) and the new
  `multi-window` example (software, auto, opengl): exit 0.
- `tools/platform/verify_windows_uia.ps1` against `accessibility.exe`,
  software and auto: passed, including closing the window.
- Linux (WSL2 Ubuntu, WSLg Wayland): the `multi_window` test passed with
  software and OpenGL. X11 could not run there (no `libxkbcommon-x11`); CI's
  hardening job now runs the test and the example smoke on X11 (software,
  Mesa OpenGL) and Wayland (software), and `windows-msvc` smokes the example.

### 9 November 2026: `div-cli` skeleton

Change: a new `crates/div-cli` crate (binary `openui`, no new
dependencies) with `new`, `dev`, `build` and `doctor`, and a `minimal`
template. `div new` writes the Skia build profile from CI into the app's
`.cargo/config.toml`, the workspace `[patch.crates-io]` table (rewritten to the
framework's git source or a local `--framework-path`) and a lockfile seeded from
the workspace's. `div dev` polls the app's sources, rebuilds, and restarts
the app; it stops the old app by closing its stdin (`DIV_DEV`, handled in
`div-platform/src/desktop.rs`). `openui` gains a `desktop` feature that
selects the host platform, and an `DIV_RENDER_PNG` hook that writes the
first frame of `App::run` to a PNG and exits.

Verification on Windows (working branch):

- `cargo test -p div-cli`: 26 passed. Two tests parse the `rust-parity`
  and `windows-msvc` env blocks in `.github/workflows/ci.yml` so the template's
  Skia profile cannot drift from CI.
- `cargo test -p divui render_png_hook_matches_the_headless_first_frame`:
  passed.
- `python tools/framework/check_cli_template.py`: an app made by
  `div new demo --framework-path <repo>` and built by `div build`
  (release) and by plain `cargo build` (dev), with the Skia variables removed
  from the environment, rendered byte-identical PNGs
  (SHA-256 `780C6C3A…261C`). Clean builds took 327 s (release) and 325 s (dev)
  with a cold Skia; an edit-and-rebuild took 3.7 s and the first frame
  appeared in 0.2–0.4 s (dev) and under 0.05 s (release).
- Linux is verified by the new `cli-template` CI job (ubuntu-24.04 and
  windows-2025); its report is uploaded as an artifact.

Follow-up for the engine track: census renders use release Skia (opt-level 3)
while workspace CI tests use dev; the template pins `skia-bindings` to
opt-level 3 in both profiles so apps match census.

### 9 November 2026: RFC 0002

Change: `docs/alpha/rfcs/0002-cli-and-app-layout.md` proposes the CLI
commands, the project template, typed declarations of windows, menus and
services, the async model and the PR order above. Docs only; no code changed.

Findings recorded in the RFC that later PRs depend on:

- A downstream app inherits neither the workspace `[patch.crates-io]` table
  nor the CI Skia build flags, so `div new` must write both.
- `Signal<T>` is `Send` today and panics off the UI thread
  (`openui/src/signal.rs`); the async PR makes it `!Send`.
- `div-platform` checks with `--features linux,windows` on Windows
  (`cargo check -p div-platform --features linux,windows --locked`,
  passed locally), so a single host-selecting `desktop` feature is viable;
  Linux is checked in the CLI PR.
