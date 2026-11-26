use super::*;

fn parse(text: &str) -> ParsedMediaQueryList {
    parse_media_query_list(text)
}

fn screen() -> MediaValues {
    MediaValues::screen(800.0, 600.0, 1.0)
}

fn matches(text: &str, values: &MediaValues) -> bool {
    parse(text).list.matches(values)
}

#[track_caller]
fn assert_clean(text: &str) -> MediaQueryList {
    let parsed = parse(text);
    assert!(parsed.problems.is_empty(), "{text}: {:?}", parsed.problems);
    parsed.list
}

#[test]
fn empty_list_matches_and_media_types_follow_the_screen() {
    let values = screen();
    assert!(assert_clean("").queries.is_empty());
    assert!(matches("", &values));
    assert!(matches("all", &values));
    assert!(matches("screen", &values));
    assert!(matches("SCREEN", &values));
    assert!(!matches("print", &values));
    assert!(matches("not print", &values));
    assert!(!matches("not screen", &values));
    assert!(matches("only screen", &values));
    assert!(matches("print, screen", &values));
    let tv = parse("tv");
    assert_eq!(tv.problems.len(), 1);
    assert!(!tv.list.matches(&values));
    assert!(matches("not tv", &values));
}

#[test]
fn width_and_height_tests_allow_one_layout_unit() {
    let values = MediaValues::screen(800.0, 600.0, 1.0);
    assert!(matches("(min-width: 800px)", &values));
    assert!(matches("(min-width: 800.01px)", &values));
    assert!(!matches("(min-width: 800.02px)", &values));
    assert!(matches("(max-width: 799.99px)", &values));
    assert!(!matches("(max-width: 799.98px)", &values));
    assert!(matches("(width: 800px)", &values));
    assert!(matches("(width >= 800px)", &values));
    assert!(!matches("(width > 800px)", &values));
    assert!(matches("(width < 800.01px)", &values));
    assert!(matches("(400px < width <= 800px)", &values));
    assert!(!matches("(400px < width < 800px)", &values));
    assert!(matches("(800px >= width > 400px)", &values));
    assert!(matches("(height: 600px)", &values));
    assert!(matches("(min-width: 50em)", &values));
    assert!(!matches("(min-width: 50.1em)", &values));
    assert!(matches("(width: 100vw)", &values));
    assert!(matches("(width)", &values));
    assert!(!matches("(width)", &MediaValues::screen(0.0, 600.0, 1.0)));
    assert!(matches("(min-width: 0)", &values));
    assert!(matches("(width > 0)", &values));
    // Negative values: only `>` and `>=` hold.
    assert!(matches("(min-width: -1px)", &values));
    assert!(!matches("(max-width: -1px)", &values));
    assert!(!matches("(width: -1px)", &values));
}

#[test]
fn invalid_queries_become_not_all() {
    let values = screen();
    for text in [
        "screen and",
        "screen (width)",
        "(width) and (height) or (orientation)",
        "and (width)",
        "not not (width)",
    ] {
        let parsed = parse(text);
        assert_eq!(parsed.list.queries, vec![MediaQuery::not_all()], "{text}");
        assert_eq!(parsed.problems.len(), 1, "{text}");
        assert!(!parsed.list.matches(&values), "{text}");
    }
    // An invalid query only voids itself.
    assert!(matches("screen and, screen", &values));
}

#[test]
fn unknown_terms_never_match_even_negated() {
    let values = screen();
    for text in [
        "(unknown-feature)",
        "(min-width: 100)",
        "(width: 100px 200px)",
        "(min-width)",
        "(400px < width > 300px)",
        "(100px < width < 200px 300px)",
        "(100px < width = 200px)",
        "(min--webkit-device-pixel-ratio: 1)",
        "(color)",
        "not (color)",
        "foo(bar)",
        "(width: 100px) and (color)",
        "(min-width: calc(100px))",
        "(min-width: 10ch)",
    ] {
        let parsed = parse(text);
        assert_eq!(parsed.problems.len(), 1, "{text}: {:?}", parsed.problems);
        assert!(!parsed.list.matches(&values), "{text}");
    }
    // Unknown or true is true.
    let parsed = parse("(color) or (width)");
    assert_eq!(parsed.problems.len(), 1);
    assert!(parsed.list.matches(&values));
    assert!(parse("(color)").problems[0].contains("not supported yet"));
    assert!(parse("(min-width: calc(1px))").problems[0].contains("calc()"));
}

#[test]
fn conditions_nest_and_combine() {
    let values = screen();
    assert!(matches(
        "screen and (min-width: 100px) and (max-width: 900px)",
        &values
    ));
    assert!(matches("(max-width: 100px) or (min-width: 700px)", &values));
    assert!(matches("not (max-width: 100px)", &values));
    assert!(matches(
        "((min-width: 100px) and (orientation: landscape))",
        &values
    ));
    assert!(matches(
        "not ((max-width: 100px) or (orientation: portrait))",
        &values
    ));
    assert!(!matches("not screen and (min-width: 100px)", &values));
    assert!(matches("not screen and (max-width: 100px)", &values));
    assert!(matches("only screen and (orientation:landscape)", &values));
}

