//! Style presets (RFC 0004): the browser preset's user-agent sheet ranks
//! below every author style, and `NONE` leaves a document as it was.

use div::prelude::*;
use div::{stylesheet, StylePreset, UserAgentStyle};

stylesheet!(author {
    [hidden] { display: flex }
});

fn document() -> Document {
    Document::with_font_collection(
        ViewportMetrics::from_logical_size(320.0, 240.0, 1.0).unwrap(),
        FontCollection::deterministic_test(),
    )
    .unwrap()
}

fn child(document: &Document, tag: &str) -> Element {
    let element = Element::create(document, tag).unwrap();
    document.body().append_child(&element).unwrap();
    element
}

fn display(element: &Element) -> Display {
    element.computed_style().unwrap().display
}

#[test]
fn none_is_the_default_and_adds_no_rules() {
    let document = document();
    assert_eq!(document.style_preset(), StylePreset::NONE);
    assert_eq!(StylePreset::default(), StylePreset::NONE);
    assert_eq!(StylePreset::BROWSER.user_agent, UserAgentStyle::Browser);
    let hidden = child(&document, "p");
    hidden.set_attribute("hidden", "").unwrap();
    assert_eq!(display(&hidden), Display::Block);
    // Setting the current preset again is a no-op.
    document.set_style_preset(StylePreset::NONE).unwrap();
    assert_eq!(display(&hidden), Display::Block);
}

#[test]
fn the_browser_preset_hides_what_the_rendering_section_hides() {
    let document = document();
    let hidden = child(&document, "p");
    hidden.set_attribute("hidden", "").unwrap();
    let until_found = child(&document, "p");
    until_found.set_attribute("hidden", "UNTIL-FOUND").unwrap();
    let hidden_input = child(&document, "input");
    hidden_input.set_attribute("type", "Hidden").unwrap();
    let embed = child(&document, "embed");
    embed.set_attribute("hidden", "").unwrap();
    let shown = child(&document, "p");

    document.set_style_preset(StylePreset::BROWSER).unwrap();
    assert_eq!(document.style_preset(), StylePreset::BROWSER);
    assert_eq!(display(&hidden), Display::None);
    assert_eq!(display(&hidden_input), Display::None);
    assert_eq!(display(&shown), Display::Block);
    // `hidden=until-found` keeps its box.
    assert_eq!(display(&until_found), Display::Block);
    // A hidden `embed` stays an empty inline box.
    assert_eq!(display(&embed), Display::Inline);

    // Back to `NONE`, the rules no longer apply.
    document.set_style_preset(StylePreset::NONE).unwrap();
    assert_eq!(display(&hidden), Display::Block);
    assert_ne!(display(&hidden_input), Display::None);
}

#[test]
fn author_styles_outrank_the_preset() {
    let document = document();
    document.set_style_preset(StylePreset::BROWSER).unwrap();
    let ruled = child(&document, "p");
    ruled.set_attribute("hidden", "").unwrap();
    let attributed = child(&document, "p");
    attributed.set_attribute("hidden", "").unwrap();
    attributed.set_attribute("style", "display: grid").unwrap();
    let own = child(&document, "p");
    own.set_attribute("hidden", "").unwrap();
    own.set_display(Display::InlineBlock).unwrap();
    assert_eq!(display(&ruled), Display::None);
    assert_eq!(display(&attributed), Display::Grid);
    assert_eq!(display(&own), Display::InlineBlock);

    // An author rule of any specificity beats a user-agent rule.
    document.adopt_stylesheet(&author::SHEET).unwrap();
    assert_eq!(display(&ruled), Display::Flex);

    // A user-agent `!important` declaration beats every author one.
    let input = child(&document, "input");
    input.set_attribute("type", "hidden").unwrap();
    input
        .set_attribute("style", "display: block !important")
        .unwrap();
    assert_eq!(display(&input), Display::None);
}
