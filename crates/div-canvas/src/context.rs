//! `blink::Canvas2DRecorderContext` (`canvas_2d_recorder_context.{h,cc}`,
//! Chromium BSD) for core state, paths, rectangles, fill, stroke, clip and
//! transforms.
//!
//! Blink records into a `cc::PaintRecord` and plays it back onto a raster
//! `SkCanvas`. This crate draws onto the raster `SkCanvas` directly, issuing
//! the same Skia calls in the same order, so the pixels are those of the
//! playback. Where the playback is not a plain Skia call (`DrawArcOp`), it is
//! ported from `cc/paint/paint_op.cc` (Chromium BSD).

use div_style::Color;
use skia_safe::{
    canvas::SaveLayerRec, path_utils, AlphaType, BlendMode, Canvas, ClipOp, ColorType, IRect,
    ImageInfo, Matrix, Paint, PaintStyle, Path, PathBuilder, PathFillType, PixelGeometry, Rect,
    Surface, SurfaceProps, SurfacePropsFlags, M44,
};

use std::sync::Arc;

use div_geometry::{RasterConfiguration, TextHinting};
use div_text::{FontCollection, TextShaper};

use crate::affine::AffineTransform;
use crate::math::{clamp_non_finite_to_zero, clamp_to_f32, float_nearly_equal, PI_FLOAT};
use crate::path::{CanvasPath, RoundRectRadius};
use crate::shape_cache::ShapeCaches;
use crate::state::{composite_name, parse_composite, CanvasState, LineCap, LineJoin, PaintType};
use crate::style::{CanvasGradient, CanvasStyle, CanvasStyleValue};
use crate::CanvasError;

/// `CanvasFillRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "nonzero" => Some(Self::NonZero),
            "evenodd" => Some(Self::EvenOdd),
            _ => None,
        }
    }

    fn to_skia(self) -> PathFillType {
        match self {
            Self::NonZero => PathFillType::Winding,
            Self::EvenOdd => PathFillType::EvenOdd,
        }
    }
}

/// A `CanvasRenderingContext2D` over its own raster backing store.
pub struct CanvasRenderingContext2D {
    surface: Surface,
    width: i32,
    height: i32,
    states: Vec<CanvasState>,
    path: CanvasPath,
    /// The fonts text resolves against (a document's `FontFaceSet` and the
    /// system fonts).
    fonts: Arc<FontCollection>,
    shaper: TextShaper,
    shape_caches: ShapeCaches,
    raster: RasterConfiguration,
    /// The canvas's origin-clean flag (RFC 0008 §5). Every source is local
    /// until H1 lands, so nothing clears it yet.
    origin_clean: bool,
}

mod readback;
mod text_methods;

type Bounds = (f32, f32, f32, f32);

/// Chromium's text raster for the platform: its page text's font settings
/// (RFC 0012 on Windows) with the hinting Chromium asks for. Canvas text is
/// drawn on a surface with unknown pixel geometry, so Skia draws it
/// grayscale.
fn platform_raster_configuration() -> RasterConfiguration {
    if cfg!(target_os = "windows") {
        let mut configuration = RasterConfiguration::chromium_windows_lcd();
        // TEMPORARY: compensates for div-text. Windows Chromium asks for
        // medium hinting (`gfx::FontRenderParams`,
        // `font_render_params_win.cc`), which Skia's DirectWrite scaler
        // treats as it treats slight hinting. Asking for it keeps div-text's
        // per-family resolution of slight hinting, a model of Linux
        // Fontconfig profiles, away from canvas text on Windows.
        // Removed by the engine lane's fix: `resolve_hinting` stops applying
        // that model on Windows, or `chromium_windows_lcd` asks for medium
        // hinting itself. See "Temporary compensations" in
        // docs/progress/canvas-text-2026-11-21.md.
        configuration.author_text.hinting = TextHinting::Normal;
        configuration
    } else {
        RasterConfiguration::chromium_linux_lcd()
    }
}

impl CanvasRenderingContext2D {
    /// A transparent `width`×`height` canvas (N32, premultiplied, as Blink's
    /// software canvas resource). Returns `None` for a size Skia rejects.
    pub fn new(width: i32, height: i32) -> Option<Self> {
        Self::with_fonts(width, height, FontCollection::system())
    }

