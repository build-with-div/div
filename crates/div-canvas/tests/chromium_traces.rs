//! Replays every trace in `tests/fidelity/fixtures/canvas-traces-v1/traces`
//! and compares it, exactly, with the Chromium 147.0.7727.50 capture for this
//! operating system: every observation (reads, return values, exceptions)
//! with `==`, and every canvas's `getImageData` bytes byte for byte.
//!
//! Regenerate the oracles with `tools/canvas/capture_canvas_traces.py` (see
//! the fixture's README). On a mismatch the test writes Div's bitmap next to
//! the expected one under `CARGO_TARGET_TMPDIR/canvas-traces`.
//!
//! A few canvases differ between Intel and AMD in pinned Chromium itself.
//! The fixture's `cpu-classes.json` lists them, with one oracle per CPU
//! class, chosen by this machine's RCPPS fingerprint. An unknown fingerprint
//! fails: no skip, no nearest class, no tolerance.

#[path = "support/rcpps.rs"]
mod rcpps;

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

/// A `fontFace` line's file: one of div-text's bundled fonts, which the
/// capture embeds from the same directory.
fn bundled_font(file: &str) -> Option<Vec<u8>> {
    if file.contains(['/', '\\']) || file.starts_with('.') {
        return None;
    }
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../div-text/fonts")
            .join(file),
    )
    .ok()
}

/// Canvases (`trace.canvas`) and observations (`trace#index`) known to
/// differ from one platform's Chromium, each with its cause. They are still
/// replayed and compared: a listed entry must keep differing (it is removed
/// when it starts to match), and is reported, not hidden.
const KNOWN_GAPS: &[(&str, &str, &str)] = &[(
    "linux",
    "text-transform-composite.c",
    "scaled, skewed and rotated web-font text: hinting and coverage differ \
     (the 2x1.5 scaled text's first ink row moves); div-text web-font \
     outlines under a transform, engine track",
)];

/// `cpu-classes.json`: each class's fingerprint, and the canvases that have
/// one oracle per class.
struct CpuClasses {
    classes: Vec<(u64, String)>,
    canvases: Vec<String>,
}

fn cpu_classes() -> CpuClasses {
    let json: Value = serde_json::from_str(
        &fs::read_to_string(fixture().join("cpu-classes.json")).expect("read cpu-classes.json"),
    )
    .expect("parse cpu-classes.json");
    let classes = json["classes"]
        .as_object()
        .expect("classes")
        .iter()
        .map(|(fingerprint, class)| {
            (
                u64::from_str_radix(fingerprint, 16).expect("hex fingerprint"),
                class.as_str().expect("class name").to_owned(),
            )
        })
        .collect();
    let canvases = json["canvases"]
        .as_array()
        .expect("canvases")
        .iter()
        .map(|key| key.as_str().expect("canvas key").to_owned())
        .collect();
    CpuClasses { classes, canvases }
}

/// The one class `fingerprint` selects, or why there is none.
fn class_for(cpu: &CpuClasses, fingerprint: Option<u64>) -> Result<&str, String> {
    let Some(fingerprint) = fingerprint else {
        return Err("no RCPPS fingerprint on this CPU (no AVX)".to_owned());
    };
    let mut matching = cpu.classes.iter().filter(|(f, _)| *f == fingerprint);
    match (matching.next(), matching.next()) {
        (Some((_, class)), None) => Ok(class),
        (None, _) => Err(format!(
            "unknown RCPPS fingerprint {fingerprint:016x}: no Chromium oracle for this CPU class"
        )),
        (Some(_), Some(_)) => Err(format!(
            "RCPPS fingerprint {fingerprint:016x} names more than one class"
        )),
    }
}

/// The oracle PNG for `key` (`trace.canvas`) on `platform` and this CPU.
fn oracle_png(
    root: &Path,
    platform: &str,
    key: &str,
    cpu: &CpuClasses,
    fingerprint: Option<u64>,
) -> Result<PathBuf, String> {
    let file = if cpu.canvases.iter().any(|k| k == key) {
        format!("{key}.{}.png", class_for(cpu, fingerprint)?)
    } else {
        format!("{key}.png")
    };
    Ok(root.join(format!("chromium-{platform}")).join(file))
}

