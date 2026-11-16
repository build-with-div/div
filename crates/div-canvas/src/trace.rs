//! The canvas trace format (RFC 0008 §7 item 3).
//!
//! A trace is JSON lines. Each line names a canvas and does one thing:
//!
//! - `{"canvas": id, "width": w, "height": h}` creates a canvas;
//! - `{"canvas": id, "call": name, "args": [...]}` calls a method;
//! - `{"canvas": id, "set": prop, "value": v}` assigns an attribute;
//! - `{"canvas": id, "get": prop}` reads an attribute.
//!
//! Numbers that JSON cannot carry are strings: `"NaN"`, `"Infinity"`,
//! `"-Infinity"` and `"-0"`. Replaying a trace yields its observations (every
//! read, every return value and every exception, in order) and each canvas's
//! final bitmap. The Chromium capture (`tools/canvas/capture_canvas_traces.py`)
//! replays the same lines with JavaScript and writes the same observations,
//! so the two compare with `==`.

use std::collections::BTreeMap;

use serde_json::{json, Map, Number, Value};

use crate::{CanvasError, CanvasRenderingContext2D, FillRule, RoundRectRadius};

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

/// Replays `trace` (JSON lines; blank lines are skipped).
pub fn replay(trace: &str) -> Result<Replay, TraceError> {
    let mut canvases: BTreeMap<String, CanvasRenderingContext2D> = BTreeMap::new();
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
            let ctx = CanvasRenderingContext2D::new(w, h)
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
            match call(ctx, name, &args).map_err(err)? {
                CallResult::Void => {}
                CallResult::Return(value) => {
                    observations.push(json!({"canvas": id, "call": name, "return": value}))
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

enum CallResult {
    Void,
    Return(Value),
    Throw(CanvasError),
}

fn from_result(r: Result<(), CanvasError>) -> CallResult {
    match r {
        Ok(()) => CallResult::Void,
        Err(e) => CallResult::Throw(e),
    }
}

fn call(
    ctx: &mut CanvasRenderingContext2D,
    name: &str,
    args: &[Value],
) -> Result<CallResult, String> {
    let n = |i: usize| -> Result<f64, String> {
        args.get(i)
            .and_then(number)
            .ok_or_else(|| format!("{name}: argument {i} is not a number"))
    };
    let flag = |i: usize| -> bool { args.get(i).and_then(Value::as_bool).unwrap_or(false) };
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
        other => return Err(format!("unsupported attribute {other}")),
    })
}
