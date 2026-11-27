//! `pixel_compare damage-coverage`: the RFC 0006 A1 gate.
//!
//! For every registered fixture, under the selected render profile, this
//! renders a full frame, applies one mutation, renders the next full frame,
//! and checks that every pixel that differs between the two lies inside the
//! next scene's physical damage. The mutations cover each invalidation
//! class Div has (paint, composite, layout, intrinsic, subtree), scrolling
//! and animation steps; they run in sequence on one engine, so each step is
//! compared with the frame before it.
//!
//! The over-damage it prints (damaged area over viewport area, and how many
//! steps fell back to the whole viewport) is a statistic, not a gate.

use div_compositor::{Frame, PhysicalSceneRect, SceneSnapshot, SoftwareCompositor};
use div_dom::ElementTag;
use div_engine::{AnimationTimeline, Engine, NodeHandle};
use div_geometry::{Length, ViewportMetrics};
use div_style::{
    AnimationOptions, Color, Display, FillMode, Keyframes, PropertyKeyframes, RendererStyleValue,
    StyleProperty,
};

type Mutation = (&'static str, fn(&mut Engine, &[NodeHandle]) -> bool);

fn style(
    engine: &mut Engine,
    targets: &[NodeHandle],
    value: impl Fn() -> RendererStyleValue,
) -> bool {
    let mut any = false;
    for &node in targets {
        any |= engine.set_renderer_style(node, value()).is_ok();
    }
    any
}

const MUTATIONS: &[Mutation] = &[
    ("paint/background-color", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::BackgroundColor(Color::from_rgba8(12, 140, 90, 255))
        })
    }),
    ("paint/color", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::Color(Color::from_rgba8(200, 30, 60, 255))
        })
    }),
    ("composite/opacity", |engine, targets| {
        style(engine, targets, || RendererStyleValue::Opacity(0.5))
    }),
    ("layout/margin-left", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::MarginLeft(Length::px(7.0))
        })
    }),
    ("layout/width", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::Width(Length::px(37.0))
        })
    }),
    ("intrinsic/font-size", |engine, targets| {
        style(engine, targets, || RendererStyleValue::FontSize(21.0))
    }),
    ("subtree/append", |engine, targets| {
        let mut any = false;
        for &parent in targets {
            let Ok(child) = engine.create_native_element(ElementTag::Div) else {
                continue;
            };
            let _ = engine.set_renderer_style(child, RendererStyleValue::Display(Display::Block));
            let _ = engine.set_renderer_style(child, RendererStyleValue::Width(Length::px(11.0)));
            let _ = engine.set_renderer_style(child, RendererStyleValue::Height(Length::px(9.0)));
            let _ = engine.set_renderer_style(
                child,
                RendererStyleValue::BackgroundColor(Color::from_rgba8(30, 60, 200, 255)),
            );
            any |= engine.append_child(parent, child).is_ok();
        }
        any
    }),
    ("scroll/to-13", |engine, targets| {
        let mut any = false;
        for &node in targets.iter().chain(std::iter::once(&engine.root())) {
            any |= engine.scroll_to(node, 13.0, 13.0).is_ok();
        }
        any
    }),
    ("animation/start", |engine, targets| {
        let mut any = false;
        for &node in targets {
            let Ok(keyframes) = PropertyKeyframes::typed(
                StyleProperty::Opacity,
                Keyframes::from_values(0.2_f32, 0.9_f32),
            ) else {
                continue;
            };
            let options = AnimationOptions {
                duration_ms: 1000.0,
                fill: FillMode::Both,
                ..AnimationOptions::default()
            };
            any |= engine
                .animate(node, keyframes, options, AnimationTimeline::Document)
                .is_ok();
        }
        any
    }),
    ("animation/250ms", |engine, _| {
        engine.set_animation_time(250.0).is_ok()
    }),
    ("animation/600ms", |engine, _| {
        engine.set_animation_time(600.0).is_ok()
    }),
    ("subtree/display-none", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::Display(Display::None)
        })
    }),
    ("subtree/display-block", |engine, targets| {
        style(engine, targets, || {
            RendererStyleValue::Display(Display::Block)
        })
    }),
    ("subtree/remove", |engine, targets| {
        let mut any = false;
        for &node in targets {
            any |= engine.remove(node).is_ok();
        }
        any
    }),
];

fn elements(engine: &Engine) -> Vec<NodeHandle> {
    let mut out = Vec::new();
    let mut stack = vec![engine.root()];
    while let Some(node) = stack.pop() {
        out.push(node);
        if let Ok(children) = engine.children(node) {
            stack.extend(children.into_iter().rev());
        }
    }
    out
}

/// Three nodes spread through the tree (not the root), so a mutation
/// touches a shallow, a middle and a deep node.
fn targets(engine: &Engine) -> Vec<NodeHandle> {
    let nodes = elements(engine);
    if nodes.len() <= 1 {
        return Vec::new();
    }
    let rest = &nodes[1..];
    let mut picked = vec![rest[0], rest[rest.len() / 2], rest[rest.len() - 1]];
    picked.dedup();
    picked
}

