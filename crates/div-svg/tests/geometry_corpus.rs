//! `div-svg` against Chromium 147's answers for the geometry corpus
//! (`tools/svg/geometry_corpus.py`), each OS against its own capture.
//!
//! Every value is compared exactly. A field of a case that differs must be
//! listed in `known-gaps.txt` next to the oracle, as `<case> <field>`, under
//! a comment giving the cause; a gap is never a tolerance, and the other
//! fields of that case stay exact. The test fails both on a new difference
//! and on a listed gap that no longer differs, so the list stays true.

#![cfg(any(target_os = "linux", windows))]

mod support;

use div_svg::geometry::{
    contains_in_fill, contains_in_stroke, point_at_length, total_length, FillRule,
};
use div_svg::shapes::Shape;
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::Point;
use serde_json::Value;
use std::collections::BTreeSet;
use support::{elements, shape, stroke_style, style_map, viewport};

// Each OS is checked against Chromium captured on that OS. The two
// captures are identical apart from their timestamp and platform.
#[cfg(target_os = "linux")]
const ORACLE: (&str, &str) = (
    "linux",
    include_str!("../../../tests/fidelity/fixtures/svg-geometry-v1/chromium-linux.json"),
);
#[cfg(windows)]
const ORACLE: (&str, &str) = (
    "win32",
    include_str!("../../../tests/fidelity/fixtures/svg-geometry-v1/chromium-windows.json"),
);
const KNOWN_GAPS: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-geometry-v1/known-gaps.txt");

fn f(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

fn floats(value: &Value) -> Vec<f64> {
    value.as_array().unwrap().iter().map(f).collect()
}

/// Compares one case; returns each differing field with its first
/// difference.
fn compare(case: &Value) -> Vec<(&'static str, String)> {
    let markup = case["svg"].as_str().unwrap();
    let chromium = &case["chromium"];
    let elems = elements(markup);
    let outer = viewport(&elems[0], None);
    // In `viewbox/nested` the probe is the inner `svg` and Chromium answers
    // for the `rect` inside it, which inherits the probe's style.
    let (vp, element, style) = if let Some(inner) = elems
        .iter()
        .position(|e| e.attrs.contains_key("data-inner"))
    {
        let probe = &elems[inner - 1];
        let mut style = style_map(probe.attrs.get("style").map_or("", |s| s));
        style.extend(style_map(elems[inner].attrs.get("style").map_or("", |s| s)));
        (viewport(probe, Some(&outer)), &elems[inner], style)
    } else {
        let probe = elems
            .iter()
            .find(|e| e.attrs.contains_key("data-probe"))
            .unwrap();
        let style = style_map(probe.attrs.get("style").map_or("", |s| s));
        (outer, probe, style)
    };
    let own = element
        .attrs
        .get("transform")
        .and_then(|t| parse_transform_list(t))
        .unwrap_or(Matrix::IDENTITY);
    let ctm = vp.ctm.then_apply(&own);
    let Shape { path, bbox, .. } = shape(element, &vp);

    let mut diffs = Vec::new();
    let mut check = |field: &'static str, ours: Vec<f64>, theirs: Vec<f64>| {
        if ours != theirs {
            diffs.push((field, format!("div {ours:?} chromium {theirs:?}")));
        }
    };
    // `getCTM` is a DOMMatrix of doubles, computed in double precision.
    // (`getScreenCTM` adds the case's page slot, which is layout's answer,
    // not this crate's.)
    check(
        "ctm",
        vec![ctm.a, ctm.b, ctm.c, ctm.d, ctm.e, ctm.f],
        floats(&chromium["ctm"]),
    );
    check(
        "bbox",
        [bbox.x, bbox.y, bbox.width, bbox.height]
            .iter()
            .map(|v| f64::from(*v))
            .collect(),
        floats(&chromium["bbox"]),
    );
    let Some(theirs_length) = chromium.get("total_length") else {
        return diffs;
    };
    let length = total_length(&path);
    check(
        "total_length",
        vec![f64::from(length)],
        vec![f(theirs_length)],
    );
    match chromium["points"].as_array() {
        Some(points) => {
            for (i, p) in points.iter().enumerate() {
                // The capture computes `total * i / 32` in double; the IDL
                // argument is a float.
                let distance = (f64::from(length) * i as f64 / 32.0) as f32;
                let ours = point_at_length(&path, distance).unwrap_or_default();
                let theirs = floats(p);
                let ours = vec![f64::from(ours.x), f64::from(ours.y)];
                if ours != theirs {
                    diffs.push(("points", format!("[{i}] div {ours:?} chromium {theirs:?}")));
                    break;
                }
            }
        }
        // Chromium throws: the path is empty.
        None => check(
            "points",
            vec![f64::from(u8::from(point_at_length(&path, 0.0).is_some()))],
            vec![0.0],
        ),
    }
    let grid = floats(&chromium["grid"]);
    let rule = if style.get("fill-rule").map(String::as_str) == Some("evenodd") {
        FillRule::EvenOdd
    } else {
        FillRule::NonZero
    };
    let stroke = stroke_style(&style);
    let mut fill = String::new();
    let mut in_stroke = String::new();
    for j in 0..17 {
        for i in 0..17 {
            let p = Point::new(
                (grid[0] + grid[2] * (i as f64 + 0.5) / 17.0) as f32,
                (grid[1] + grid[3] * (j as f64 + 0.5) / 17.0) as f32,
            );
            fill.push(if contains_in_fill(&path, p, rule) {
                '1'
            } else {
                '0'
            });
            in_stroke.push(if contains_in_stroke(&path, p, &stroke) {
                '1'
            } else {
                '0'
            });
        }
    }
    for (field, ours, theirs) in [
        ("in_fill", fill, chromium["in_fill"].as_str().unwrap()),
        (
            "in_stroke",
            in_stroke,
            chromium["in_stroke"].as_str().unwrap(),
        ),
    ] {
        if ours != theirs {
            let count = ours
                .chars()
                .zip(theirs.chars())
                .filter(|(a, b)| a != b)
                .count();
            diffs.push((field, format!("{count} of 289 grid points differ")));
        }
    }
    diffs
}

#[test]
fn geometry_matches_chromium_147() {
    let (platform, text) = ORACLE;
    let oracle: Value = serde_json::from_str(text).unwrap();
    assert_eq!(oracle["chromium"], "Chrome/147.0.7727.50");
    assert_eq!(oracle["platform"], platform);
    let known: BTreeSet<String> = KNOWN_GAPS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .collect();
    let mut report = Vec::new();
    let mut differing = BTreeSet::new();
    let mut exact = 0;
    let cases = oracle["cases"].as_array().unwrap();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let diffs = compare(case);
        exact += usize::from(diffs.is_empty());
        for (field, message) in diffs {
            let key = format!("{id} {field}");
            if !known.contains(&key) {
                report.push(format!("{key}: {message}"));
            }
            differing.insert(key);
        }
    }
    eprintln!(
        "{exact}/{} corpus cases exact; {} case fields differ",
        cases.len(),
        differing.len()
    );
    let new: Vec<_> = differing.difference(&known).collect();
    let fixed: Vec<_> = known.difference(&differing).collect();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "new gaps {new:?}; listed gaps now exact {fixed:?}\n\n{}",
        report.join("\n")
    );
}
