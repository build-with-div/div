//! Partial raster (RFC 0006 A2): a compositor that keeps its last frame
//! rasters only the tiles a change damages, and its frame equals a full
//! raster byte for byte.

use div_compositor::{Frame, SceneSnapshot, SoftwareCompositor};
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

struct Page {
    engine: Engine,
    boxes: Vec<NodeHandle>,
    scroller: NodeHandle,
}

/// A page larger than one untiled layer, with a nested scroller and
/// translucent boxes. With `root_scrolls`, the root's scrollbar is a
/// viewport control drawn over the tiles.
fn page(width: f64, height: f64, scale: f64, root_scrolls: bool) -> Page {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(width, height, scale).unwrap()).unwrap();
    let root = engine.root();
    engine
        .set_renderer_style(root, RendererStyleValue::Display(Display::Block))
        .unwrap();
    if root_scrolls {
        engine
            .set_renderer_style(root, RendererStyleValue::OverflowY(Overflow::Scroll))
            .unwrap();
    }
    let mut boxes = Vec::new();
    for i in 0..12u8 {
        let color = Color::from_rgba8(20 * i, 140, 255 - 20 * i, 255);
        let node = block(&mut engine, root, 90.0 + 37.0 * f32::from(i), 61.0, color);
        if i % 3 == 1 {
            engine
                .set_renderer_style(node, RendererStyleValue::Opacity(0.6))
                .unwrap();
        }
        if i % 4 == 2 {
            engine
                .set_renderer_style(node, RendererStyleValue::MarginLeft(Length::px(13.5)))
                .unwrap();
        }
        boxes.push(node);
    }
    let scroller = block(&mut engine, root, 300.0, 120.0, Color::WHITE);
    engine
        .set_renderer_style(scroller, RendererStyleValue::OverflowY(Overflow::Scroll))
        .unwrap();
    block(&mut engine, scroller, 250.0, 40.0, Color::RED);
    block(&mut engine, scroller, 250.0, 400.0, Color::BLUE);
    engine.scene().unwrap();
    Page {
        engine,
        boxes,
        scroller,
    }
}

fn set(page: &mut Page, index: usize, value: RendererStyleValue) {
    let node = page.boxes[index];
    page.engine.set_renderer_style(node, value).unwrap();
}

