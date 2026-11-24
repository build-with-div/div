//! Prints this CPU's RCPPS fingerprint for `capture_canvas_traces.py
//! --cpu-fingerprint`, or exits with status 1 where it cannot be computed.
//!
//! ```text
//! rustc --edition 2021 -O tools/canvas/rcpps_fingerprint.rs -o out/rcpps_fingerprint
//! ```

#[path = "../../crates/div-canvas/tests/support/rcpps.rs"]
mod rcpps;

fn main() {
    match rcpps::fingerprint() {
        Some(value) => println!("{value:016x}"),
        None => {
            eprintln!("no RCPPS fingerprint: this CPU has no AVX");
            std::process::exit(1);
        }
    }
}
