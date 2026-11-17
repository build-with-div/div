use div::prelude::*;
use std::{cell::Cell, rc::Rc};

#[path = "support/real_zoom.rs"]
mod real_zoom;
use real_zoom::{Gap, RealZoomGaps};

fn document(scale: f64) -> Document {
    Document::with_font_collection_and_options(
        ViewportMetrics::from_logical_size(800.0, 600.0, scale).unwrap(),
        FontCollection::deterministic_test(),
        EngineOptions {
            raster_configuration: RasterConfiguration::deterministic_aliased(false),
        },
    )
    .unwrap()
}

// The expected widths are pinned Chromium 147's at scale 1, measured by
// `tools/qualification/probe_windows_text_oracle.py` on Linux and Windows
// (build-with-div/div-fidelity-data `archive/docs/progress/evidence/`
// `windows-text-oracle-v2.json` and `linux-text-oracle-v2.json`).
// Div keeps them at every scale. Above scale 1, real-zoom Chromium lays the
// line heights out in device pixels and differs where
// `LINE_HEIGHT_REAL_ZOOM_GAPS` lists it: (scale, case, before or after,
// Chromium's width, the same for the node and its descendant). Line-height
// feeds a retained 2.5lh width; the native callback changes only the parent
// font, exercising both metric contexts.
#[cfg(windows)]
const LINE_HEIGHT_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "before", 152.0),
    (1.25, 1, "before", 119.9749984741211),
    (1.25, 2, "before", 116.9375),
    (1.25, 3, "before", 114.0374984741211),
    (1.25, 5, "before", 96.55000305175781),
    (1.25, 0, "after", 152.0),
    (1.25, 1, "after", 119.9749984741211),
    (1.25, 2, "after", 127.25),
    (1.25, 3, "after", 114.0374984741211),
    (1.25, 5, "after", 96.55000305175781),
    (1.5, 0, "before", 153.33334350585938),
    (1.5, 1, "before", 119.94792175292969),
    (1.5, 2, "before", 116.97917175292969),
    (1.5, 5, "before", 96.54167175292969),
    (1.5, 0, "after", 153.33334350585938),
    (1.5, 1, "after", 119.94792175292969),
    (1.5, 2, "after", 127.23958587646484),
    (1.5, 5, "after", 96.54167175292969),
    (2.0, 0, "before", 152.5),
    (2.0, 1, "before", 122.53125),
    (2.0, 2, "before", 116.9921875),
    (2.0, 0, "after", 152.5),
    (2.0, 1, "after", 122.53125),
    (2.0, 2, "after", 127.265625),
    (3.0, 0, "before", 151.6666717529297),
    (3.0, 1, "before", 121.64583587646484),
    (3.0, 2, "before", 116.97917175292969),
    (3.0, 3, "before", 114.03646087646484),
    (3.0, 0, "after", 151.6666717529297),
    (3.0, 1, "after", 121.64583587646484),
    (3.0, 2, "after", 127.29167175292969),
    (3.0, 3, "after", 114.03646087646484),
];
#[cfg(not(windows))]
const LINE_HEIGHT_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "before", 152.0),
    (1.25, 2, "before", 116.9375),
    (1.25, 3, "before", 114.0374984741211),
    (1.25, 5, "before", 96.55000305175781),
    (1.25, 0, "after", 152.0),
    (1.25, 2, "after", 127.25),
    (1.25, 3, "after", 114.0374984741211),
    (1.25, 5, "after", 96.55000305175781),
    (1.5, 0, "before", 153.33334350585938),
    (1.5, 2, "before", 116.97917175292969),
    (1.5, 5, "before", 96.54167175292969),
    (1.5, 0, "after", 153.33334350585938),
    (1.5, 2, "after", 127.23958587646484),
    (1.5, 5, "after", 96.54167175292969),
    (2.0, 0, "before", 152.5),
    (2.0, 1, "before", 121.6328125),
    (2.0, 2, "before", 116.9921875),
    (2.0, 0, "after", 152.5),
    (2.0, 1, "after", 121.6328125),
    (2.0, 2, "after", 127.265625),
    (3.0, 0, "before", 151.6666717529297),
    (3.0, 1, "before", 121.63542175292969),
    (3.0, 2, "before", 116.97917175292969),
    (3.0, 3, "before", 114.03646087646484),
    (3.0, 0, "after", 151.6666717529297),
    (3.0, 1, "after", 121.63542175292969),
    (3.0, 2, "after", 127.29167175292969),
    (3.0, 3, "after", 114.03646087646484),
];

