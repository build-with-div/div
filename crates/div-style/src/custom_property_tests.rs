use super::*;

fn values<'a>(entries: &'a [(&'a str, &'a str)]) -> impl FnMut(&str) -> Option<CustomValue> + 'a {
    move |name| {
        entries
            .iter()
            .find(|(entry, _)| *entry == name)
            .map(|(_, value)| CustomValue::new(value))
    }
}

fn text(value: Option<CustomValue>) -> Option<String> {
    value.map(|value| value.as_str().to_owned())
}

#[test]
fn wide_keywords_ignore_case_and_surrounding_whitespace() {
    assert_eq!(
        CssWideKeyword::from_css(" INHERIT "),
        Some(CssWideKeyword::Inherit)
    );
    assert_eq!(
        CssWideKeyword::from_css("Revert-Layer"),
        Some(CssWideKeyword::RevertLayer)
    );
    assert_eq!(CssWideKeyword::from_css("inherits"), None);
    assert_eq!(CssWideKeyword::from_css("default"), None);
}

#[test]
fn custom_property_names_are_dashed_identifiers() {
    assert_eq!(CustomPropertyName::new("--a").unwrap().as_str(), "--a");
    assert_eq!(CustomPropertyName::new("--Pad").unwrap().as_str(), "--Pad");
    for bad in ["a", "-a", "--a b", " --a", "--a;", "", "--1px"] {
        if bad == "--1px" {
            // `--1px` is an identifier: names may start with a digit
            // after the dashes.
            assert!(CustomPropertyName::new(bad).is_ok());
            continue;
        }
        assert!(CustomPropertyName::new(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn values_keep_interior_text_and_trim_the_ends() {
    let value = UnparsedValue::parse("   1px    2px   ").unwrap();
    assert_eq!(value.as_str(), "1px    2px");
    assert!(!value.has_references());
    assert_eq!(UnparsedValue::parse("/* c */x").unwrap().as_str(), "x");
    assert_eq!(UnparsedValue::parse("").unwrap().as_str(), "");
    assert_eq!(UnparsedValue::parse("  ").unwrap().as_str(), "");
    assert_eq!(UnparsedValue::parse("{a:b}").unwrap().as_str(), "{a:b}");
    assert_eq!(UnparsedValue::parse("a\r\nb").unwrap().as_str(), "a\nb");
}

#[test]
fn values_must_be_declaration_values() {
    for bad in [
        "a; b",
        "a ! b",
        "(a",
        "a)",
        "[a)",
        "\"a\nb",
        "var(a)",
        "var()",
        "var(--a b)",
    ] {
        assert!(UnparsedValue::parse(bad).is_err(), "{bad:?}");
    }
    for good in [
        "(a; b)",
        "var(--a)",
        "var( --a )",
        "var(--a,)",
        "var(--a, 1px, 2px)",
        "calc(var(--n) * 2px)",
        "VAR(--a)",
        "var(--a, var(--b, red))",
    ] {
        let value = UnparsedValue::parse(good).unwrap_or_else(|error| panic!("{good:?}: {error}"));
        assert_eq!(
            value.has_references(),
            good.to_ascii_lowercase().contains("var(")
        );
    }
}

#[test]
fn substitution_replaces_references_and_uses_fallbacks() {
    let mut resolve = values(&[("--a", "1px    2px"), ("--n", "3")]);
    let substitute = |source: &str, resolve: &mut dyn FnMut(&str) -> Option<CustomValue>| {
        text(UnparsedValue::parse(source).unwrap().substitute(resolve))
    };
    assert_eq!(
        substitute("var(--a) var(--a)", &mut resolve).as_deref(),
        Some("1px    2px 1px    2px")
    );
    assert_eq!(
        substitute("calc(var(--n) * 2px)", &mut resolve).as_deref(),
        Some("calc(3 * 2px)")
    );
    assert_eq!(
        substitute("var(--missing, fall  back)", &mut resolve).as_deref(),
        Some("fall  back")
    );
    assert_eq!(
        substitute("var(--missing, var(--also, var(--n)))", &mut resolve).as_deref(),
        Some("3")
    );
    assert_eq!(
        substitute("var(--missing,)", &mut resolve).as_deref(),
        Some("")
    );
    assert_eq!(
        substitute("var(--missing, 1px, 2px)", &mut resolve).as_deref(),
        Some("1px, 2px")
    );
    assert_eq!(substitute("a var(--missing) b", &mut resolve), None);
    assert_eq!(
        substitute("var(--missing, var(--also-missing))", &mut resolve),
        None
    );
}

#[test]
fn a_fallback_is_not_resolved_when_the_reference_is_valid() {
    let mut asked = Vec::new();
    let mut resolve = |name: &str| {
        asked.push(name.to_owned());
        (name == "--a").then(|| CustomValue::new("x"))
    };
    let value = UnparsedValue::parse("var(--a, var(--b))").unwrap();
    assert_eq!(text(value.substitute(&mut resolve)).as_deref(), Some("x"));
    assert_eq!(asked, ["--a"]);
}

#[test]
fn computed_maps_stay_shared_until_they_change() {
    let name = |text: &str| CustomPropertyName::new(text).unwrap();
    let value = |text: &str| Some(CustomValue::new(text));
    let parent = CustomProperties::NONE.with_changes([(name("--a"), value("1"))]);
    assert_eq!(parent.get("--a"), Some("1"));
    let same = parent.with_changes([(name("--a"), value("1"))]);
    assert!(Arc::ptr_eq(
        parent.0.as_ref().unwrap(),
        same.0.as_ref().unwrap()
    ));
    let child = parent.with_changes([(name("--a"), None), (name("--b"), value(""))]);
    assert_eq!(child.get("--a"), None);
    assert_eq!(child.get("--b"), Some(""));
    assert_eq!(
        child.with_changes([(name("--b"), None)]),
        CustomProperties::NONE
    );
}

#[test]
fn substitution_separates_tokens_that_would_merge() {
    let mut resolve = values(&[("--n", "1"), ("--a", "a"), ("--s", "/"), ("--e", "")]);
    let mut substitute = |source: &str| {
        text(
            UnparsedValue::parse(source)
                .unwrap()
                .substitute(&mut resolve),
        )
    };
    assert_eq!(substitute("var(--n)px").as_deref(), Some("1/**/px"));
    assert_eq!(substitute("var(--n)var(--n)").as_deref(), Some("1/**/1"));
    assert_eq!(substitute("var(--a)(b)").as_deref(), Some("a/**/(b)"));
    assert_eq!(substitute("var(--s)*").as_deref(), Some("//**/*"));
    assert_eq!(substitute("var(--n)%").as_deref(), Some("1/**/%"));
    assert_eq!(substitute("var(--n) px").as_deref(), Some("1 px"));
    assert_eq!(substitute("var(--n)/**/px").as_deref(), Some("1/**/px"));
    assert_eq!(substitute("var(--n),px").as_deref(), Some("1,px"));
    // An empty value leaves the previous token in place.
    assert_eq!(
        substitute("var(--n)var(--e)var(--n)").as_deref(),
        Some("1/**/1")
    );
    // The edges carry through a computed value.
    let value = UnparsedValue::parse("x var(--n)")
        .unwrap()
        .substitute(&mut values(&[("--n", "1")]))
        .unwrap();
    assert_eq!(
        (value.first, value.last),
        (TokenEdge::Ident, TokenEdge::Number)
    );
}

#[test]
fn values_split_into_runs_and_references() {
    let value = UnparsedValue::parse("calc(var(--n) * 2px) var(--a, x  y)").unwrap();
    let fallback = UnparsedValue::parse("x  y").unwrap();
    assert_eq!(
        value.segments(),
        [
            UnparsedSegment::Text {
                start: 0,
                end: 5,
                first: TokenEdge::Function,
                last: TokenEdge::Function
            },
            UnparsedSegment::Var {
                name: CustomPropertyName::new("--n").unwrap(),
                fallback: None
            },
            UnparsedSegment::Text {
                start: 13,
                end: 21,
                first: TokenEdge::Other,
                last: TokenEdge::Other
            },
            UnparsedSegment::Var {
                name: CustomPropertyName::new("--a").unwrap(),
                fallback: Some(fallback)
            },
        ]
    );
    assert_eq!(
        UnparsedValue::from_checked(value.as_str(), value.segments().to_vec()),
        value
    );
    assert!(UnparsedValue::parse("a").unwrap().wide_keyword().is_none());
    assert_eq!(
        UnparsedValue::parse(" Inherit ").unwrap().wide_keyword(),
        Some(CssWideKeyword::Inherit)
    );
}

#[test]
fn substituted_values_parse_with_the_property_grammar() {
    use crate::StyleProperty;
    let mut resolve = values(&[("--n", "3"), ("--one", "2px"), ("--c", "rgb(0, 0, 255)")]);
    // `calc()` over lengths in margins and padding waits for calc() PR 2,
    // and the `border` shorthand does not read a comma-separated `rgb()`
    // with or without `var()`; the custom-properties-v1 oracle names both
    // as gaps.
    for (property, source) in [
        (StyleProperty::LineHeight, "calc(var(--n) * 1.5)"),
        (StyleProperty::Border, "var(--one) solid blue"),
        (StyleProperty::Margin, "var(--one) var(--one)"),
    ] {
        let text = UnparsedValue::parse(source)
            .unwrap()
            .substitute(&mut resolve)
            .unwrap();
        let parsed = parse_substituted(property, text.as_str());
        assert!(
            parsed.is_ok(),
            "{source} -> {:?}: {parsed:?}",
            text.as_str()
        );
    }
}

#[test]
fn the_border_gap_does_not_come_from_var() {
    use crate::StyleProperty;
    assert!(crate::parse_literal(StyleProperty::Border, "2px solid rgb(0, 0, 255)").is_err());
    assert!(crate::parse_literal(StyleProperty::BorderTopColor, "rgb(0, 0, 255)").is_ok());
}

#[test]
fn keywords_and_var_match_no_property_outside_the_cascade() {
    // Animations, keyframes and presentational hints check values with
    // `value_matches_property`; only the cascade resolves these.
    use crate::{value_matches_property, StyleProperty, StyleValue};
    assert!(!value_matches_property(
        StyleProperty::Color,
        &StyleValue::Wide(CssWideKeyword::Inherit)
    ));
    assert!(!value_matches_property(
        StyleProperty::MarginLeft,
        &StyleValue::Unparsed(UnparsedValue::parse("var(--x)").unwrap())
    ));
}
