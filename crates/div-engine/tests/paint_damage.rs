//! Paint damage (RFC 0006 A1): the damage between two scenes covers every
//! pixel that differs, and little else.

use div_compositor::{Frame, PhysicalSceneRect, SceneSnapshot, SoftwareCompositor};
use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle};
use div_geometry::{Length, ViewportMetrics};
use div_style::{Color, Display, Overflow, RendererStyleValue};

fn block(engine: &mut Engine, parent: NodeHandle, w: f32, h: f32, color: Color) -> NodeHandle {
    let node = engine.create_native_element(ElementTag::Div).unwrap();
    for value in [
        RendererStyleValue::Display(Display::Block),
        RendererStyleValue::Width(Length::px(w)),
        RendererStyleValue::Height(Length::px(h)),
        RendererStyleValue::BackgroundColor(color),
    ] {
        engine.set_renderer_style(node, value).unwrap();
    }
    engine.append_child(parent, node).unwrap();
    node
}

fn engine() -> Engine {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(320.0, 240.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    engine
        .set_renderer_style(root, RendererStyleValue::Display(Display::Block))
        .unwrap();
    engine
}

fn setup() -> (Engine, NodeHandle, NodeHandle) {
    let mut engine = engine();
    let root = engine.root();
    let a = block(&mut engine, root, 40.0, 20.0, Color::RED);
    let b = block(&mut engine, root, 40.0, 20.0, Color::BLUE);
    engine.scene().unwrap();
    (engine, a, b)
}

fn render(scene: &SceneSnapshot) -> Frame {
    SoftwareCompositor::default().render(scene).unwrap()
}

/// How many pixels differ between two frames, and how many of them lie
/// outside `damage`.
fn compare(before: &Frame, after: &Frame, damage: &[PhysicalSceneRect]) -> (usize, usize) {
    let (mut changed, mut outside) = (0, 0);
    for y in 0..after.height {
        for x in 0..after.width {
            let i = y as usize * before.stride + x as usize * 4;
            let j = y as usize * after.stride + x as usize * 4;
            if before.pixels[i..i + 4] != after.pixels[j..j + 4] {
                changed += 1;
                outside += usize::from(
                    !damage
                        .iter()
                        .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height),
                );
            }
        }
    }
    (changed, outside)
}

#[test]
fn a_background_change_damages_only_its_box() {
    let (mut engine, _, b) = setup();
    let before = engine.scene().unwrap();
    engine
        .set_renderer_style(b, RendererStyleValue::BackgroundColor(Color::BLACK))
        .unwrap();
    let rects = engine.scene().unwrap().physical_damage_since(&before);
    assert!(!rects.is_empty());
    // The box is 40 by 20 at (0, 20). Its chunk is outset by one pixel in
    // the scrolling contents' recording and once more where that recording
    // is drawn into the document's.
    for r in rects {
        assert!(
            r.y >= 18 && r.x + r.width <= 42 && r.y + r.height <= 42,
            "{r:?}"
        );
    }
}

#[test]
fn a_change_without_visual_effect_damages_nothing() {
    let (mut engine, a, _) = setup();
    let before = engine.scene().unwrap();
    engine
        .set_renderer_style(a, RendererStyleValue::Color(Color::BLUE))
        .unwrap();
    let after = engine.scene().unwrap();
    assert_eq!(after.physical_damage_since(&before), Vec::new());
}

/// A layer with one draw folds into it, which rounds differently from a
/// real layer, so a layer gaining a second draw changes the first draw's
/// pixels although that draw did not change.
#[test]
fn an_opacity_group_gaining_a_draw_damages_the_draw_it_held() {
    let mut engine = engine();
    let root = engine.root();
    for value in [
        RendererStyleValue::OverflowX(Overflow::Hidden),
        RendererStyleValue::OverflowY(Overflow::Hidden),
    ] {
        engine.set_renderer_style(root, value).unwrap();
    }
    let color = Color::from_rgba8(12, 140, 90, 255);
    let outer = block(&mut engine, root, 200.0, 120.0, color);
    engine
        .set_renderer_style(outer, RendererStyleValue::Opacity(0.5))
        .unwrap();
    let inner = block(
        &mut engine,
        outer,
        80.0,
        96.0,
        Color::from_rgba8(12, 140, 91, 255),
    );
    engine
        .set_renderer_style(inner, RendererStyleValue::Opacity(0.5))
        .unwrap();
    let before = engine.scene().unwrap();
    block(&mut engine, inner, 11.0, 9.0, Color::BLUE);
    let after = engine.scene().unwrap();
    let damage = after.physical_damage_since(&before);
    let (changed, outside) = compare(&render(&before), &render(&after), &damage);
    assert!(changed > 11 * 9, "the held draw must change: {changed}");
    assert_eq!(outside, 0);
}

/// The engine builds scenes for geometry reads too, so a presenter can skip
/// some; its damage must span all of them.
#[test]
fn damage_spans_scenes_the_presenter_skipped() {
    let mut engine = engine();
    let root = engine.root();
    let mut scrollers = Vec::new();
    for _ in 0..2 {
        let scroller = block(&mut engine, root, 60.0, 40.0, Color::WHITE);
        engine
            .set_renderer_style(scroller, RendererStyleValue::OverflowY(Overflow::Scroll))
            .unwrap();
        block(&mut engine, scroller, 40.0, 10.0, Color::RED);
        block(&mut engine, scroller, 40.0, 200.0, Color::BLUE);
        scrollers.push(scroller);
    }
    let before = engine.scene().unwrap();
    engine.scroll_to(scrollers[0], 0.0, 5.0).unwrap();
    let skipped = engine.scene().unwrap();
    engine.scroll_to(scrollers[1], 0.0, 5.0).unwrap();
    let after = engine.scene().unwrap();
    assert_ne!(skipped.generation(), after.generation());
    let damage = after.physical_damage_since(&before);
    let (changed, outside) = compare(&render(&before), &render(&after), &damage);
    assert!(changed > 0);
    assert_eq!(outside, 0);
}
