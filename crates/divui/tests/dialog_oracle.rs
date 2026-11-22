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
//! Div has no `dialog` element yet, so this test cannot replay a scenario.
//! It checks the capture and keeps the list of scenarios still to replay:
//! each later RFC 0015 PR replays its scenarios here and takes them off the
//! list, which only shrinks.

use serde_json::Value;
use std::collections::BTreeSet;

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json");

/// Every RFC 0015 scenario needs the `divui` dialog API (Migration, PR 5).
const API: &str = "RFC 0015 PR 5: the divui dialog API, events, Escape and focus";

/// Scenarios not replayed yet, with what each waits for.
const PENDING: &[(&str, &str)] = &[
    ("modal_open_close", API),
    ("modal_tab", API),
    ("escape_closes", API),
    ("escape_cancel_prevented", API),
    ("escape_cancel_prevented_activation", API),
    ("escape_keydown_prevented", API),
    ("escape_in_field", API),
    ("escape_non_modal", API),
    ("click_inert", API),
    ("click_dialog_content", API),
    ("script_focus_inert", API),
    ("autofocus", API),
    ("no_focusable", API),
    ("nested", API),
    ("non_modal_focus", API),
    ("non_modal_focus_outside", API),
    ("non_modal_click_outside", API),
    ("remove_open_modal", API),
    ("remove_open_attribute", API),
    ("return_value", API),
    ("request_close_prevented", API),
    ("invalid_state", API),
    ("beforetoggle_prevented", API),
];

/// The geometry records wait for the top layer and the UA rules.
const GEOMETRY_PENDING: &str =
    "RFC 0015 PRs 3 and 4: the engine top layer, `::backdrop` and the UA rules";

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
    let pending: BTreeSet<&str> = PENDING.iter().map(|(name, _)| *name).collect();
    assert_eq!(pending.len(), PENDING.len(), "a scenario is listed twice");
    let unknown: Vec<_> = pending.difference(&captured).collect();
    assert!(unknown.is_empty(), "pending but not captured: {unknown:?}");
    // No scenario is replayed yet: every captured one is pending.
    let unlisted: Vec<_> = captured.difference(&pending).collect();
    assert!(
        unlisted.is_empty(),
        "captured but neither replayed nor pending: {unlisted:?}"
    );
    assert!(!GEOMETRY_PENDING.is_empty());
}
