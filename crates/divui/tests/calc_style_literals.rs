//! `calc()` literals in the style macros: `view!`'s `style:`, `style!` and
//! `stylesheet!` parse them at compile time and emit constructors (RFC 0001).
//! Each must compile and give the same computed `line-height` as the runtime
//! parse of the same text. Tailwind writes `text-sm`'s line height as
//! `calc(1.25 / 0.875)` and `text-xs`'s as `calc(1 / 0.75)`.

use div::prelude::*;
use div::typed_style::{parse_literal, Declaration, Style, StyleProperty};
use div::{style, stylesheet, Document};

stylesheet!(calc_sheet {
    .text-sm { line-height: calc(1.25 / 0.875) }
    .text-xs { line-height: calc(1 / 0.75) }
});

const TEXT_SM: &str = "calc(1.25 / 0.875)";
const TEXT_XS: &str = "calc(1 / 0.75)";

fn runtime_line_height(document: &Document, literal: &str) -> String {
    let element = Element::create(document, "div").unwrap();
    document.body().append_child(&element).unwrap();
    let value = parse_literal(StyleProperty::LineHeight, literal).unwrap();
    element
        .set_property(StyleProperty::LineHeight, value)
        .unwrap();
    format!("{:?}", element.computed_style().unwrap().line_height)
}

fn line_height(element: &Element) -> String {
    format!("{:?}", element.computed_style().unwrap().line_height)
}

#[test]
fn style_macro_calc_matches_the_runtime_parse() {
    let built = style! { line-height: calc(1.25 / 0.875); };
    let expected: Style = [Declaration {
        property: StyleProperty::LineHeight,
        value: parse_literal(StyleProperty::LineHeight, TEXT_SM).unwrap(),
    }]
    .into_iter()
    .collect();
    assert_eq!(format!("{built:?}"), format!("{expected:?}"));
}

#[test]
fn view_style_calc_matches_the_runtime_parse() {
    let mut app =
        HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap();
    app.mount(|| {
        view! {
            <div>
                <p style:line-height="calc(1.25 / 0.875)">"sm"</p>
                <p style:line-height="calc(1 / 0.75)">"xs"</p>
            </div>
        }
    })
    .unwrap();
    let document = app.document();
    let root = document.body().first_child().unwrap().unwrap();
    let sm = root.first_child().unwrap().unwrap();
    let xs = sm.next_sibling().unwrap().unwrap();
    assert_eq!(line_height(&sm), runtime_line_height(&document, TEXT_SM));
    assert_eq!(line_height(&xs), runtime_line_height(&document, TEXT_XS));
}

#[test]
fn stylesheet_calc_matches_the_runtime_parse() {
    let document = Document::new(100, 100).unwrap();
    document.adopt_stylesheet(&calc_sheet::SHEET).unwrap();
    for (class, literal) in [("text-sm", TEXT_SM), ("text-xs", TEXT_XS)] {
        let element = Element::create(&document, "div").unwrap();
        element.add_class(class).unwrap();
        document.body().append_child(&element).unwrap();
        assert_eq!(
            line_height(&element),
            runtime_line_height(&document, literal),
            "{class}"
        );
    }
}
