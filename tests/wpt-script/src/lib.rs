//! WPT `testharness.js` tests translated into Div tests (RFC 0005).
//!
//! `tools/wpt_script/codegen.py` writes the tests under `tests/` from the
//! pinned WPT sources and the ledgers in `ledgers/`; they run on
//! [`runtime`]. Do not edit the generated files: change the generator and
//! regenerate them (`codegen.py`, checked by `codegen.py --check`).

pub mod runtime;
