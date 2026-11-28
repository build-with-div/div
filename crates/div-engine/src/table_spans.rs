//! Reflection of the table span content attributes into layout state.
//!
//! The span limits and fallbacks follow the table model in the WHATWG HTML
//! Standard, section 4.9 "Tabular data" (CC BY 4.0). How attribute text is
//! read as an integer was measured as a black box in pinned Chromium
//! 147.0.7727.50. The corpus and results are in
//! `tests/fidelity/fixtures/table-spans-v1`, and
//! `tools/qualification/capture_table_span_oracle.py` regenerates them.

use crate::{Engine, EngineError, NodeHandle};
use div_dom::{ElementTag, NodeId};

const DEFAULT_SPAN: u32 = 1;
const MAX_COLUMN_SPAN: u32 = 1000;
const MAX_ROW_SPAN: u32 = 65534;
/// The largest value the IDL setters write back unchanged. Anything above it
/// stores the default span.
const MAX_STORED_SPAN: u32 = i32::MAX as u32;

/// Reads the integer at the start of `text`.
///
/// Leading ASCII whitespace and one sign are skipped, and anything after the
/// digits is ignored. Magnitudes too large for `u32` saturate, so they still
/// land on the upper clamp. A negative value is an error unless it is zero.
fn leading_integer(text: &str) -> Option<u32> {
    let unsigned = text.trim_start_matches([' ', '\t', '\n', '\u{c}', '\r']);
    let (negative, body) = match unsigned.as_bytes().first() {
        Some(b'-') => (true, &unsigned[1..]),
        Some(b'+') => (false, &unsigned[1..]),
        _ => (false, unsigned),
    };
    let digits = body.bytes().take_while(u8::is_ascii_digit);
    let mut seen = false;
    let mut magnitude = 0_u32;
    for digit in digits {
        seen = true;
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(u32::from(digit - b'0'));
    }
    (seen && (!negative || magnitude == 0)).then_some(magnitude)
}

fn column_span_from(value: Option<&str>) -> u32 {
    match value.and_then(leading_integer) {
        None | Some(0) => DEFAULT_SPAN,
        Some(span) => span.min(MAX_COLUMN_SPAN),
    }
}

fn row_span_from(value: Option<&str>) -> u32 {
    value
        .and_then(leading_integer)
        .map_or(DEFAULT_SPAN, |span| span.min(MAX_ROW_SPAN))
}

fn stored_span_text(value: u32) -> String {
    if value > MAX_STORED_SPAN {
        DEFAULT_SPAN.to_string()
    } else {
        value.to_string()
    }
}

fn is_cell(tag: ElementTag) -> bool {
    matches!(tag, ElementTag::TableCell | ElementTag::TableHeaderCell)
}

/// The content attribute holding this element's column span, if it has one.
fn column_attribute(tag: ElementTag) -> Option<&'static str> {
    match tag {
        ElementTag::TableCell | ElementTag::TableHeaderCell => Some("colspan"),
        ElementTag::TableColumn | ElementTag::TableColumnGroup => Some("span"),
        _ => None,
    }
}

impl Engine {
    /// Updates layout state after a span attribute changed. `name` must be
    /// lowercase and `value` is `None` when the attribute was removed. Returns
    /// whether the element's span changed.
    pub(crate) fn sync_table_span_attribute(
        &mut self,
        node: NodeId,
        name: &str,
        value: Option<&str>,
    ) -> bool {
        let tag = self.document.node(node).tag;
        if column_attribute(tag) == Some(name) {
            let span = column_span_from(value);
            let state = self.document.node_mut(node);
            let changed = state.table_col_span != span;
            state.table_col_span = span;
            changed
        } else if is_cell(tag) && name == "rowspan" {
            let span = row_span_from(value);
            let state = self.document.node_mut(node);
            let changed = state.table_row_span != span;
            state.table_row_span = span;
            changed
        } else {
            false
        }
    }

    /// The reflected `colSpan` of a cell or `span` of a column or column group.
    pub fn column_span(&self, handle: NodeHandle) -> Result<u32, EngineError> {
        let node = self.resolve(handle)?;
        let state = self.document.node(node);
        column_attribute(state.tag)
            .map(|_| state.table_col_span)
            .ok_or(EngineError::InvalidInput(
                "column span needs a table cell, column or column group",
            ))
    }

    /// Writes the column span attribute through the shared attribute path.
    pub fn set_column_span(&mut self, handle: NodeHandle, value: u32) -> Result<(), EngineError> {
        let node = self.resolve(handle)?;
        let name = column_attribute(self.document.node(node).tag).ok_or(
            EngineError::InvalidInput("column span needs a table cell, column or column group"),
        )?;
        self.set_attribute(handle, name, stored_span_text(value))
    }

    /// The reflected `rowSpan` of a cell. Zero means the rest of the row group.
    pub fn row_span(&self, handle: NodeHandle) -> Result<u32, EngineError> {
        let node = self.resolve(handle)?;
        let state = self.document.node(node);
        if is_cell(state.tag) {
            Ok(state.table_row_span)
        } else {
            Err(EngineError::InvalidInput("row span needs a table cell"))
        }
    }

    /// Writes the `rowspan` attribute through the shared attribute path.
    pub fn set_row_span(&mut self, handle: NodeHandle, value: u32) -> Result<(), EngineError> {
        let node = self.resolve(handle)?;
        if !is_cell(self.document.node(node).tag) {
            return Err(EngineError::InvalidInput("row span needs a table cell"));
        }
        self.set_attribute(handle, "rowspan", stored_span_text(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_skip_ascii_whitespace_and_one_sign() {
        assert_eq!(leading_integer(" \t\n\u{c}\r+7tail"), Some(7));
        assert_eq!(leading_integer("-0tail"), Some(0));
        for invalid in [
            "", "x", "-", "+", "++2", "-+2", "- 2", "-2", "\u{b}2", "\u{a0}2",
        ] {
            assert_eq!(leading_integer(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn oversized_integers_saturate_and_negative_ones_stay_errors() {
        assert_eq!(leading_integer("4294967295"), Some(u32::MAX));
        assert_eq!(leading_integer("99999999999999999999"), Some(u32::MAX));
        assert_eq!(leading_integer("-99999999999999999999"), None);
    }

    #[test]
    fn spans_apply_their_fallbacks_and_limits() {
        for (text, column, row) in [
            (None, 1, 1),
            (Some("junk"), 1, 1),
            (Some("0"), 1, 0),
            (Some("1001"), 1000, 1001),
            (Some("65535"), 1000, 65534),
            (Some("4294967296"), 1000, 65534),
        ] {
            assert_eq!(column_span_from(text), column, "{text:?}");
            assert_eq!(row_span_from(text), row, "{text:?}");
        }
    }

    #[test]
    fn setters_store_the_default_above_the_signed_range() {
        assert_eq!(stored_span_text(0), "0");
        assert_eq!(stored_span_text(MAX_STORED_SPAN), "2147483647");
        assert_eq!(stored_span_text(MAX_STORED_SPAN + 1), "1");
        assert_eq!(stored_span_text(u32::MAX), "1");
    }
}
