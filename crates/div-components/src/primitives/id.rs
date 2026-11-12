//! Unique ids for the `aria-controls` and `aria-labelledby` relations, as
//! Radix's `useId`.
//!
//! The values are never compared with Chromium's; the trace harness
//! compares only which element each relation points to.

use std::cell::Cell;

thread_local! {
    static NEXT: Cell<u64> = const { Cell::new(1) };
}

/// A new id, `div-<prefix>-<n>`, unique on this thread.
pub fn unique_id(prefix: &str) -> String {
    let n = NEXT.with(|next| next.replace(next.get() + 1));
    format!("div-{prefix}-{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let first = unique_id("tabs");
        let second = unique_id("tabs");
        assert_ne!(first, second);
        assert!(first.starts_with("div-tabs-"));
    }
}