    /// [`Self::new`] with text resolved against `fonts`, which several
    /// canvases may share as a document's canvases share its fonts.
    pub fn with_fonts(width: i32, height: i32, fonts: Arc<FontCollection>) -> Option<Self> {
        // SkGraphics::Init before this Skia user exists (see div-text).
        div_text::initialize_skia();
        let raster = platform_raster_configuration();
        // `CanvasResourceProvider::GetSkSurfaceProps` for a canvas with alpha:
        // `LegacyDisplayGlobals::ComputeSurfaceProps(false)`, unknown pixel
        // geometry (no LCD text) with the display's text contrast and gamma.
        let props = SurfaceProps::new_with_text_properties(
            SurfacePropsFlags::default(),
            PixelGeometry::Unknown,
            raster.contrast(),
            raster.gamma(),
        );
        let info = ImageInfo::new_n32_premul((width, height), None);
        let surface = skia_safe::surfaces::raster(&info, None, Some(&props))?;
        Some(Self {
            surface,
            width,
            height,
            states: vec![CanvasState::default()],
            path: CanvasPath::new(),
            fonts,
            shaper: TextShaper::new(),
            shape_caches: ShapeCaches::new(),
            raster,
            origin_clean: true,
        })
    }

    /// The fonts this canvas's text resolves against.
    pub fn font_collection(&self) -> &Arc<FontCollection> {
        &self.fonts
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    fn state(&self) -> &CanvasState {
        self.states.last().expect("state stack is never empty")
    }

    fn state_mut(&mut self) -> &mut CanvasState {
        self.states.last_mut().expect("state stack is never empty")
    }

    fn canvas(&mut self) -> &Canvas {
        self.surface.canvas()
    }

    fn set_transform_state(&mut self, t: AffineTransform) {
        let invertible = t.is_invertible();
        let state = self.state_mut();
        state.transform = t;
        state.transform_invertible = invertible;
        self.path.transform = t;
        self.path.transform_invertible = invertible;
    }

    fn sync_path_transform(&mut self) {
        let (t, inv) = (self.state().transform, self.state().transform_invertible);
        self.path.transform = t;
        self.path.transform_invertible = inv;
    }

    // ---- State ---------------------------------------------------------

    pub fn save(&mut self) {
        let copy = self.state().clone();
        self.states.push(copy);
        self.canvas().save();
    }

    pub fn restore(&mut self) {
        if self.states.len() <= 1 {
            return;
        }
        self.canvas().restore();
        // PopStateStack.
        let current = self.state().clone();
        if current.transform_invertible && !current.transform.is_identity() {
            self.path.transform_path(&current.transform);
        }
        self.states.pop();
        self.sync_path_transform();
        let state = self.state().clone();
        if state.transform_invertible && !state.transform.is_identity() {
            self.path.transform_path(&state.transform.inverse());
        }
    }

    // ---- Transforms ----------------------------------------------------

    pub fn scale(&mut self, sx: f64, sy: f64) {
        if !sx.is_finite() || !sy.is_finite() {
            return;
        }
        let mut new_transform = self.state().transform;
        let fsx = clamp_to_f32(sx);
        let fsy = clamp_to_f32(sy);
        new_transform.scale_non_uniform(fsx as f64, fsy as f64);
        if self.state().transform == new_transform {
            return;
        }
        self.set_transform_state(new_transform);
        self.canvas().scale((fsx, fsy));
        if self.state().transform_invertible {
            let mut delta = AffineTransform::IDENTITY;
            delta.scale_non_uniform(1.0 / fsx as f64, 1.0 / fsy as f64);
            self.path.transform_path(&delta);
        }
    }

    pub fn rotate(&mut self, angle_in_radians: f64) {
        if !angle_in_radians.is_finite() {
            return;
        }
        let mut new_transform = self.state().transform;
        new_transform.rotate_radians(angle_in_radians);
        if self.state().transform == new_transform {
            return;
        }
        self.set_transform_state(new_transform);
        let degrees = clamp_to_f32(angle_in_radians * (180.0 / PI_FLOAT as f64));
        self.canvas().rotate(degrees, None);
        if self.state().transform_invertible {
            let mut delta = AffineTransform::IDENTITY;
            delta.rotate_radians(-angle_in_radians);
            self.path.transform_path(&delta);
        }
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        if !self.state().transform_invertible {
            return;
        }
        if !tx.is_finite() || !ty.is_finite() {
            return;
        }
        let mut new_transform = self.state().transform;
        let ftx = clamp_to_f32(tx);
        let fty = clamp_to_f32(ty);
        new_transform.translate(ftx as f64, fty as f64);
        if self.state().transform == new_transform {
            return;
        }
        self.set_transform_state(new_transform);
        self.canvas().translate((ftx, fty));
        if self.state().transform_invertible {
            let mut delta = AffineTransform::IDENTITY;
            delta.translate(-ftx as f64, -fty as f64);
            self.path.transform_path(&delta);
        }
    }

    pub fn transform(&mut self, m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64) {
        if ![m11, m12, m21, m22, dx, dy].iter().all(|v| v.is_finite()) {
            return;
        }
        let [a, b, c, d, e, f] = [m11, m12, m21, m22, dx, dy].map(|v| clamp_to_f32(v) as f64);
        let transform = AffineTransform::new(a, b, c, d, e, f);
        let new_transform = self.state().transform.multiplied(&transform);
        if self.state().transform == new_transform {
            return;
        }
        self.set_transform_state(new_transform);
        self.canvas().concat_44(&transform.to_sk_m44());
        if self.state().transform_invertible {
            self.path.transform_path(&transform.inverse());
        }
    }

    pub fn reset_transform(&mut self) {
        let ctm = self.state().transform;
        let invertible_ctm = self.state().transform_invertible;
        if ctm.is_identity() && invertible_ctm {
            return;
        }
        self.set_transform_state(AffineTransform::IDENTITY);
        self.canvas().set_matrix(&M44::new_identity());
        if invertible_ctm {
            self.path.transform_path(&ctm);
        }
    }

    pub fn set_transform(&mut self, m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64) {
        if ![m11, m12, m21, m22, dx, dy].iter().all(|v| v.is_finite()) {
            return;
        }
        self.reset_transform();
        self.transform(m11, m12, m21, m22, dx, dy);
    }

    /// `getTransform()` as `[a, b, c, d, e, f]`.
    pub fn get_transform(&self) -> [f64; 6] {
        self.state().transform.values()
    }

    // ---- Compositing and line styles -------------------------------------

    pub fn global_alpha(&self) -> f64 {
        self.state().global_alpha
    }

    pub fn set_global_alpha(&mut self, alpha: f64) {
        if !(0.0..=1.0).contains(&alpha) {
            return;
        }
        self.state_mut().global_alpha = alpha;
    }

    pub fn global_composite_operation(&self) -> &'static str {
        composite_name(self.state().composite)
    }

