//! Radix-like behaviors for the components: state, keyboard, focus and the
//! DOM contract (`role`, `aria-*`, `data-*`, `tabindex`), without visual
//! style. See RFC 0003 (`docs/alpha/rfcs/0003-component-primitives.md`).
//!
//! Each primitive ports the pinned Radix source, which is its
//! specification, and is checked against Radix running in Chromium by the
//! trace harness (`reference_harness.py trace`). They use only the public
//! `divui` API; the interims for what it lacks are listed in the RFC.
//!
//! Radix UI is MIT licensed, Copyright (c) 2022 WorkOS; see
//! `crates/div-components/THIRD_PARTY.md`.

pub mod collection;
pub mod controllable;
pub mod id;
pub mod keys;
pub mod roving_focus;
pub mod tabs;

pub use controllable::{Controllable, OnChange};
pub use keys::{Held, Key};
pub use roving_focus::{Direction, ItemOptions, RovingFocusGroup, RovingFocusOptions};
pub use tabs::{ActivationMode, TabsOptions, TabsState};
