//! The canvas trace format (RFC 0008 §7 item 3).
//!
//! A trace is JSON lines. Each line names a canvas and does one thing:
//!
//! - `{"canvas": id, "width": w, "height": h}` creates a canvas;
//! - `{"canvas": id, "call": name, "args": [...]}` calls a method;
//! - `{"canvas": id, "set": prop, "value": v}` assigns an attribute;
//! - `{"canvas": id, "get": prop}` reads an attribute;
//! - `{"fontFace": family, "file": name, "weight": w, "style": s}` adds a
//!   font face, as `document.fonts.add(new FontFace(...))` does: `file` names
//!   one of div-text's bundled fonts (`crates/div-text/fonts`), which the
//!   capture embeds and [`replay_with_fonts`]'s loader supplies. Every canvas
//!   of the trace sees every face, as a document's canvases do;
//! - `{"imageData": name, "width": w, "height": h}` creates a named
//!   `ImageData`, and with `"data": hex` (then `height` is optional) creates
//!   it from those bytes. A constructor that throws is an observation.
//!
//! A `getImageData` or `createImageData` call may add `"store": name` to keep
//! its result, and `{"imageData": name}` as an argument passes a kept one. A
//! returned `ImageData` is observed as its width, height and bytes in hex.
//!
//! Numbers that JSON cannot carry are strings: `"NaN"`, `"Infinity"`,
//! `"-Infinity"` and `"-0"`. Replaying a trace yields its observations (every
//! read, every return value and every exception, in order) and each canvas's
//! final bitmap. The Chromium capture (`tools/canvas/capture_canvas_traces.py`)
//! replays the same lines with JavaScript and writes the same observations,
//! so the two compare with `==`.

use std::collections::BTreeMap;

use std::sync::Arc;

use div_text::{FontAxisRange, FontCollection, FontFaceDescriptor, FontStyleRange};
use serde_json::{json, Map, Number, Value};

use crate::{
    CanvasError, CanvasRenderingContext2D, FillRule, ImageData, RoundRectRadius, TextMetrics,
};

/// One canvas's final state after a replay.
pub struct ReplayedCanvas {
    pub width: i32,
    pub height: i32,
    /// Unpremultiplied RGBA8888, as `getImageData` returns it.
    pub rgba: Vec<u8>,
}

/// The result of replaying a trace.
pub struct Replay {
    /// Observation lines, in trace order.
    pub observations: Vec<Value>,
    /// Final bitmaps by canvas id.
    pub canvases: BTreeMap<String, ReplayedCanvas>,
}

/// An error in the trace itself (not a canvas exception).
#[derive(Debug)]
pub struct TraceError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "trace line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TraceError {}