    pub fn set_global_composite_operation(&mut self, operation: &str) {
        if let Some(mode) = parse_composite(operation) {
            self.state_mut().composite = mode;
        }
    }

    pub fn line_width(&self) -> f64 {
        self.state().line_width as f64
    }

    pub fn set_line_width(&mut self, width: f64) {
        if !width.is_finite() || width <= 0.0 {
            return;
        }
        self.state_mut().line_width = clamp_to_f32(width);
    }

    pub fn line_cap(&self) -> &'static str {
        self.state().line_cap.name()
    }

    pub fn set_line_cap(&mut self, cap: &str) {
        if let Some(cap) = LineCap::parse(cap) {
            self.state_mut().line_cap = cap;
        }
    }

    pub fn line_join(&self) -> &'static str {
        self.state().line_join.name()
    }

    pub fn set_line_join(&mut self, join: &str) {
        if let Some(join) = LineJoin::parse(join) {
            self.state_mut().line_join = join;
        }
    }

    pub fn miter_limit(&self) -> f64 {
        self.state().miter_limit as f64
    }

    pub fn set_miter_limit(&mut self, limit: f64) {
        if !limit.is_finite() || limit <= 0.0 {
            return;
        }
        self.state_mut().miter_limit = clamp_to_f32(limit);
    }

    pub fn set_line_dash(&mut self, dash: &[f64]) {
        // LineDashSequenceIsValid: every value finite and non-negative.
        if !dash.iter().all(|d| d.is_finite() && *d >= 0.0) {
            return;
        }
        let mut line_dash = dash.to_vec();
        if dash.len() % 2 == 1 {
            line_dash.extend_from_slice(dash);
        }
        for d in &mut line_dash {
            *d = clamp_to_f32(*d) as f64;
        }
        self.state_mut().line_dash = line_dash;
    }

    pub fn get_line_dash(&self) -> Vec<f64> {
        self.state().line_dash.clone()
    }

    pub fn line_dash_offset(&self) -> f64 {
        self.state().line_dash_offset as f64
    }

    pub fn set_line_dash_offset(&mut self, offset: f64) {
        if !offset.is_finite() {
            return;
        }
        self.state_mut().line_dash_offset = clamp_to_f32(offset);
    }

    // ---- Colour styles -------------------------------------------------

    pub fn set_fill_style(&mut self, color: &str) {
        if let Some(c) = parse_canvas_color(color) {
            self.state_mut().fill_style = CanvasStyle::Color(c);
        }
    }

    pub fn set_stroke_style(&mut self, color: &str) {
        if let Some(c) = parse_canvas_color(color) {
            self.state_mut().stroke_style = CanvasStyle::Color(c);
        }
    }

    /// `fillStyle = gradient`.
    pub fn set_fill_gradient(&mut self, gradient: &CanvasGradient) {
        self.state_mut().fill_style = CanvasStyle::Gradient(gradient.clone());
    }

    /// `strokeStyle = gradient`.
    pub fn set_stroke_gradient(&mut self, gradient: &CanvasGradient) {
        self.state_mut().stroke_style = CanvasStyle::Gradient(gradient.clone());
    }

    pub fn fill_style(&self) -> CanvasStyleValue {
        self.state().fill_style.value()
    }

    pub fn stroke_style(&self) -> CanvasStyleValue {
        self.state().stroke_style.value()
    }

    // ---- Gradients -----------------------------------------------------

    /// `createLinearGradient`. `None` for a non-finite argument, which script
    /// cannot pass: the binding throws a `TypeError` first.
    pub fn create_linear_gradient(
        &self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> Option<CanvasGradient> {
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return None;
        }
        Some(CanvasGradient::linear(
            (clamp_to_f32(x0), clamp_to_f32(y0)),
            (clamp_to_f32(x1), clamp_to_f32(y1)),
        ))
    }

    /// `createRadialGradient`: an `IndexSizeError` for a negative radius,
    /// else as [`Self::create_linear_gradient`].
    pub fn create_radial_gradient(
        &self,
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
    ) -> Result<Option<CanvasGradient>, CanvasError> {
        if r0 < 0.0 || r1 < 0.0 {
            return Err(CanvasError::IndexSize);
        }
        if ![x0, y0, r0, x1, y1, r1].iter().all(|v| v.is_finite()) {
            return Ok(None);
        }
        Ok(Some(CanvasGradient::radial(
            (clamp_to_f32(x0), clamp_to_f32(y0)),
            clamp_to_f32(r0),
            (clamp_to_f32(x1), clamp_to_f32(y1)),
            clamp_to_f32(r1),
        )))
    }

    /// `createConicGradient`, as [`Self::create_linear_gradient`].
    pub fn create_conic_gradient(
        &self,
        start_angle: f64,
        center_x: f64,
        center_y: f64,
    ) -> Option<CanvasGradient> {
        if ![start_angle, center_x, center_y]
            .iter()
            .all(|v| v.is_finite())
        {
            return None;
        }
        // `Rad2deg(float)`, then a quarter turn so that an angle of 0 starts
        // on the x-axis.
        let degrees = clamp_to_f32(start_angle) * (180.0 / PI_FLOAT) + 90.0;
        Some(CanvasGradient::conic(
            degrees,
            (clamp_to_f32(center_x), clamp_to_f32(center_y)),
        ))
    }

    // ---- Paths ---------------------------------------------------------

    pub fn begin_path(&mut self) {
        self.path.clear();
    }

    pub fn path(&mut self) -> &mut CanvasPath {
        &mut self.path
    }

    pub fn close_path(&mut self) {
        self.path.close_path();
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.path.move_to(x, y);
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.path.line_to(x, y);
    }

    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        self.path.quadratic_curve_to(cpx, cpy, x, y);
    }

    pub fn bezier_curve_to(&mut self, a: f64, b: f64, c: f64, d: f64, x: f64, y: f64) {
        self.path.bezier_curve_to(a, b, c, d, x, y);
    }

    pub fn arc_to(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        r: f64,
    ) -> Result<(), CanvasError> {
        self.path.arc_to(x1, y1, x2, y2, r)
    }

    pub fn arc(
        &mut self,
        x: f64,
        y: f64,
        r: f64,
        start: f64,
        end: f64,
        anticlockwise: bool,
    ) -> Result<(), CanvasError> {
        self.path.arc(x, y, r, start, end, anticlockwise)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        &mut self,
        x: f64,
        y: f64,
        rx: f64,
        ry: f64,
        rotation: f64,
        start: f64,
        end: f64,
        anticlockwise: bool,
    ) -> Result<(), CanvasError> {
        self.path
            .ellipse(x, y, rx, ry, rotation, start, end, anticlockwise)
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.path.rect(x, y, w, h);
    }

    pub fn round_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        radii: &[RoundRectRadius],
    ) -> Result<(), CanvasError> {
        self.path.round_rect(x, y, w, h, radii)
    }

    // ---- Drawing -------------------------------------------------------

    /// `Draw` → `DrawInternal` without shadows or filters.
    fn draw(&mut self, bounds: Bounds, paint_type: PaintType, draw: impl Fn(&Canvas, &Paint)) {
        if !self.state().transform_invertible {
            return;
        }
        let Some(clip_bounds) = self.canvas().device_clip_bounds() else {
            return;
        };
        let state = self.state().clone();
        let composite = state.composite;
        if is_full_canvas_composite_mode(composite) {
            // CompositedDraw: a layer with the composite mode, drawn into in
            // source-over.
            let mut foreground = state.paint(paint_type);
            foreground.set_blend_mode(BlendMode::SrcOver);
            let mut composite_paint = Paint::default();
            composite_paint.set_blend_mode(composite);
            let canvas = self.canvas();
            let ctm = canvas.local_to_device();
            canvas.set_matrix(&M44::new_identity());
            canvas.save_layer(&SaveLayerRec::default().paint(&composite_paint));
            canvas.set_matrix(&ctm);
            draw(canvas, &foreground);
            canvas.restore();
            canvas.set_matrix(&ctm);
        } else if composite == BlendMode::Src {
            let paint = state.paint(paint_type);
            let canvas = self.canvas();
            canvas.clear(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.0));
            draw(canvas, &paint);
        } else {
            if compute_dirty_rect(&state.transform, bounds, clip_bounds).is_none() {
                return;
            }
            let paint = state.paint(paint_type);
            draw(self.canvas(), &paint);
        }
    }

    /// `InflateStrokeRect`.
    fn inflate_stroke_rect(&self, rect: Bounds) -> Bounds {
        let state = self.state();
        let root2 = 2.0f32.sqrt() as f64;
        let mut delta = state.line_width as f64 / 2.0;
        if state.line_join == LineJoin::Miter {
            delta *= state.miter_limit as f64;
        } else if state.line_cap == LineCap::Square {
            delta *= root2;
        }
        let d = clamp_to_f32(delta);
        (rect.0 - d, rect.1 - d, rect.2 + 2.0 * d, rect.3 + 2.0 * d)
    }

    fn draw_path_internal(&mut self, paint_type: PaintType, fill_rule: FillRule) {
        if self.path.is_empty() {
            return;
        }
        let mut bounds = self.path.bounding_rect();
        if bounds.0.is_nan() || bounds.1.is_nan() || bounds.2.is_nan() || bounds.3.is_nan() {
            return;
        }
        if paint_type == PaintType::Stroke {
            bounds = self.inflate_stroke_rect(bounds);
        }
        if self.path.is_line() {
            if paint_type == PaintType::Fill {
                return;
            }
            let line = self.path.line_data();
            self.draw(bounds, paint_type, |c, p| {
                c.draw_line(line.start, line.end, p);
            });
            return;
        }
        if self.path.is_arc() {
            let arc = self.path.arc_data();
            let oval = Rect::new(
                arc.x - arc.radius,
                arc.y - arc.radius,
                arc.x + arc.radius,
                arc.y + arc.radius,
            );
            let start_degrees =
                clamp_non_finite_to_zero(arc.start_angle_radians * 180.0 / PI_FLOAT);
            let sweep_degrees =
                clamp_non_finite_to_zero(arc.sweep_angle_radians * 180.0 / PI_FLOAT);
            let closed = arc.closed;
            self.draw(bounds, paint_type, |c, p| {
                // cc's DrawArcImpl (cc/paint/paint_op.cc), the playback of
                // Blink's DrawArcOp: a closed arc is an oval when the sweep is
                // a full turn, else the arc path closed.
                if closed {
                    if float_nearly_equal(sweep_degrees.abs(), 360.0) {
                        c.draw_oval(oval, p);
                    } else {
                        let mut b = PathBuilder::new();
                        b.arc_to(oval, start_degrees, sweep_degrees, false);
                        b.close();
                        c.draw_path(&b.detach(), p);
                    }
                } else {
                    c.draw_arc(oval, start_degrees, sweep_degrees, false, p);
                }
            });
            return;
        }
        let mut sk_path = self.path.sk_path();
        sk_path.set_fill_type(fill_rule.to_skia());
        self.draw(bounds, paint_type, |c, p| {
            c.draw_path(&sk_path, p);
        });
    }

    pub fn fill(&mut self, fill_rule: FillRule) {
        self.draw_path_internal(PaintType::Fill, fill_rule);
    }

    pub fn stroke(&mut self) {
        self.draw_path_internal(PaintType::Stroke, FillRule::NonZero);
    }

    pub fn clip(&mut self, fill_rule: FillRule) {
        if !self.state().transform_invertible {
            return;
        }
        let mut sk_path = self.path.sk_path();
        sk_path.set_fill_type(fill_rule.to_skia());
        let is_rect = sk_path.is_rect().is_some();
        {
            let state = self.state_mut();
            state.has_clip = true;
            if !is_rect {
                state.has_complex_clip = true;
            }
        }
        self.canvas().clip_path(&sk_path, ClipOp::Intersect, true);
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        if !validate_rect(x, y, width, height) {
            return;
        }
        let (x, y, width, height) = adjust_rect(x, y, width, height);
        let rect = (
            clamp_to_f32(x),
            clamp_to_f32(y),
            clamp_to_f32(width),
            clamp_to_f32(height),
        );
        let sk = Rect::from_xywh(rect.0, rect.1, rect.2, rect.3);
        self.draw(rect, PaintType::Fill, |c, p| {
            c.draw_rect(sk, p);
        });
    }

    pub fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        if !validate_rect(x, y, width, height) {
            return;
        }
        let (x, y, width, height) = adjust_rect(x, y, width, height);
        let rect = (
            clamp_to_f32(x),
            clamp_to_f32(y),
            clamp_to_f32(width),
            clamp_to_f32(height),
        );
        let bounds = self.inflate_stroke_rect(rect);
        if !validate_rect(
            bounds.0 as f64,
            bounds.1 as f64,
            bounds.2 as f64,
            bounds.3 as f64,
        ) {
            return;
        }
        self.draw(bounds, PaintType::Stroke, |c, p| {
            // StrokeRectOnCanvas.
            if (rect.2 > 0.0) != (rect.3 > 0.0) {
                let mut b = PathBuilder::new();
                b.move_to((rect.0, rect.1));
                b.line_to((rect.0 + rect.2, rect.1 + rect.3));
                b.close();
                c.draw_path(&b.detach(), p);
                return;
            }
            c.draw_rect(Rect::from_xywh(rect.0, rect.1, rect.2, rect.3), p);
        });
    }

    pub fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        if !validate_rect(x, y, width, height) {
            return;
        }
        if !self.state().transform_invertible {
            return;
        }
        let Some(clip_bounds) = self.canvas().device_clip_bounds() else {
            return;
        };
        let (x, y, width, height) = adjust_rect(x, y, width, height);
        let rect = (
            clamp_to_f32(x),
            clamp_to_f32(y),
            clamp_to_f32(width),
            clamp_to_f32(height),
        );
        // Whether the rect covers the clip (drawn) or not (drawn when its
        // dirty rect meets the clip), the draw is the same; it is skipped only
        // when the dirty rect misses the clip.
        let transform = self.state().transform;
        if !rect_contains_transformed_rect(&transform, rect, clip_bounds)
            && compute_dirty_rect(&transform, rect, clip_bounds).is_none()
        {
            return;
        }
        // GetClearFlags with alpha: kClear, fill, cc::PaintFlags defaults
        // (no antialiasing).
        let mut paint = Paint::default();
        paint.set_style(PaintStyle::Fill);
        paint.set_blend_mode(BlendMode::Clear);
        let sk = Rect::from_xywh(rect.0, rect.1, rect.2, rect.3);
        self.canvas().draw_rect(sk, &paint);
    }

    // ---- Hit testing ---------------------------------------------------

    pub fn is_point_in_path(&mut self, x: f64, y: f64, fill_rule: FillRule) -> bool {
        if !self.state().transform_invertible || !x.is_finite() || !y.is_finite() {
            return false;
        }
        let (px, py) = self
            .state()
            .transform
            .inverse()
            .map_point(clamp_to_f32(x), clamp_to_f32(y));
        if !px.is_finite() || !py.is_finite() {
            return false;
        }
        let mut path = self.path.sk_path();
        path.set_fill_type(fill_rule.to_skia());
        path.contains((px, py))
    }

    pub fn is_point_in_stroke(&mut self, x: f64, y: f64) -> bool {
        if !self.state().transform_invertible || !x.is_finite() || !y.is_finite() {
            return false;
        }
        let ctm = self.state().transform;
        let (px, py) = ctm.inverse().map_point(clamp_to_f32(x), clamp_to_f32(y));
        if !px.is_finite() || !py.is_finite() {
            return false;
        }
        let state = self.state().clone();
        // StrokeData::SetupPaint; StrokeData::SetLineDash repeats an odd list
        // (already doubled here) and keeps an all-zero list.
        let mut paint = Paint::default();
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(state.line_width);
        paint.set_stroke_cap(match state.line_cap {
            LineCap::Butt => skia_safe::PaintCap::Butt,
            LineCap::Round => skia_safe::PaintCap::Round,
            LineCap::Square => skia_safe::PaintCap::Square,
        });
        paint.set_stroke_join(match state.line_join {
            LineJoin::Miter => skia_safe::PaintJoin::Miter,
            LineJoin::Round => skia_safe::PaintJoin::Round,
            LineJoin::Bevel => skia_safe::PaintJoin::Bevel,
        });
        paint.set_stroke_miter(state.miter_limit);
        if !state.line_dash.is_empty() {
            let intervals: Vec<f32> = state.line_dash.iter().map(|&d| d as f32).collect();
            paint.set_path_effect(skia_safe::dash_path_effect::new(
                &intervals,
                state.line_dash_offset,
            ));
        }
        let precision = clamp_to_f32(ctm.x_scale_squared().max(ctm.y_scale_squared()).sqrt());
        let path = self.path.sk_path();
        let mut dst = PathBuilder::new();
        path_utils::fill_path_with_paint(
            &path,
            &paint,
            &mut dst,
            None,
            Matrix::scale((precision, precision)),
        );
        let stroke: Path = dst.detach();
        stroke.contains((px, py))
    }

    // ---- Readback ------------------------------------------------------

    /// The whole bitmap as unpremultiplied RGBA8888, as `getImageData(0, 0,
    /// width, height)` returns it.
    pub fn image_data(&mut self) -> Vec<u8> {
        let info = ImageInfo::new(
            (self.width, self.height),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let row_bytes = self.width as usize * 4;
        let mut pixels = vec![0u8; row_bytes * self.height as usize];
        let ok = self
            .surface
            .read_pixels(&info, &mut pixels, row_bytes, (0, 0));
        assert!(ok, "reading back a raster canvas cannot fail");
        pixels
    }
}