#[test]
fn orientation_aspect_ratio_and_resolution() {
    let wide = MediaValues::screen(800.0, 600.0, 2.0);
    assert!(matches("(orientation: landscape)", &wide));
    assert!(matches(
        "(orientation: portrait)",
        &MediaValues::screen(600.0, 600.0, 1.0)
    ));
    assert!(matches("(aspect-ratio: 4/3)", &wide));
    assert!(matches("(aspect-ratio: 4 / 3)", &wide));
    assert!(matches("(min-aspect-ratio: 1)", &wide));
    assert!(!matches("(max-aspect-ratio: 1)", &wide));
    assert!(matches("(aspect-ratio > 1/1)", &wide));
    assert!(!matches("(aspect-ratio: 0/0)", &wide));
    assert!(matches("(aspect-ratio)", &wide));
    // The viewport is truncated to whole pixels.
    assert!(matches(
        "(aspect-ratio: 4/3)",
        &MediaValues::screen(800.9, 600.5, 1.0)
    ));
    assert!(matches("(resolution: 2dppx)", &wide));
    assert!(matches("(resolution: 2x)", &wide));
    assert!(matches("(resolution: 192dpi)", &wide));
    assert!(matches("(min-resolution: 1.5dppx)", &wide));
    assert!(!matches("(max-resolution: 1.5dppx)", &wide));
    assert!(matches("(-webkit-device-pixel-ratio: 2)", &wide));
    assert!(matches("(-webkit-min-device-pixel-ratio: 1.5)", &wide));
    assert!(!matches("(-webkit-max-device-pixel-ratio: 1.5)", &wide));
    assert!(matches("(resolution: 75.59dpcm)", &wide));
    assert!(matches("(resolution)", &wide));
    let zoomed = MediaValues::screen(800.0, 600.0, 1.25);
    assert!(matches("(resolution: 1.25dppx)", &zoomed));
    assert!(matches("(resolution: 120dpi)", &zoomed));
    assert!(!parse("(resolution: -1dppx)").problems.is_empty());
}

#[test]
fn preference_and_interaction_features() {
    let mut values = screen();
    assert!(matches("(prefers-color-scheme: light)", &values));
    assert!(!matches("(prefers-color-scheme: dark)", &values));
    assert!(matches("(prefers-color-scheme)", &values));
    values.color_scheme = ColorScheme::Dark;
    assert!(matches("(prefers-color-scheme: dark)", &values));
    assert!(matches("(prefers-color-scheme)", &values));
    assert!(matches("(prefers-reduced-motion: no-preference)", &values));
    assert!(!matches("(prefers-reduced-motion)", &values));
    values.reduced_motion = true;
    assert!(matches("(prefers-reduced-motion: reduce)", &values));
    assert!(matches("(prefers-reduced-motion)", &values));
    assert!(matches("(hover: hover)", &values));
    assert!(matches("(hover)", &values));
    assert!(matches("(any-hover: hover)", &values));
    assert!(!matches("(any-hover: none)", &values));
    assert!(matches("(pointer: fine)", &values));
    assert!(!matches("(pointer: coarse)", &values));
    assert!(matches("(any-pointer: fine)", &values));
    assert!(matches("(forced-colors: none)", &values));
    assert!(!matches("(forced-colors: active)", &values));
    assert!(!matches("(forced-colors)", &values));
    // A keyword another feature takes is invalid.
    assert!(!parse("(hover: fine)").problems.is_empty());
    assert!(!parse("(prefers-color-scheme: no-preference)")
        .problems
        .is_empty());
}

#[test]
fn whitespace_and_case() {
    let values = screen();
    assert!(assert_clean("  SCREEN  AND  ( MIN-WIDTH :100PX )  ").matches(&values));
    assert!(assert_clean("(width>=100px)").matches(&values));
    assert!(assert_clean("(100px<=width)").matches(&values));
    // `and(` is a function token, not `and` followed by a block.
    assert!(!parse("screen and(min-width: 100px)").problems.is_empty());
    // `< =` is `<` followed by garbage.
    assert!(!parse("(width < = 100px)").problems.is_empty());
}

#[test]
fn parsed_structure() {
    let list = assert_clean("only screen and (300px <= width < 900px)");
    assert_eq!(
        list.queries,
        vec![MediaQuery {
            restrictor: MediaRestrictor::Only,
            media_type: MediaType::Screen,
            condition: Some(MediaCondition::Feature(MediaFeature {
                name: MediaFeatureName::Width,
                prefix: None,
                test: FeatureTest::Range {
                    left: Some((
                        FeatureValue::Length {
                            value: 300.0,
                            unit: MediaLengthUnit::Px,
                        },
                        Comparison::Le,
                    )),
                    right: Some((
                        Comparison::Lt,
                        FeatureValue::Length {
                            value: 900.0,
                            unit: MediaLengthUnit::Px,
                        },
                    )),
                },
            })),
        }]
    );
}
