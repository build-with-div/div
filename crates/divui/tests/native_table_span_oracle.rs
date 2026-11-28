//! Table span attributes against pinned Chromium 147.
//!
//! The value corpus and Chromium's results live in
//! `tests/fidelity/fixtures/table-spans-v1`; regenerate them with
//! `tools/qualification/capture_table_span_oracle.py`. These tests build the
//! same elements and tables through the public API and require exact matches.
use div::prelude::*;
use serde_json::Value;

#[cfg(target_os = "windows")]
const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/table-spans-v1/chromium-windows.json");
#[cfg(not(target_os = "windows"))]
const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/table-spans-v1/chromium-linux.json");

fn oracle() -> Value {
    serde_json::from_str(ORACLE).unwrap()
}

fn viewport(oracle: &Value) -> Document {
    let size = oracle["viewport"].as_array().unwrap();
    Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(
            size[0].as_f64().unwrap(),
            size[1].as_f64().unwrap(),
            size[2].as_f64().unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn append(document: &Document, parent: &Element, tag: &str) -> Element {
    let element = Element::create(document, tag).unwrap();
    parent.append_child(&element).unwrap();
    element
}

fn put(element: &Element, name: &str, value: &Value) {
    if let Some(text) = value.as_str() {
        element.set_attribute(name, text).unwrap();
    }
}

fn new_table(document: &Document) -> Element {
    let table = append(document, &document.body(), "table");
    table.set_border_collapse(BorderCollapse::Separate).unwrap();
    table
        .set_border_spacing((Length::px(0.0), Length::px(0.0)))
        .unwrap();
    table
}

fn cell(document: &Document, row: &Element) -> Element {
    let cell = append(document, row, "td");
    cell.set_padding(Edges::all(LengthValue::px(0.0))).unwrap();
    cell.set_border(Border::NONE).unwrap();
    cell
}

fn sized_column(document: &Document, parent: &Element, tag: &str, width: f32) -> Element {
    let column = append(document, parent, tag);
    column.set_width(LengthValue::px(width)).unwrap();
    column
}

fn assert_rect(label: &str, element: &Element, table: &Element, expected: &Value) {
    let origin = table.bounding_rect().unwrap().unwrap();
    let rect = element.bounding_rect().unwrap().unwrap();
    let actual = [
        f64::from(rect.x - origin.x),
        f64::from(rect.y - origin.y),
        f64::from(rect.width),
        f64::from(rect.height),
    ];
    let expected: Vec<f64> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_f64().unwrap())
        .collect();
    assert_eq!(actual.as_slice(), expected.as_slice(), "{label}");
}

#[test]
fn span_attribute_reflection_matches_chromium() {
    let oracle = oracle();
    let document = viewport(&oracle);
    for entry in oracle["reflection"].as_array().unwrap() {
        let value = &entry["value"];
        for tag in ["td", "th"] {
            let colspan = Element::create(&document, tag).unwrap();
            put(&colspan, "colspan", value);
            assert_eq!(
                colspan.column_span().unwrap(),
                number(&entry[tag]["colSpan"]),
                "{tag} colspan {value}"
            );
            let rowspan = Element::create(&document, tag).unwrap();
            put(&rowspan, "rowspan", value);
            assert_eq!(
                rowspan.row_span().unwrap(),
                number(&entry[tag]["rowSpan"]),
                "{tag} rowspan {value}"
            );
        }
        for tag in ["col", "colgroup"] {
            let element = Element::create(&document, tag).unwrap();
            put(&element, "span", value);
            assert_eq!(
                element.column_span().unwrap(),
                number(&entry[tag]["span"]),
                "{tag} span {value}"
            );
        }
    }
}

#[test]
fn span_setters_match_chromium() {
    let oracle = oracle();
    let document = viewport(&oracle);
    for entry in oracle["setters"].as_array().unwrap() {
        let value = number(&entry["value"]);
        for tag in ["td", "th"] {
            let expected = &entry[tag];
            let colspan = Element::create(&document, tag).unwrap();
            colspan.set_table_column_span(value).unwrap();
            assert_eq!(
                colspan.column_span().unwrap(),
                number(&expected["colSpan"]),
                "{tag} colSpan = {value}"
            );
            assert_eq!(
                colspan.get_attribute("colspan").unwrap().as_deref(),
                expected["colspanAttribute"].as_str(),
                "{tag} colSpan = {value}"
            );
            let rowspan = Element::create(&document, tag).unwrap();
            rowspan.set_row_span(value).unwrap();
            assert_eq!(
                rowspan.row_span().unwrap(),
                number(&expected["rowSpan"]),
                "{tag} rowSpan = {value}"
            );
            assert_eq!(
                rowspan.get_attribute("rowspan").unwrap().as_deref(),
                expected["rowspanAttribute"].as_str(),
                "{tag} rowSpan = {value}"
            );
        }
        for tag in ["col", "colgroup"] {
            let expected = &entry[tag];
            let element = Element::create(&document, tag).unwrap();
            element.set_table_column_span(value).unwrap();
            assert_eq!(
                element.column_span().unwrap(),
                number(&expected["span"]),
                "{tag} span = {value}"
            );
            assert_eq!(
                element.get_attribute("span").unwrap().as_deref(),
                expected["spanAttribute"].as_str(),
                "{tag} span = {value}"
            );
        }
    }
}

fn check_colspan(document: &Document, value: &Value, expected: &Value) {
    let table = new_table(document);
    let body = append(document, &table, "tbody");
    let first = append(document, &body, "tr");
    let target = cell(document, &first);
    target.set_height(LengthValue::px(10.0)).unwrap();
    put(&target, "colspan", value);
    let marker = cell(document, &first);
    marker.set_height(LengthValue::px(10.0)).unwrap();
    let grid = append(document, &body, "tr");
    for _ in 0..number(&expected["columns"]) {
        let column = cell(document, &grid);
        column.set_width(LengthValue::px(1.0)).unwrap();
        column.set_height(LengthValue::px(1.0)).unwrap();
    }
    let label = format!("colspan {value}");
    assert_rect(&label, &table, &table, &with_origin(&expected["table"]));
    assert_rect(&label, &target, &table, &expected["target"]);
    assert_rect(&label, &marker, &table, &expected["marker"]);
    table.remove().unwrap();
}

fn check_rowspan(document: &Document, value: &Value, rows: u32, expected: &Value) {
    let table = new_table(document);
    let body = append(document, &table, "tbody");
    let mut target = None;
    for index in 0..rows {
        let row = append(document, &body, "tr");
        row.set_height(LengthValue::px(10.0)).unwrap();
        if index == 0 {
            let spanning = cell(document, &row);
            spanning.set_width(LengthValue::px(10.0)).unwrap();
            put(&spanning, "rowspan", value);
            target = Some(spanning);
        }
        let filler = cell(document, &row);
        filler.set_width(LengthValue::px(10.0)).unwrap();
        filler.set_height(LengthValue::px(10.0)).unwrap();
    }
    let next = append(document, &table, "tbody");
    let row = append(document, &next, "tr");
    row.set_height(LengthValue::px(10.0)).unwrap();
    let after = cell(document, &row);
    after.set_width(LengthValue::px(10.0)).unwrap();
    after.set_height(LengthValue::px(10.0)).unwrap();
    let label = format!("rowspan {value}");
    assert_rect(&label, &table, &table, &with_origin(&expected["table"]));
    assert_rect(&label, &target.unwrap(), &table, &expected["target"]);
    assert_rect(&label, &after, &table, &expected["after"]);
    table.remove().unwrap();
}

fn check_columns(document: &Document, kind: &str, value: &Value, expected: &Value) {
    let table = new_table(document);
    match kind {
        "col" => {
            let group = append(document, &table, "colgroup");
            put(&sized_column(document, &group, "col", 1.0), "span", value);
            sized_column(document, &group, "col", 7.0);
        }
        "colgroup" => {
            put(
                &sized_column(document, &table, "colgroup", 1.0),
                "span",
                value,
            );
            sized_column(document, &table, "colgroup", 7.0);
        }
        _ => {
            let group = sized_column(document, &table, "colgroup", 3.0);
            put(&group, "span", value);
            sized_column(document, &group, "col", 5.0);
            let next = append(document, &table, "colgroup");
            sized_column(document, &next, "col", 7.0);
        }
    }
    let body = append(document, &table, "tbody");
    let row = append(document, &body, "tr");
    let columns = number(&expected["columns"]) as usize;
    let cells: Vec<Element> = (0..columns)
        .map(|_| {
            let item = cell(document, &row);
            item.set_height(LengthValue::px(4.0)).unwrap();
            item
        })
        .collect();
    let label = format!("{kind} span {value}");
    let probe = number(&expected["probe"]) as usize;
    assert_rect(&label, &table, &table, &with_origin(&expected["table"]));
    assert_rect(&label, &cells[0], &table, &expected["first"]);
    assert_rect(&label, &cells[probe], &table, &expected["at_probe"]);
    assert_rect(&label, &cells[columns - 1], &table, &expected["last"]);
    table.remove().unwrap();
}

/// A table measured against itself has a zero offset; keep only its size.
fn with_origin(rect: &Value) -> Value {
    let rect = rect.as_array().unwrap();
    serde_json::json!([0.0, 0.0, rect[2], rect[3]])
}

#[test]
fn span_layout_matches_chromium() {
    let oracle = oracle();
    let document = viewport(&oracle);
    let rows = number(&oracle["section_rows"]);
    for entry in oracle["layout"].as_array().unwrap() {
        let value = &entry["value"];
        check_colspan(&document, value, &entry["colspan"]);
        check_rowspan(&document, value, rows, &entry["rowspan"]);
        check_columns(&document, "col", value, &entry["col"]);
        check_columns(&document, "colgroup", value, &entry["colgroup"]);
        check_columns(
            &document,
            "colgroup_with_col",
            value,
            &entry["colgroup_with_col"],
        );
    }
}