fn is_full_canvas_composite_mode(mode: BlendMode) -> bool {
    matches!(
        mode,
        BlendMode::SrcIn | BlendMode::SrcOut | BlendMode::DstIn | BlendMode::DstATop
    )
}

/// `ValidateRectForCanvas`.
fn validate_rect(x: f64, y: f64, width: f64, height: f64) -> bool {
    x.is_finite()
        && y.is_finite()
        && width.is_finite()
        && height.is_finite()
        && (width != 0.0 || height != 0.0)
}

/// `AdjustRectForCanvas`.
fn adjust_rect(mut x: f64, mut y: f64, mut width: f64, mut height: f64) -> (f64, f64, f64, f64) {
    if width < 0.0 {
        width = -width;
        x -= width;
    }
    if height < 0.0 {
        height = -height;
        y -= height;
    }
    (x, y, width, height)
}

/// `SkRect::roundOut` with `sk_float_floor2int`/`sk_float_ceil2int`
/// saturation.
fn round_out(l: f32, t: f32, r: f32, b: f32) -> IRect {
    let sat = |v: f32| -> i32 {
        if v.is_nan() {
            0
        } else {
            v.clamp(i32::MIN as f32, i32::MAX as f32) as i32
        }
    };
    IRect::new(sat(l.floor()), sat(t.floor()), sat(r.ceil()), sat(b.ceil()))
}