fn frame(engine: &mut Engine) -> Result<(Frame, SceneSnapshot), String> {
    let scene = engine.scene().map_err(|e| e.to_string())?;
    let frame = SoftwareCompositor::default()
        .render(&scene)
        .map_err(|e| e.to_string())?;
    Ok((frame, scene))
}

struct Step {
    uncovered: usize,
    changed: usize,
    damaged_area: u64,
    full: bool,
}

fn compare(before: &Frame, after: &Frame, damage: &[PhysicalSceneRect]) -> Step {
    let mut step = Step {
        uncovered: 0,
        changed: 0,
        damaged_area: 0,
        full: damage.len() == 1
            && damage[0].x == 0
            && damage[0].y == 0
            && damage[0].width == after.width
            && damage[0].height == after.height,
    };
    let mut mask = vec![false; after.width as usize * after.height as usize];
    for rect in damage {
        for y in rect.y..(rect.y + rect.height).min(after.height) {
            for x in rect.x..(rect.x + rect.width).min(after.width) {
                mask[y as usize * after.width as usize + x as usize] = true;
            }
        }
    }
    step.damaged_area = mask.iter().filter(|&&m| m).count() as u64;
    if (before.width, before.height) != (after.width, after.height) {
        step.uncovered = usize::from(!step.full);
        return step;
    }
    for y in 0..after.height as usize {
        let a = &before.pixels[y * before.stride..y * before.stride + after.width as usize * 4];
        let b = &after.pixels[y * after.stride..y * after.stride + after.width as usize * 4];
        if a == b {
            continue;
        }
        for x in 0..after.width as usize {
            if a[x * 4..x * 4 + 4] != b[x * 4..x * 4 + 4] {
                step.changed += 1;
                if !mask[y * after.width as usize + x] {
                    step.uncovered += 1;
                }
            }
        }
    }
    step
}

/// Per-mutation statistics: steps, steps that changed no pixel, steps
/// damaged as one full-viewport rect, and the damaged and changed pixels.
#[derive(Default)]
struct Totals {
    steps: usize,
    unchanged: usize,
    full: usize,
    damaged: u64,
    changed: u64,
    area: u64,
}

/// Runs the gate over `fixtures` and returns whether every step passed.
/// Each step is announced on stderr first, so an abort names its step.
pub fn run(
    fixtures: &[(&'static str, crate::TestBuilder)],
    viewport: ViewportMetrics,
    filter: Option<&str>,
) -> bool {
    let (mut steps, mut failures, mut fallbacks, mut errors) = (0usize, 0usize, 0usize, 0usize);
    let (mut damaged, mut total_area) = (0u64, 0u64);
    let mut totals: Vec<Totals> = MUTATIONS.iter().map(|_| Totals::default()).collect();
    for (id, builder) in fixtures {
        // A filter is an id prefix, or one exact id when it ends in `$`.
        let selected = filter.is_none_or(|filter| match filter.strip_suffix('$') {
            Some(exact) => id.len() == exact.len() && id.starts_with(exact),
            None => id.starts_with(filter),
        });
        if !selected {
            continue;
        }
        let Ok(mut engine) = builder(viewport) else {
            errors += 1;
            continue;
        };
        let Ok((mut previous, mut previous_scene)) = frame(&mut engine) else {
            errors += 1;
            continue;
        };
        let picked = targets(&engine);
        for (index, (name, mutation)) in MUTATIONS.iter().enumerate() {
            eprintln!("step {id} {name}");
            if !mutation(&mut engine, &picked) {
                continue;
            }
            let Ok((next, scene)) = frame(&mut engine) else {
                errors += 1;
                break;
            };
            // What a presenter would use: the damage from the scene it last
            // presented, across any scenes the engine built in between.
            let damage = scene.physical_damage_since(&previous_scene);
            let step = compare(&previous, &next, &damage);
            steps += 1;
            damaged += step.damaged_area;
            total_area += u64::from(next.width) * u64::from(next.height);
            fallbacks += usize::from(step.full);
            let total = &mut totals[index];
            total.steps += 1;
            total.unchanged += usize::from(step.changed == 0);
            total.full += usize::from(step.full);
            total.damaged += step.damaged_area;
            total.changed += step.changed as u64;
            total.area += u64::from(next.width) * u64::from(next.height);
            if step.uncovered > 0 {
                failures += 1;
                println!(
                    "UNCOVERED {id} {name}: {} of {} changed pixels outside {} damage rects",
                    step.uncovered,
                    step.changed,
                    damage.len()
                );
            }
            previous = next;
            previous_scene = scene;
        }
    }
    println!(
        "damage-coverage: {steps} steps, {failures} with uncovered pixels, {fallbacks} full-viewport, \
         {errors} fixture errors; damaged area {:.1}% of the viewport",
        if total_area == 0 { 0.0 } else { damaged as f64 * 100.0 / total_area as f64 }
    );
    for ((name, _), total) in MUTATIONS.iter().zip(&totals) {
        let percent = |part: u64| {
            if total.area == 0 {
                0.0
            } else {
                part as f64 * 100.0 / total.area as f64
            }
        };
        println!(
            "  {name}: {} steps, {} unchanged, {} full-viewport; damaged {:.1}%, changed {:.1}%",
            total.steps,
            total.unchanged,
            total.full,
            percent(total.damaged),
            percent(total.changed)
        );
    }
    failures == 0
}
