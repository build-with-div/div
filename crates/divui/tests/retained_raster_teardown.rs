use div::prelude::*;

/// RFC 0006 A2 keeps one compositor, with the last frame's raster, in each
/// document. A document that has rendered partial frames is still freed when
/// its last handle drops.
#[test]
fn a_document_with_a_retained_raster_is_freed_on_drop() {
    let document = Document::with_font_collection(
        ViewportMetrics::from_logical_size(800.0, 600.0, 1.0).unwrap(),
        FontCollection::deterministic_test(),
    )
    .unwrap();
    let root = document.body();
    root.set_background_color(Color::WHITE).unwrap();
    let card = Element::create(&document, "div").unwrap();
    card.set_width(LengthValue::px(40.0)).unwrap();
    card.set_height(LengthValue::px(40.0)).unwrap();
    card.set_background_color(Color::RED).unwrap();
    root.append_child(&card).unwrap();
    let pixel = |frame: &Bitmap| {
        let offset = 20 * frame.stride() + 20 * 4;
        frame.pixels()[offset..offset + 4].to_vec()
    };
    assert_eq!(
        pixel(&document.render_to_bitmap().unwrap()),
        [255, 0, 0, 255]
    );
    card.set_background_color(Color::BLUE).unwrap();
    assert_eq!(
        pixel(&document.render_to_bitmap().unwrap()),
        [0, 0, 255, 255]
    );
    card.set_background_color(Color::RED).unwrap();
    assert_eq!(
        pixel(&document.render_to_bitmap().unwrap()),
        [255, 0, 0, 255]
    );
    let weak = card.downgrade();
    drop(card);
    drop(root);
    drop(document);
    assert!(weak.upgrade().is_none());
}
