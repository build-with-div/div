//! `<dialog>` and the top layer against pinned Chromium 147 (RFC 0015,
//! platform-plan item H4).
//!
//! `tools/qualification/capture_dialog_oracle.py` recorded what
//! Chrome-for-Testing 147.0.7727.50 does with real XTEST key and button
//! input on X11 under WSL (`tests/fidelity/fixtures/dialog-v1/
//! chromium-linux-x11.json`), at device scale 1:
//!
//! - after every step of each scenario, the `keydown`, `keyup`,
//!   `mousedown`, `mouseup`, `click`, `beforetoggle`, `toggle`, `cancel` and
//!   `close` events, with targets, `cancelable`, the toggle states and the
//!   closing `returnValue`, any exception a script call threw, the focused
//!   element, and each dialog's `open`, `:modal` and `returnValue`;
//! - each dialog's box and computed style, and its `::backdrop`'s computed
//!   style, at three viewports.
//!
//! `crates/div-engine/tests/dialog_oracle.rs` replays the scenarios that
//! need only the engine (RFC 0015 PR 3a): their input events, focus and
//! dialog states match exactly. Their toggle-family events are a pending
//! field owned by PR 5, and that test fails once they match. It also
//! replays all 15 geometry records field by field, with the fields the UA
//! `max-width`/`max-height` calc decides pending on engine 0.2 PR 2
//! (`calc()` in `width`, `height`, `min-*` and `max-*`, with `em` and `%`).
//! This test checks the capture and keeps the list of scenarios not
//! replayed at all: each later PR replays its scenarios and takes them off
//! the list, which only shrinks.

use serde_json::Value;
use std::collections::BTreeSet;

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json");

/// Replayed by the engine with every input event, focus and dialog state;
/// their `beforetoggle`, `toggle`, `cancel` and `close` events are a pending
/// field owned by [`API`].
const ENGINE_REPLAYED: &[&str] = &[
    "modal_open_close",
    "modal_tab",
    "click_dialog_content",
    "script_focus_inert",
    "autofocus",
    "no_focusable",
    "non_modal_focus",
    "non_modal_focus_outside",
    "non_modal_click_outside",
    "remove_open_modal",
];

/// Escape, the dialog events and script exceptions.
const API: &str = "RFC 0015 PR 5: the divui dialog API, events, Escape and focus";

/// A press on inert content reaches the modal dialog through its backdrop.
const BACKDROP: &str = "RFC 0015 PR 4: `::backdrop` takes presses outside the modal dialog";

/// Removing `open` from a modal dialog hides its focused button, and Blink
/// then clears focus (`active` is `body`); Div has no focus fixup for an
/// element that stops being rendered and keeps it.
const FOCUS_FIXUP: &str = "engine: focus fixup when the focused element stops being rendered";

/// Scenarios not replayed yet, with what each waits for.
const PENDING: &[(&str, &str)] = &[
    ("escape_closes", API),
    ("escape_cancel_prevented", API),
    ("escape_cancel_prevented_activation", API),
    ("escape_keydown_prevented", API),
    ("escape_in_field", API),
    ("escape_non_modal", API),
    ("click_inert", BACKDROP),
    // The press on the lower dialog reaches the upper one's backdrop, and
    // Escape closes each in turn.
    ("nested", "RFC 0015 PRs 4 and 5: `::backdrop`, then Escape"),
    ("remove_open_attribute", FOCUS_FIXUP),
    ("return_value", API),
    ("request_close_prevented", API),
    ("invalid_state", API),
    ("beforetoggle_prevented", API),
];

/// The `::backdrop` half of each geometry record (its computed style).
const GEOMETRY_PENDING: &str = "RFC 0015 PR 4: `::backdrop`";

fn oracle() -> Value {
    serde_json::from_str(LINUX).expect("the dialog oracle is JSON")
}

#[test]
fn the_capture_is_pinned_chromium_with_real_input() {
    let oracle = oracle();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    assert_eq!(oracle["platform"], "linux-x11");
    assert_eq!(oracle["viewport"], serde_json::json!([480, 400]));
    let scenarios = oracle["scenarios"].as_object().expect("scenarios");
    for (name, steps) in scenarios {
        let steps = steps.as_array().expect("steps");
        assert!(!steps.is_empty(), "{name} has no steps");
        for step in steps {
            for field in ["input", "events", "later", "active", "dialogs"] {
                assert!(step.get(field).is_some(), "{name}: a step has no {field}");
            }
        }
    }
    let geometry = oracle["geometry"].as_array().expect("geometry");
    // Three viewports, five dialog states each.
    assert_eq!(geometry.len(), 15);
}

#[test]
fn every_scenario_is_replayed_or_pending() {
    let oracle = oracle();
    let captured: BTreeSet<&str> = oracle["scenarios"]
        .as_object()
        .expect("scenarios")
        .keys()
        .map(String::as_str)
        .collect();
    let mut listed: BTreeSet<&str> = PENDING.iter().map(|(name, _)| *name).collect();
    listed.extend(ENGINE_REPLAYED.iter().copied());
    assert_eq!(
        listed.len(),
        PENDING.len() + ENGINE_REPLAYED.len(),
        "a scenario is listed twice"
    );
    let unknown: Vec<_> = listed.difference(&captured).collect();
    assert!(unknown.is_empty(), "listed but not captured: {unknown:?}");
    let unlisted: Vec<_> = captured.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "captured but neither replayed nor pending: {unlisted:?}"
    );
    assert!(!GEOMETRY_PENDING.is_empty());
}