/// Decodes a trace number.
pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => match s.as_str() {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "-0" => Some(-0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Encodes a number as a trace value.
pub fn encode_number(v: f64) -> Value {
    if v.is_nan() {
        json!("NaN")
    } else if v == f64::INFINITY {
        json!("Infinity")
    } else if v == f64::NEG_INFINITY {
        json!("-Infinity")
    } else if v == 0.0 && v.is_sign_negative() {
        json!("-0")
    } else if v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        // Integral values print as integers, as JSON.stringify does.
        Value::Number(Number::from(v as i64))
    } else {
        Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

/// Replays `trace` (JSON lines; blank lines are skipped). A trace that adds
/// font faces needs [`replay_with_fonts`].
pub fn replay(trace: &str) -> Result<Replay, TraceError> {
    replay_with_fonts(trace, &|_| None)
}

/// Replays `trace`, reading the font files its `fontFace` lines name with
/// `load`.
pub fn replay_with_fonts(
    trace: &str,
    load: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Replay, TraceError> {
    let fonts = FontCollection::system();
    let mut canvases: BTreeMap<String, CanvasRenderingContext2D> = BTreeMap::new();
    let mut images: BTreeMap<String, ImageData> = BTreeMap::new();
    let mut observations = Vec::new();
    for (index, line) in trace.lines().enumerate() {
        let line_no = index + 1;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let err = |message: String| TraceError {
            line: line_no,
            message,
        };
        let entry: Map<String, Value> =
            serde_json::from_str(line).map_err(|e| err(e.to_string()))?;
        if let Some(family) = entry.get("fontFace").and_then(Value::as_str) {
            add_font_face(&fonts, family, &entry, load).map_err(err)?;
            continue;
        }
        if let Some(image_name) = entry.get("imageData").and_then(Value::as_str) {
            match new_image_data(&entry).map_err(err)? {
                Ok(image) => {
                    images.insert(image_name.to_string(), image);
                }
                Err(e) => observations.push(json!({"imageData": image_name, "throws": e.name()})),
            }
            continue;
        }
        let id = entry
            .get("canvas")
            .and_then(Value::as_str)
            .ok_or_else(|| err("missing canvas id".into()))?
            .to_string();
        if let (Some(w), Some(h)) = (entry.get("width"), entry.get("height")) {
            let (w, h) = (
                w.as_i64().ok_or_else(|| err("bad width".into()))? as i32,
                h.as_i64().ok_or_else(|| err("bad height".into()))? as i32,
            );
            let ctx = CanvasRenderingContext2D::with_fonts(w, h, Arc::clone(&fonts))
                .ok_or_else(|| err(format!("cannot create a {w}x{h} canvas")))?;
            canvases.insert(id, ctx);
            continue;
        }
        let ctx = canvases
            .get_mut(&id)
            .ok_or_else(|| err(format!("unknown canvas {id}")))?;
        if let Some(name) = entry.get("call").and_then(Value::as_str) {
            let args = entry
                .get("args")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            match call(ctx, name, &args, &images).map_err(err)? {
                CallResult::Void => {}
                CallResult::Return(value) => {
                    observations.push(json!({"canvas": id, "call": name, "return": value}))
                }
                CallResult::Image(image) => {
                    observations
                        .push(json!({"canvas": id, "call": name, "return": image_value(&image)}));
                    if let Some(store) = entry.get("store").and_then(Value::as_str) {
                        images.insert(store.to_string(), image);
                    }
                }
                CallResult::Throw(e) => {
                    observations.push(json!({"canvas": id, "call": name, "throws": e.name()}))
                }
            }
        } else if let Some(prop) = entry.get("set").and_then(Value::as_str) {
            let value = entry.get("value").cloned().unwrap_or(Value::Null);
            set(ctx, prop, &value).map_err(err)?;
        } else if let Some(prop) = entry.get("get").and_then(Value::as_str) {
            let value = get(ctx, prop).map_err(err)?;
            observations.push(json!({"canvas": id, "get": prop, "value": value}));
        } else {
            return Err(err("line has no call, set or get".into()));
        }
    }
    let canvases = canvases
        .into_iter()
        .map(|(id, mut ctx)| {
            let replayed = ReplayedCanvas {
                width: ctx.width(),
                height: ctx.height(),
                rgba: ctx.image_data(),
            };
            (id, replayed)
        })
        .collect();
    Ok(Replay {
        observations,
        canvases,
    })
}

/// Registers a `fontFace` line's face.
fn add_font_face(
    fonts: &FontCollection,
    family: &str,
    entry: &Map<String, Value>,
    load: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), String> {
    let file = entry
        .get("file")
        .and_then(Value::as_str)
        .ok_or("fontFace: missing file")?;
    let bytes = load(file).ok_or_else(|| format!("fontFace: cannot load {file}"))?;
    let mut descriptor = FontFaceDescriptor::new(family);
    if let Some(weight) = entry.get("weight").and_then(Value::as_str) {
        let w: f32 = weight
            .parse()
            .map_err(|_| format!("fontFace: bad weight {weight}"))?;
        descriptor.weight = FontAxisRange::new(w, w);
    }
    descriptor.style = match entry.get("style").and_then(Value::as_str) {
        None | Some("normal") => FontStyleRange::Normal,
        Some("italic") => FontStyleRange::Italic,
        Some(other) => return Err(format!("fontFace: unsupported style {other}")),
    };
    fonts
        .register(Arc::from(bytes), descriptor)
        .map_err(|e| format!("fontFace {family}: {e}"))?;
    Ok(())
}

/// An `imageData` line's `ImageData`. The outer error is a bad trace line, the
/// inner one what the constructor throws.
fn new_image_data(entry: &Map<String, Value>) -> Result<Result<ImageData, CanvasError>, String> {
    let side = |key: &str| -> Result<Option<u32>, String> {
        entry
            .get(key)
            .map(|v| {
                v.as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("imageData: bad {key}"))
            })
            .transpose()
    };
    let width = side("width")?.ok_or("imageData: missing width")?;
    let height = side("height")?;
    Ok(match entry.get("data") {
        Some(data) => {
            let hex = data.as_str().ok_or("imageData: data is not a string")?;
            ImageData::from_data(decode_hex(hex)?, width, height)
        }
        None => ImageData::new(width, height.ok_or("imageData: missing height")?),
    })
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.is_ascii() || hex.len() % 2 != 0 {
        return Err("imageData: data is not hex bytes".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| "imageData: data is not hex bytes".to_string())
        })
        .collect()
}

/// A returned `ImageData` as the capture lists it.
fn image_value(image: &ImageData) -> Value {
    let mut data = String::with_capacity(image.data().len() * 2);
    for byte in image.data() {
        data.push_str(&format!("{byte:02x}"));
    }
    json!({"width": image.width(), "height": image.height(), "data": data})
}

/// A `TextMetrics` as the capture lists it.
fn text_metrics(m: &TextMetrics) -> Value {
    json!({
        "width": encode_number(m.width),
        "actualBoundingBoxLeft": encode_number(m.actual_bounding_box_left),
        "actualBoundingBoxRight": encode_number(m.actual_bounding_box_right),
        "fontBoundingBoxAscent": encode_number(m.font_bounding_box_ascent),
        "fontBoundingBoxDescent": encode_number(m.font_bounding_box_descent),
        "actualBoundingBoxAscent": encode_number(m.actual_bounding_box_ascent),
        "actualBoundingBoxDescent": encode_number(m.actual_bounding_box_descent),
        "hangingBaseline": encode_number(m.hanging_baseline),
        "alphabeticBaseline": encode_number(m.alphabetic_baseline),
        "ideographicBaseline": encode_number(m.ideographic_baseline),
    })
}

enum CallResult {
    Void,
    Return(Value),
    Image(ImageData),
    Throw(CanvasError),
}

fn from_result(r: Result<(), CanvasError>) -> CallResult {
    match r {
        Ok(()) => CallResult::Void,
        Err(e) => CallResult::Throw(e),
    }
}

fn image_result(r: Result<ImageData, CanvasError>) -> CallResult {
    match r {
        Ok(image) => CallResult::Image(image),
        Err(e) => CallResult::Throw(e),
    }
}

fn call(
    ctx: &mut CanvasRenderingContext2D,
    name: &str,
    args: &[Value],
    images: &BTreeMap<String, ImageData>,
) -> Result<CallResult, String> {
    let n = |i: usize| -> Result<f64, String> {
        args.get(i)
            .and_then(number)
            .ok_or_else(|| format!("{name}: argument {i} is not a number"))
    };
    // `[EnforceRange] long`: a value out of range would be a TypeError from
    // the bindings, which the Rust API cannot receive, so a trace may not
    // contain one.
    let long = |i: usize| -> Result<i32, String> {
        let v = n(i)?.trunc();
        if v.is_finite() && v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
            Ok(v as i32)
        } else {
            Err(format!("{name}: argument {i} is not a long"))
        }
    };
    let image = |i: usize| -> Result<&ImageData, String> {
        args.get(i)
            .and_then(|v| v.get("imageData"))
            .and_then(Value::as_str)
            .and_then(|key| images.get(key))
            .ok_or_else(|| format!("{name}: argument {i} is not a stored ImageData"))
    };
    let flag = |i: usize| -> bool { args.get(i).and_then(Value::as_bool).unwrap_or(false) };
    let string = |i: usize| -> Result<&str, String> {
        args.get(i)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{name}: argument {i} is not a string"))
    };
    let optional = |i: usize| -> Result<Option<f64>, String> {
        if args.len() > i {
            n(i).map(Some)
        } else {
            Ok(None)
        }
    };
    let rule = |i: usize| -> Result<FillRule, String> {
        match args.get(i) {
            None => Ok(FillRule::NonZero),
            Some(v) => v
                .as_str()
                .and_then(FillRule::parse)
                .ok_or_else(|| format!("{name}: bad fill rule {v}")),
        }
    };
    Ok(match name {
        "save" => {
            ctx.save();
            CallResult::Void
        }
        "restore" => {
            ctx.restore();
            CallResult::Void
        }
        "scale" => {
            ctx.scale(n(0)?, n(1)?);
            CallResult::Void
        }
        "rotate" => {
            ctx.rotate(n(0)?);
            CallResult::Void
        }
        "translate" => {
            ctx.translate(n(0)?, n(1)?);
            CallResult::Void
        }
        "transform" => {
            ctx.transform(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?);
            CallResult::Void
        }
        "setTransform" => {
            ctx.set_transform(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?);
            CallResult::Void
        }
        "resetTransform" => {
            ctx.reset_transform();
            CallResult::Void
        }
        "getTransform" => CallResult::Return(Value::Array(
            ctx.get_transform()
                .iter()
                .map(|&v| encode_number(v))
                .collect(),
        )),
        "beginPath" => {
            ctx.begin_path();
            CallResult::Void
        }
        "closePath" => {
            ctx.close_path();
            CallResult::Void
        }
        "moveTo" => {
            ctx.move_to(n(0)?, n(1)?);
            CallResult::Void
        }
        "lineTo" => {
            ctx.line_to(n(0)?, n(1)?);
            CallResult::Void
        }
        "quadraticCurveTo" => {
            ctx.quadratic_curve_to(n(0)?, n(1)?, n(2)?, n(3)?);
            CallResult::Void
        }
        "bezierCurveTo" => {
            ctx.bezier_curve_to(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?);
            CallResult::Void
        }
        "arcTo" => from_result(ctx.arc_to(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?)),
        "arc" => from_result(ctx.arc(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, flag(5))),
        "ellipse" => {
            from_result(ctx.ellipse(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?, n(6)?, flag(7)))
        }
        "rect" => {
            ctx.rect(n(0)?, n(1)?, n(2)?, n(3)?);
            CallResult::Void
        }
        "roundRect" => {
            let radii = round_rect_radii(args.get(4)).ok_or("roundRect: bad radii")?;
            from_result(ctx.round_rect(n(0)?, n(1)?, n(2)?, n(3)?, &radii))
        }
        "fill" => {
            ctx.fill(rule(0)?);
            CallResult::Void
        }
        "stroke" => {
            ctx.stroke();
            CallResult::Void
        }
        "clip" => {
            ctx.clip(rule(0)?);
            CallResult::Void
        }
        "fillRect" => {
            ctx.fill_rect(n(0)?, n(1)?, n(2)?, n(3)?);
            CallResult::Void
        }
        "strokeRect" => {
            ctx.stroke_rect(n(0)?, n(1)?, n(2)?, n(3)?);
            CallResult::Void
        }
        "clearRect" => {
            ctx.clear_rect(n(0)?, n(1)?, n(2)?, n(3)?);
            CallResult::Void
        }
        "isPointInPath" => {
            CallResult::Return(Value::Bool(ctx.is_point_in_path(n(0)?, n(1)?, rule(2)?)))
        }
        "isPointInStroke" => CallResult::Return(Value::Bool(ctx.is_point_in_stroke(n(0)?, n(1)?))),
        "setLineDash" => {
            let list = args
                .first()
                .and_then(Value::as_array)
                .ok_or("setLineDash: argument is not a list")?;
            let dash: Option<Vec<f64>> = list.iter().map(number).collect();
            ctx.set_line_dash(&dash.ok_or("setLineDash: non-numeric entry")?);
            CallResult::Void
        }
        "getLineDash" => CallResult::Return(Value::Array(
            ctx.get_line_dash().into_iter().map(encode_number).collect(),
        )),
        "fillText" => {
            ctx.fill_text(string(0)?, n(1)?, n(2)?, optional(3)?);
            CallResult::Void
        }
        "strokeText" => {
            ctx.stroke_text(string(0)?, n(1)?, n(2)?, optional(3)?);
            CallResult::Void
        }
        "measureText" => CallResult::Return(text_metrics(&ctx.measure_text(string(0)?))),
        "createImageData" => image_result(if args.len() == 1 {
            ctx.create_image_data_from(image(0)?)
        } else {
            ctx.create_image_data(long(0)?, long(1)?)
        }),
        "getImageData" => image_result(ctx.get_image_data(long(0)?, long(1)?, long(2)?, long(3)?)),
        "putImageData" => {
            if args.len() > 3 {
                ctx.put_image_data_dirty(
                    image(0)?,
                    long(1)?,
                    long(2)?,
                    long(3)?,
                    long(4)?,
                    long(5)?,
                    long(6)?,
                );
            } else {
                ctx.put_image_data(image(0)?, long(1)?, long(2)?);
            }
            CallResult::Void
        }
        other => return Err(format!("unsupported call {other}")),
    })
}

/// `roundRect`'s fifth argument: a number, a `{x, y}` point, or a list of
/// them. Absent means 0.
fn round_rect_radii(value: Option<&Value>) -> Option<Vec<RoundRectRadius>> {
    fn one(v: &Value) -> Option<RoundRectRadius> {
        if let Some(obj) = v.as_object() {
            let x = obj.get("x").map_or(Some(0.0), number)?;
            let y = obj.get("y").map_or(Some(0.0), number)?;
            return Some(RoundRectRadius::Point { x, y });
        }
        number(v).map(RoundRectRadius::Uniform)
    }
    match value {
        None => Some(vec![RoundRectRadius::Uniform(0.0)]),
        Some(Value::Array(items)) => items.iter().map(one).collect(),
        Some(v) => Some(vec![one(v)?]),
    }
}

fn set(ctx: &mut CanvasRenderingContext2D, prop: &str, value: &Value) -> Result<(), String> {
    let num = || number(value).ok_or_else(|| format!("{prop}: value is not a number"));
    let text = || {
        value
            .as_str()
            .ok_or_else(|| format!("{prop}: value is not a string"))
    };
    match prop {
        "fillStyle" => ctx.set_fill_style(text()?),
        "strokeStyle" => ctx.set_stroke_style(text()?),
        "lineWidth" => ctx.set_line_width(num()?),
        "lineCap" => ctx.set_line_cap(text()?),
        "lineJoin" => ctx.set_line_join(text()?),
        "miterLimit" => ctx.set_miter_limit(num()?),
        "lineDashOffset" => ctx.set_line_dash_offset(num()?),
        "globalAlpha" => ctx.set_global_alpha(num()?),
        "globalCompositeOperation" => ctx.set_global_composite_operation(text()?),
        "font" => ctx.set_font(text()?),
        "textAlign" => ctx.set_text_align(text()?),
        "textBaseline" => ctx.set_text_baseline(text()?),
        "direction" => ctx.set_direction(text()?),
        "letterSpacing" => ctx.set_letter_spacing(text()?),
        "wordSpacing" => ctx.set_word_spacing(text()?),
        "fontKerning" => ctx.set_font_kerning(text()?),
        "fontStretch" => ctx.set_font_stretch(text()?),
        "fontVariantCaps" => ctx.set_font_variant_caps(text()?),
        "textRendering" => ctx.set_text_rendering(text()?),
        "lang" => ctx.set_lang(text()?),
        other => return Err(format!("unsupported attribute {other}")),
    }
    Ok(())
}

fn get(ctx: &CanvasRenderingContext2D, prop: &str) -> Result<Value, String> {
    Ok(match prop {
        "fillStyle" => json!(ctx.fill_style()),
        "strokeStyle" => json!(ctx.stroke_style()),
        "lineWidth" => encode_number(ctx.line_width()),
        "lineCap" => json!(ctx.line_cap()),
        "lineJoin" => json!(ctx.line_join()),
        "miterLimit" => encode_number(ctx.miter_limit()),
        "lineDashOffset" => encode_number(ctx.line_dash_offset()),
        "globalAlpha" => encode_number(ctx.global_alpha()),
        "globalCompositeOperation" => json!(ctx.global_composite_operation()),
        "font" => json!(ctx.font()),
        "textAlign" => json!(ctx.text_align()),
        "textBaseline" => json!(ctx.text_baseline()),
        "direction" => json!(ctx.direction()),
        "letterSpacing" => json!(ctx.letter_spacing()),
        "wordSpacing" => json!(ctx.word_spacing()),
        "fontKerning" => json!(ctx.font_kerning()),
        "fontStretch" => json!(ctx.font_stretch()),
        "fontVariantCaps" => json!(ctx.font_variant_caps()),
        "textRendering" => json!(ctx.text_rendering()),
        "lang" => json!(ctx.lang()),
        other => return Err(format!("unsupported attribute {other}")),
    })
}