/// The cause of a known gap for `key` on `platform`.
fn known_gap(platform: &str, key: &str) -> Option<&'static str> {
    KNOWN_GAPS
        .iter()
        .find(|(p, k, _)| *p == platform && *k == key)
        .map(|(_, _, why)| *why)
}

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

    let cpu = cpu_classes();
    let fingerprint = rcpps::fingerprint();
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
        let replay = trace::replay_with_fonts(&fs::read_to_string(path).unwrap(), &bundled_font)
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
            let key = format!("{name}#{i}");
            let gap = known_gap(platform, &key);
            if same(got, want) {
                if gap.is_some() {
                    failures.push(format!(
                        "{key}: now matches Chromium; remove it from KNOWN_GAPS"
                    ));
                }
            } else {
                let message = format!("{key}: Div {got}, Chromium {want}");
                match gap {
                    Some(why) => known.push(format!("{message}: known gap: {why}")),
                    None => failures.push(message),
                }
            }
        }

        let want_canvases = expected["canvases"].as_object().expect("canvases");
        assert_eq!(
            want_canvases.len(),
            replay.canvases.len(),
            "{name}: canvas count"
        );
        for (id, canvas) in &replay.canvases {
            let key = format!("{name}.{id}");
            let png = match oracle_png(&root, platform, &key, &cpu, fingerprint) {
                Ok(png) => png,
                Err(why) => {
                    failures.push(format!("{key}: {why}"));
                    continue;
                }
            };
            checked_canvases += 1;
            let (w, h, rgba) = decode_png(&png);
            assert_eq!(
                (w as i32, h as i32),
                (canvas.width, canvas.height),
                "{name}.{id}: size"
            );
            let gap = known_gap(platform, &key);
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

#[test]
fn a_fingerprint_selects_exactly_one_oracle() {
    let cpu = cpu_classes();
    let mut fingerprints: Vec<u64> = cpu.classes.iter().map(|(f, _)| *f).collect();
    let mut names: Vec<&str> = cpu.classes.iter().map(|(_, c)| c.as_str()).collect();
    fingerprints.sort();
    fingerprints.dedup();
    names.sort();
    names.dedup();
    assert_eq!(
        fingerprints.len(),
        cpu.classes.len(),
        "a fingerprint is listed twice"
    );
    assert_eq!(names, ["amd", "intel"]);
    for (fingerprint, class) in &cpu.classes {
        assert_eq!(class_for(&cpu, Some(*fingerprint)), Ok(class.as_str()));
    }
    let unknown = class_for(&cpu, Some(0x0123_4567_89ab_cdef)).unwrap_err();
    assert!(unknown.contains("0123456789abcdef"), "{unknown}");
    assert!(class_for(&cpu, None).is_err());

    // Each listed canvas has exactly one file per class on each platform,
    // and no class-free file to fall back on.
    let root = fixture();
    assert!(!cpu.canvases.is_empty());
    for platform in ["linux", "windows"] {
        let dir = root.join(format!("chromium-{platform}"));
        for key in &cpu.canvases {
            assert!(
                !dir.join(format!("{key}.png")).exists(),
                "{platform} {key}.png"
            );
            let mut found: Vec<String> = fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .filter(|f| {
                    f.strip_prefix(key.as_str())
                        .and_then(|rest| rest.strip_prefix('.'))
                        .is_some_and(|rest| rest.matches('.').count() == 1)
                })
                .collect();
            found.sort();
            let mut want: Vec<String> = names.iter().map(|c| format!("{key}.{c}.png")).collect();
            want.sort();
            assert_eq!(found, want, "{platform} {key}");
            for (fingerprint, class) in &cpu.classes {
                let png = oracle_png(&root, platform, key, &cpu, Some(*fingerprint)).unwrap();
                assert_eq!(png, dir.join(format!("{key}.{class}.png")));
            }
        }
    }
}

/// The pixels where pinned Chromium on AMD differs from Intel, as measured in
/// Div run 45123063142: canvas, pixel index, Intel RGBA, AMD RGBA.
const CPU_CLASS_PIXELS: [(&str, usize, [u8; 4], [u8; 4]); 7] = [
    (
        "gradient-composite.color-dodge",
        83,
        [61, 139, 138, 235],
        [61, 139, 139, 235],
    ),
    (
        "gradient-composite.color-dodge",
        275,
        [61, 139, 138, 235],
        [61, 139, 139, 235],
    ),
    (
        "gradient-composite.hue",
        107,
        [130, 86, 143, 235],
        [130, 87, 143, 235],
    ),
    (
        "gradient-composite.hue",
        299,
        [130, 86, 143, 235],
        [130, 87, 143, 235],
    ),
    (
        "gradient-composite.saturation",
        79,
        [35, 146, 84, 240],
        [34, 146, 84, 240],
    ),
    (
        "gradient-composite.saturation",
        155,
        [42, 140, 94, 235],
        [43, 140, 94, 235],
    ),
    (
        "gradient-composite.saturation",
        271,
        [35, 146, 84, 240],
        [34, 146, 84, 240],
    ),
];

#[test]
fn intel_and_amd_oracles_differ_only_at_the_known_pixels() {
    let cpu = cpu_classes();
    let root = fixture();
    for platform in ["linux", "windows"] {
        let dir = root.join(format!("chromium-{platform}"));
        let mut differing = Vec::new();
        for key in &cpu.canvases {
            let (iw, ih, intel) = decode_png(&dir.join(format!("{key}.intel.png")));
            let (aw, ah, amd) = decode_png(&dir.join(format!("{key}.amd.png")));
            assert_eq!((iw, ih), (aw, ah), "{platform} {key}: size");
            for (index, (i, a)) in intel.chunks(4).zip(amd.chunks(4)).enumerate() {
                if i != a {
                    differing.push((
                        key.as_str(),
                        index,
                        <[u8; 4]>::try_from(i).unwrap(),
                        <[u8; 4]>::try_from(a).unwrap(),
                    ));
                }
            }
        }
        assert_eq!(differing, CPU_CLASS_PIXELS, "{platform}");
    }
}