/// `ComputeDirtyRect` without a shadow: the transformed bounds, rounded out,
/// intersected with the device clip bounds.
fn compute_dirty_rect(transform: &AffineTransform, local: Bounds, clip: IRect) -> Option<IRect> {
    let (x, y, w, h) = transform.map_rect(local);
    let dirty = round_out(x, y, x + w, y + h);
    IRect::intersect(&dirty, &clip)
}

/// `RectContainsTransformedRect`: the mapped quad contains the clip rect.
fn rect_contains_transformed_rect(transform: &AffineTransform, rect: Bounds, clip: IRect) -> bool {
    let (x, y, w, h) = rect;
    let quad = [
        transform.map_point(x, y),
        transform.map_point(x + w, y),
        transform.map_point(x + w, y + h),
        transform.map_point(x, y + h),
    ];
    let clip_points = [
        (clip.left as f32, clip.top as f32),
        (clip.right as f32, clip.top as f32),
        (clip.right as f32, clip.bottom as f32),
        (clip.left as f32, clip.bottom as f32),
    ];
    clip_points.iter().all(|&p| quad_contains(&quad, p))
}

/// `gfx::QuadF::Contains` for a convex quad, inclusive of its edges.
fn quad_contains(q: &[(f32, f32); 4], p: (f32, f32)) -> bool {
    let cross = |a: (f32, f32), b: (f32, f32)| -> f32 {
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    };
    let mut pos = false;
    let mut neg = false;
    for i in 0..4 {
        let c = cross(q[i], q[(i + 1) % 4]);
        pos |= c > 0.0;
        neg |= c < 0.0;
    }
    !(pos && neg)
}