#[test]
fn relative_line_height_updates_from_native_callbacks_and_inherits_fixed_values() {
    let mut gaps = RealZoomGaps::new(LINE_HEIGHT_REAL_ZOOM_GAPS);
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let document = document(scale);
        document
            .body()
            .set_font_size(LengthValue::px(16.0))
            .unwrap();
        let parent = Element::create(&document, "div").unwrap();
        document.body().append_child(&parent).unwrap();
        parent
            .set_font_family(FontFamilyList::single("Ahem"))
            .unwrap();
        parent.set_font_size(LengthValue::px(18.72)).unwrap();
        parent.set_line_height(LineHeight::Number(1.25)).unwrap();
        let mut nodes = Vec::new();
        for (case, (unit, before, after)) in [
            (LengthValue::Ch(2.0), 150.0, 150.0),
            (
                LengthValue::Ex(2.0),
                if cfg!(windows) { 119.9375 } else { 121.625 },
                if cfg!(windows) { 119.9375 } else { 121.625 },
            ),
            (LengthValue::Lh(2.0), 116.953125, 127.1875),
            (LengthValue::Em(1.5), 114.03125, 114.03125),
            (LengthValue::Rem(1.5), 60.0, 60.0),
            (LengthValue::percent(127.5), 96.546875, 96.546875),
        ]
        .into_iter()
        .enumerate()
        {
            let node = Element::create(&document, "div").unwrap();
            parent.append_child(&node).unwrap();
            node.set_width(LengthValue::Lh(2.5)).unwrap();
            node.set_line_height_length(unit).unwrap();
            node.set_font_size(LengthValue::px(30.41)).unwrap();
            let descendant = Element::create(&document, "div").unwrap();
            node.append_child(&descendant).unwrap();
            descendant.set_font_size(LengthValue::px(11.0)).unwrap();
            descendant.set_width(LengthValue::Lh(2.5)).unwrap();
            assert_eq!(
                node.bounding_rect().unwrap().unwrap().width,
                before,
                "{unit:?}/{scale}"
            );
            assert_eq!(descendant.bounding_rect().unwrap().unwrap().width, before);
            gaps.check(scale, case, "before", before);
            assert_eq!(
                node.computed_style().unwrap().line_height,
                descendant.computed_style().unwrap().line_height
            );
            nodes.push((node, descendant, before, after));
        }
        let owned = nodes[2].0.computed_style().unwrap();
        let weak_parent = parent.downgrade();
        let called = Rc::new(Cell::new(0));
        let count = called.clone();
        nodes[0]
            .0
            .on("click", move |_| {
                weak_parent
                    .upgrade()
                    .unwrap()
                    .set_font_size(LengthValue::px(20.37))
                    .unwrap();
                count.set(count.get() + 1);
            })
            .unwrap();
        nodes[0].0.click().unwrap();
        assert_eq!(called.get(), 1);
        for (case, (node, child, _, after)) in nodes.iter().enumerate() {
            assert_eq!(node.bounding_rect().unwrap().unwrap().width, *after);
            assert_eq!(child.bounding_rect().unwrap().unwrap().width, *after);
            gaps.check(scale, case, "after", *after);
        }
        assert_ne!(
            owned.line_height,
            nodes[2].0.computed_style().unwrap().line_height
        );
        assert_eq!(
            document.render_to_png_buffer().unwrap(),
            document.render_to_png_buffer().unwrap()
        );
        let weak = nodes[0].0.downgrade();
        drop(nodes);
        drop(parent);
        drop(document);
        assert!(weak.upgrade().is_none());
    }
    gaps.finish();
}

// Measured at scale 1 only. The expected widths are pinned Chromium 147's on
// Linux and Windows (build-with-div/div-fidelity-data
// `archive/docs/progress/evidence/` `windows-text-oracle-v2.json` and
// `linux-text-oracle-v2.json`).
#[test]
fn relative_line_height_follows_shorthand_order_pseudos_and_animation_cancel() {
    let document = document(1.0);
    let node = Element::create(&document, "div").unwrap();
    document.body().append_child(&node).unwrap();
    node.set_font_family(FontFamilyList::single("Ahem"))
        .unwrap();
    node.set_font_size(LengthValue::px(20.0)).unwrap();
    node.set_width(LengthValue::Lh(2.0)).unwrap();
    node.set_line_height_length(LengthValue::Ch(2.0)).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, 80.0);
    let mut shorthand = FontShorthand::new(24.0, FontFamilyList::single("Ahem")).unwrap();
    shorthand.line_height = LineHeight::Number(1.5);
    node.set_font(shorthand).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, 72.0);
    node.set_line_height_length(LengthValue::Ex(2.5)).unwrap();
    let ex_width = if cfg!(windows) { 95.03125 } else { 96.0 };
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, ex_width);
    let pseudo = Style::default().line_height_length(LengthValue::Ch(1.5));
    node.set_pseudo_style(PseudoStyleTarget::FirstLine, &pseudo)
        .unwrap();
    assert_eq!(
        node.computed_style()
            .unwrap()
            .first_line_style
            .as_ref()
            .unwrap()
            .line_height,
        LineHeight::Length(36.0)
    );
    let animation = node
        .animate(
            StyleProperty::LineHeight,
            Keyframes::from_values(LengthValue::Ex(2.5), LengthValue::Ex(5.0)),
            AnimationOptions {
                duration_ms: 1000.0,
                fill: FillMode::Both,
                ..AnimationOptions::default()
            },
        )
        .unwrap();
    document.seek_animation(animation, 500.0).unwrap();
    let (midpoint, enlarged, cancelled) = if cfg!(windows) {
        (142.546875, 195.109375, 130.078125)
    } else {
        (144.0, 192.0, 128.0)
    };
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, midpoint);
    node.set_font_size(LengthValue::px(32.0)).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, enlarged);
    document.cancel_animation(animation).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, cancelled);
    let copy = node.clone_subtree().unwrap();
    document.body().append_child(&copy).unwrap();
    assert_eq!(copy.bounding_rect().unwrap().unwrap().width, cancelled);
}

#[test]
fn invalid_relative_line_heights_leave_retained_style_intact() {
    let document = document(1.0);
    let node = document.body();
    node.set_line_height_length(LengthValue::Em(1.5)).unwrap();
    node.set_font_size(LengthValue::px(0.0)).unwrap();
    let before = node.computed_style().unwrap();
    for value in [
        LengthValue::percent(-0.5),
        LengthValue::ViewportWidth(-1.0),
        LengthValue::Ch(-1.0),
        LengthValue::Ex(f32::NAN),
        LengthValue::auto(),
        LengthValue::none(),
    ] {
        assert!(node.set_line_height_length(value).is_err());
        assert_eq!(
            node.computed_style().unwrap().line_height,
            before.line_height
        );
    }
}
