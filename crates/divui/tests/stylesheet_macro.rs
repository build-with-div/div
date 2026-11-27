//! `stylesheet!` compiles CSS style rules to a typed `StyleSheet` and a
//! module of typed class names (RFC 0001), which a document adopts.

use div::{stylesheet, Document, Element};

stylesheet!(cards, "tests/stylesheet/cards.css");

stylesheet!(pub(crate) inline {
    // Rust comments are not CSS; the macro drops them.
    .box { opacity: 0.6; color: red }
    .box:hover, #main .box { opacity: 0.7 }
    .outer > .box { color: blue !important }
});

fn element(document: &Document, parent: &Element, classes: &[&str]) -> Element {
    let element = Element::create(document, "div").unwrap();
    for class in classes {
        element.add_class(class).unwrap();
    }
    parent.append_child(&element).unwrap();
    element
}

fn opacity(element: &Element) -> f32 {
    element.computed_style().unwrap().opacity
}

#[test]
fn a_file_sheet_keeps_its_layers_rules_and_importance() {
    assert_eq!(cards::SHEET.layers(), ["base", "components"]);
    // The nested rule follows its parent.
    let layers: Vec<_> = cards::SHEET
        .rules()
        .iter()
        .map(|rule| rule.layer())
        .collect();
    assert_eq!(
        layers,
        [
            Some("base"),
            Some("components"),
            Some("components"),
            None,
            None
        ]
    );
    let last = cards::SHEET.rules().last().unwrap();
    assert!(last.declarations().declarations().is_empty());
    assert_eq!(last.important().declarations().len(), 1);
}

#[test]
fn class_constants_name_the_sheet_classes() {
    use cards::classes;
    assert_eq!(classes::card.as_str(), "card");
    assert_eq!(classes::active.as_str(), "active");
    assert_eq!(classes::card_title.as_str(), "card-title");
    // A keyword becomes a raw identifier.
    assert_eq!(classes::r#type.as_str(), "type");
    assert_eq!(&*inline::classes::outer, "outer");
}

#[test]
fn an_adopted_sheet_styles_the_document_in_cascade_order() {
    let document = Document::new(100, 100).unwrap();
    let body = document.body();
    let card = element(&document, &body, &[]);
    card.add_class(&cards::classes::card).unwrap();
    assert_eq!(opacity(&card), 1.0);

    let sheet = document.adopt_stylesheet(&cards::SHEET).unwrap();
    // The later layer wins.
    assert_eq!(opacity(&card), 0.2);
    card.add_class(&cards::classes::active).unwrap();
    assert_eq!(opacity(&card), 0.3);

    let title = element(&document, &body, &["card", "card-title"]);
    // Unlayered rules outrank layered ones.
    assert_eq!(opacity(&title), 0.4);
    let typed = element(&document, &body, &["card-title", "type"]);
    assert_eq!(opacity(&typed), 0.5);

    document.remove_stylesheet(sheet).unwrap();
    assert_eq!(opacity(&card), 1.0);
}

#[test]
fn an_inline_sheet_matches_selector_lists_and_combinators() {
    let document = Document::new(100, 100).unwrap();
    let body = document.body();
    let outer = element(&document, &body, &["outer"]);
    let main = element(&document, &body, &[]);
    main.set_attribute("id", "main").unwrap();
    let plain = element(&document, &body, &["box"]);
    let in_main = element(&document, &main, &["box"]);
    let in_outer = element(&document, &outer, &["box"]);
    document.adopt_stylesheet(&inline::SHEET).unwrap();

    assert_eq!(opacity(&plain), 0.6);
    assert_eq!(opacity(&in_main), 0.7);
    let red = plain.computed_style().unwrap().color;
    let blue = in_outer.computed_style().unwrap().color;
    assert_eq!((red.r, red.g, red.b), (1.0, 0.0, 0.0));
    assert_eq!((blue.r, blue.g, blue.b), (0.0, 0.0, 1.0));
}

stylesheet!(responsive {
    .panel { opacity: 0.1 }
    @media screen and (width >= 600px) {
        .panel { opacity: 0.2 }
        @media (min-resolution: 2dppx) { .panel { opacity: 0.3 } }
    }
    .panel {
        @media (prefers-color-scheme: dark) { opacity: 0.4 }
    }
    @media print { .panel { opacity: 0.9 } }
});

#[test]
fn media_rules_apply_while_their_queries_match() {
    use div::{ColorScheme, ViewportMetrics};
    let rules = responsive::SHEET.rules();
    assert_eq!(rules.len(), 5);
    assert!(rules[0].media().is_empty());
    assert_eq!(rules[2].media().len(), 2);

    let document = Document::new(400, 300).unwrap();
    let body = document.body();
    let panel = element(&document, &body, &["panel"]);
    document.adopt_stylesheet(&responsive::SHEET).unwrap();
    assert_eq!(opacity(&panel), 0.1);

    let resize = |width: f64, scale: f64| {
        let metrics = ViewportMetrics::from_logical_size(width, 300.0, scale).unwrap();
        document.set_viewport(metrics).unwrap();
        document.update_all().unwrap();
    };
    resize(600.0, 1.0);
    assert_eq!(opacity(&panel), 0.2);
    resize(600.0, 2.0);
    assert_eq!(opacity(&panel), 0.3);
    resize(599.0, 2.0);
    assert_eq!(opacity(&panel), 0.1);

    document.set_color_scheme(ColorScheme::Dark).unwrap();
    assert_eq!(document.color_scheme().unwrap(), ColorScheme::Dark);
    document.update_all().unwrap();
    assert_eq!(opacity(&panel), 0.4);
}

// `(color)` would be a compile error: Div does not evaluate the `color`
// feature yet. `(hover)` takes the same parenthesized-condition path.
stylesheet!(grouped {
    .panel { opacity: 0.1 }
    @media screen and (not (hover)) { .panel { opacity: 0.5 } }
    @media ((width >= 600px)) { .panel { opacity: 0.6 } }
});

#[test]
fn media_lists_keep_their_parentheses() {
    use div::ViewportMetrics;
    let rules = grouped::SHEET.rules();
    assert_eq!(rules[1].media()[0].to_string(), "screen and (not (hover))");
    assert_eq!(rules[2].media()[0].to_string(), "((width >= 600px))");

    let document = Document::new(400, 300).unwrap();
    let body = document.body();
    let panel = element(&document, &body, &["panel"]);
    document.adopt_stylesheet(&grouped::SHEET).unwrap();
    // A desktop mouse hovers, so the first rule never applies.
    assert_eq!(opacity(&panel), 0.1);
    let metrics = ViewportMetrics::from_logical_size(600.0, 300.0, 1.0).unwrap();
    document.set_viewport(metrics).unwrap();
    document.update_all().unwrap();
    assert_eq!(opacity(&panel), 0.6);
}