/// `ParseCanvasColorString` for a canvas without a host element: HTML
/// whitespace stripped, `currentcolor` resolving to black.
pub(crate) fn parse_canvas_color(input: &str) -> Option<Color> {
    let trimmed = input.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\x0c' | '\r'));
    if trimmed.eq_ignore_ascii_case("currentcolor") {
        return Some(Color::BLACK);
    }
    div_style::parse_css_color(trimmed)
}

/// `Color::SerializeAsCanvasColor` for the legacy sRGB colours this crate
/// stores: `#rrggbb` when opaque, else `rgba(r, g, b, a)`.
pub(crate) fn serialize_canvas_color(color: Color) -> String {
    let channel = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let (r, g, b) = (channel(color.r), channel(color.g), channel(color.b));
    if color.a >= 1.0 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {})", format_alpha(color.a))
    }
}

/// CSS Color 4 alpha serialization for a legacy colour: the 8-bit alpha
/// printed with two decimals when they round-trip to the same byte, else
/// three, trailing zeros removed.
fn format_alpha(a: f32) -> String {
    let byte = (a * 255.0).round();
    let two = (byte / 255.0 * 100.0).round() / 100.0;
    let value = if (two * 255.0).round() == byte {
        two
    } else {
        (byte / 255.0 * 1000.0).round() / 1000.0
    };
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}
