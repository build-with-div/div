//! Replays every trace in `tests/fidelity/fixtures/canvas-traces-v1/traces`
//! and compares it, exactly, with the Chromium 147.0.7727.50 capture for this
//! operating system: every observation (reads, return values, exceptions)
//! with `==`, and every canvas's `getImageData` bytes byte for byte.
//!
//! Regenerate the oracles with `tools/canvas/capture_canvas_traces.py` (see
//! the fixture's README). On a mismatch the test writes Div's bitmap next to
//! the expected one under `CARGO_TARGET_TMPDIR/canvas-traces`.

use std::fs;
use std::path::{Path, PathBuf};

use div_canvas::trace;
use serde_json::Value;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fidelity/fixtures/canvas-traces-v1")
}

fn platform() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("windows")
    } else if cfg!(target_os = "linux") {
        Some("linux")
    } else {
        None
    }
}

/// JSON equality where numbers compare as `f64` bit patterns, so `1` and
/// `1.0` are equal and `0.1` must round-trip exactly.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            x.as_f64().map(f64::to_bits) == y.as_f64().map(f64::to_bits)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(fs::File::open(path).expect("open oracle PNG"));
    let mut reader = decoder.read_info().expect("read PNG header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode PNG");
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{}", path.display());
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = fs::File::create(path).expect("create PNG");
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .expect("write PNG");
}

/// Canvases known to differ from Chromium, each with its cause. They are
/// still replayed and compared: a listed canvas must keep differing (the
/// entry is removed when it starts to match), and is reported, not hidden.
const KNOWN_GAPS: &[(&str, &str)] = &[
    (
        "arcs-closed-full.stroke",
        "closed full-turn arc on the drawArc fast path: cc DrawArcOp playback not ported",
    ),
    (
        "arcs-closed-full.dashed",
        "closed full-turn arc on the drawArc fast path: cc DrawArcOp playback not ported",
    ),
];

#[test]
fn traces_match_chromium() {
    let Some(platform) = platform() else {
        eprintln!("no Chromium canvas oracle for this operating system");
        return;
    };
    let root = fixture();
    let oracle: Value = serde_json::from_str(
        &fs::read_to_string(root.join(format!("chromium-{platform}.json"))).expect("read oracle"),
    )
    .expect("parse oracle");
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let mut traces: Vec<PathBuf> = fs::read_dir(root.join("traces"))
        .expect("list traces")
        .map(|e| e.expect("trace entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    traces.sort();
    assert!(!traces.is_empty());

    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("canvas-traces");
    let mut failures = Vec::new();
    let mut known = Vec::new();
    let mut checked_canvases = 0;
    let mut checked_observations = 0;
    for path in &traces {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let expected = &oracle["traces"][name];
        assert!(
            expected.is_object(),
            "{name}: no Chromium capture; rerun capture_canvas_traces.py"
        );
        let replay = trace::replay(&fs::read_to_string(path).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));

        let want = expected["observations"].as_array().expect("observations");
        if want.len() != replay.observations.len() {
            failures.push(format!(
                "{name}: {} observations, Chromium has {}",
                replay.observations.len(),
                want.len()
            ));
        }
        for (i, (got, want)) in replay.observations.iter().zip(want).enumerate() {
            checked_observations += 1;
            if !same(got, want) {
                failures.push(format!(
                    "{name}: observation {i}: Div {got}, Chromium {want}"
                ));
            }
        }

        let want_canvases = expected["canvases"].as_object().expect("canvases");
        assert_eq!(
            want_canvases.len(),
            replay.canvases.len(),
            "{name}: canvas count"
        );
        for (id, canvas) in &replay.canvases {
            checked_canvases += 1;
            let png = root
                .join(format!("chromium-{platform}"))
                .join(format!("{name}.{id}.png"));
            let (w, h, rgba) = decode_png(&png);
            assert_eq!(
                (w as i32, h as i32),
                (canvas.width, canvas.height),
                "{name}.{id}: size"
            );
            let key = format!("{name}.{id}");
            let gap = KNOWN_GAPS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, why)| *why);
            if rgba == canvas.rgba {
                if gap.is_some() {
                    failures.push(format!(
                        "{key}: now matches Chromium; remove it from KNOWN_GAPS"
                    ));
                }
            } else {
                let differing = rgba
                    .chunks(4)
                    .zip(canvas.rgba.chunks(4))
                    .filter(|(a, b)| a != b)
                    .count();
                let max = rgba
                    .iter()
                    .zip(&canvas.rgba)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                fs::create_dir_all(&out).unwrap();
                write_png(
                    &out.join(format!("{name}.{id}.div.png")),
                    w,
                    h,
                    &canvas.rgba,
                );
                fs::copy(&png, out.join(format!("{name}.{id}.chromium.png"))).unwrap();
                let message = format!(
                    "{key}: {differing} of {} pixels differ (max channel delta {max})",
                    w * h
                );
                match gap {
                    Some(why) => {
                        known.push(format!("{message}: known gap: {why}"));
                    }
                    None => failures.push(message),
                }
            }
        }
    }
    eprintln!(
        "{} traces, {checked_canvases} canvases, {checked_observations} observations checked against Chromium ({platform})",
        traces.len()
    );
    for line in &known {
        eprintln!("{line}");
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es) with Chromium ({platform}); bitmaps in {}:\n{}",
        failures.len(),
        out.display(),
        failures.join("\n")
    );
}