type Change = (&'static str, fn(&mut Page));

const CHANGES: &[Change] = &[
    ("background", |p| {
        set(p, 3, RendererStyleValue::BackgroundColor(Color::BLACK))
    }),
    ("opacity", |p| set(p, 4, RendererStyleValue::Opacity(0.25))),
    ("margin", |p| {
        set(p, 6, RendererStyleValue::MarginLeft(Length::px(41.25)))
    }),
    ("append", |p| {
        let parent = p.boxes[9];
        block(
            &mut p.engine,
            parent,
            11.0,
            9.0,
            Color::from_rgba8(30, 60, 200, 255),
        );
    }),
    ("nested scroll", |p| {
        let node = p.scroller;
        p.engine.scroll_to(node, 0.0, 37.0).unwrap();
    }),
    ("root scroll", |p| {
        let root = p.engine.root();
        // A root that does not scroll ignores this.
        let _ = p.engine.scroll_to(root, 0.0, 29.0);
    }),
    ("display none", |p| {
        set(p, 1, RendererStyleValue::Display(Display::None))
    }),
    ("width", |p| {
        set(p, 11, RendererStyleValue::Width(Length::px(17.0)))
    }),
    ("remove", |p| {
        let node = p.boxes[5];
        p.engine.remove(node).unwrap();
    }),
];

fn full(scene: &SceneSnapshot) -> Frame {
    SoftwareCompositor::default().render(scene).unwrap()
}

fn assert_same(partial: &Frame, full: &Frame, step: &str) {
    assert_eq!(
        (partial.width, partial.height, partial.stride),
        (full.width, full.height, full.stride),
        "{step}"
    );
    if partial.pixels != full.pixels {
        let first = partial
            .pixels
            .iter()
            .zip(&full.pixels)
            .position(|(a, b)| a != b)
            .unwrap();
        let (y, x) = (first / full.stride, first % full.stride / 4);
        panic!("{step}: the partial frame differs from the full raster first at ({x}, {y})");
    }
    assert_eq!(partial.scene_generation, full.scene_generation, "{step}");
}

/// Runs every change on one page, rendering each scene through a retained
/// compositor and a fresh one. Returns the retained compositor and the tiles
/// each change rastered again.
fn run(width: f64, height: f64, scale: f64, root_scrolls: bool) -> (SoftwareCompositor, Vec<u64>) {
    let mut page = page(width, height, scale, root_scrolls);
    let mut retained = SoftwareCompositor::default();
    let first = page.engine.scene().unwrap();
    assert_same(
        &retained.render(&first).unwrap(),
        &full(&first),
        "first frame",
    );
    let mut tiles = Vec::new();
    for (name, change) in CHANGES {
        change(&mut page);
        let scene = page.engine.scene().unwrap();
        let step = format!("{width}x{height}@{scale} {name}");
        let before = retained.partial_raster_stats().tiles;
        assert_same(&retained.render(&scene).unwrap(), &full(&scene), &step);
        tiles.push(retained.partial_raster_stats().tiles - before);
    }
    (retained, tiles)
}

#[test]
fn partial_frames_equal_full_rasters_at_1x() {
    let (retained, tiles) = run(1280.0, 800.0, 1.0, false);
    assert_eq!(retained.partial_raster_stats().frames, CHANGES.len() as u64);
    // 1280x800 is 6 by 4 tiles. A background change inside the first tile
    // column rasters one tile, or two where its damage meets a tile edge.
    assert!(tiles[0] <= 2, "{tiles:?}");
}

#[test]
fn partial_frames_equal_full_rasters_at_2x() {
    let (retained, _) = run(640.0, 400.0, 2.0, false);
    assert_eq!(retained.partial_raster_stats().frames, CHANGES.len() as u64);
}

#[test]
fn partial_frames_equal_full_rasters_under_viewport_controls() {
    run(1024.0, 700.0, 1.0, true);
}

#[test]
fn partial_frames_equal_full_rasters_at_a_fractional_scale() {
    // At 1.25 the physical height rounds up, so the last scanline is closed
    // from the first in every frame.
    run(1023.0, 601.0, 1.25, true);
    run(1023.0, 601.0, 1.25, false);
}

/// Content no larger than 512 pixels each way rasters untiled, and so in
/// full.
#[test]
fn an_untiled_page_rasters_in_full() {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(320.0, 240.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    engine
        .set_renderer_style(root, RendererStyleValue::Display(Display::Block))
        .unwrap();
    let a = block(&mut engine, root, 40.0, 20.0, Color::RED);
    block(&mut engine, root, 40.0, 20.0, Color::BLUE);
    let mut retained = SoftwareCompositor::default();
    let first = engine.scene().unwrap();
    assert_same(
        &retained.render(&first).unwrap(),
        &full(&first),
        "first frame",
    );
    engine
        .set_renderer_style(a, RendererStyleValue::BackgroundColor(Color::BLACK))
        .unwrap();
    let scene = engine.scene().unwrap();
    assert_same(&retained.render(&scene).unwrap(), &full(&scene), "untiled");
    assert_eq!(retained.partial_raster_stats().frames, 0);
}

/// A presenter can skip scenes the engine built; the next frame must still
/// equal a full raster.
#[test]
fn skipped_scenes_still_give_full_raster_pixels() {
    let mut page = page(1280.0, 800.0, 1.0, true);
    let mut retained = SoftwareCompositor::default();
    let first = page.engine.scene().unwrap();
    retained.render(&first).unwrap();
    for (index, (name, change)) in CHANGES.iter().enumerate() {
        change(&mut page);
        let scene = page.engine.scene().unwrap();
        if index % 2 == 0 {
            continue;
        }
        assert_same(&retained.render(&scene).unwrap(), &full(&scene), name);
    }
}

/// A scene without damage rasters no tile.
#[test]
fn an_undamaged_scene_rasters_no_tile() {
    let mut page = page(1280.0, 800.0, 1.0, false);
    let mut retained = SoftwareCompositor::default();
    let first = page.engine.scene().unwrap();
    retained.render(&first).unwrap();
    // Boxes without text do not paint their text color.
    set(&mut page, 0, RendererStyleValue::Color(Color::BLUE));
    let scene = page.engine.scene().unwrap();
    let damage = scene.physical_damage_since(&first);
    assert!(damage.is_empty(), "{damage:?}");
    assert_same(
        &retained.render(&scene).unwrap(),
        &full(&scene),
        "no damage",
    );
    assert_eq!(retained.partial_raster_stats().tiles, 0);
}
