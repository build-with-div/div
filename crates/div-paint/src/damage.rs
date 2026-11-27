//! Paint chunks and the damage between two recordings (RFC 0006 A1).
//!
//! Chromium's `PaintController` keeps paint as chunks, and
//! `RasterInvalidator::Generate` damages the old and the new rect of every
//! chunk that changed. Div records one picture, so this module observes that
//! recording through the canvas facade (`paint_record.rs`) and groups its
//! draws into chunks: a maximal run of draws with the same owner (the
//! fragment the painter is in, and its phase). It does not change what is
//! recorded.
//!
//! Each draw has bounds in the recording's coordinates, which hold every
//! pixel it can change, and a fingerprint of everything that decides those
//! pixels: its operands, its paint, the matrix and the clips and layers
//! around it. A chunk folds its draws' fingerprints and unites their bounds.
//! [`PaintChunks::damage_since`] compares two recordings:
//!
//! - a chunk that is new, gone, or whose fingerprint or bounds changed
//!   damages its old and its new bounds;
//! - unchanged chunks whose paint order changed damage their bounds (those
//!   outside the longest run that kept its order);
//! - anything this module cannot vouch for damages the whole viewport: a
//!   draw that escaped the facade, an unbalanced save stack, or a recording
//!   without observed chunks.
//!
//! A pixel changes only if some draw that can touch it changed, appeared,
//! vanished or moved in paint order, so the damage covers every changed
//! pixel. The damage-coverage test checks this on full frames.
//!
//! Two kinds of draw spread beyond their geometry, and both are bounded by
//! the clip instead: draws inside a layer with an image filter or a blend
//! mode that changes the backdrop under transparent pixels (bounded by the
//! clip at the layer's save), and draws whose paint has a mask filter, a
//! path effect or an image filter that cannot bound itself.
//!
//! A layer holding exactly one draw can be folded into that draw, by the
//! recorder (cc's `SaveLayerAlphaOp` folding) or by SkRecord's optimizer, and
//! a folded draw rounds differently from one composited through a layer. So
//! the sequence of operations in a one-draw layer is part of that draw's
//! fingerprint, and a layer that gains or loses a draw damages the draw it
//! held.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use div_dom::NodeId;
use skia_safe::wrapper::PointerWrapper;
use skia_safe::{
    BlendMode, Blender, Canvas, ClipOp, ColorFilter, ImageFilter, MaskFilter, Matrix, Paint,
    PaintStyle, PathEffect, Rect, Shader,
};

use crate::paint_program::{OwnedDraw, PaintOwnerPhase, ProgramOperation};

/// Who painted a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ChunkOwner {
    /// The fragment's node, or `None` for draws outside every fragment
    /// (the canvas background).
    pub(crate) node: Option<NodeId>,
    /// Whether the draws come from the fragment itself or its contents.
    pub(crate) contents: bool,
    /// Whether the fragment owns its element's box.
    pub(crate) owns_element_box: bool,
    /// Whether the draws belong to the viewport overlay (scrollbars).
    pub(crate) overlay: bool,
}

/// One chunk of a recording.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaintChunk {
    pub(crate) owner: ChunkOwner,
    /// How many chunks with the same owner come before it.
    pub(crate) occurrence: u32,
    /// Every pixel the chunk can change, in the recording's coordinates.
    pub(crate) bounds: Rect,
    pub(crate) fingerprint: u64,
}

/// A recording's chunks, in paint order.
#[derive(Clone, Default)]
pub(crate) struct PaintChunks {
    chunks: Vec<PaintChunk>,
    /// Whether every draw went through the observed facade.
    complete: bool,
    /// Effects whose identity is in a fingerprint stay alive while the
    /// chunks do, so no later effect can reuse their address.
    _retained: Vec<Retained>,
}

impl std::fmt::Debug for PaintChunks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaintChunks")
            .field("chunks", &self.chunks)
            .field("complete", &self.complete)
            .finish()
    }
}

#[derive(Clone)]
#[allow(dead_code)]
enum Retained {
    Shader(Shader),
    ColorFilter(ColorFilter),
    ImageFilter(ImageFilter),
    MaskFilter(MaskFilter),
    PathEffect(PathEffect),
    Blender(Blender),
}

impl PaintChunks {
    pub(crate) fn chunks(&self) -> &[PaintChunk] {
        &self.chunks
    }

    fn append(&mut self, other: PaintChunks) {
        self.complete &= other.complete;
        self.chunks.extend(other.chunks);
        self.retained_mut().extend(other._retained);
    }

    fn retained_mut(&mut self) -> &mut Vec<Retained> {
        &mut self._retained
    }

    /// The damage from `previous` to `self`, in the recording's
    /// coordinates, or `None` when only the whole viewport is safe.
    pub(crate) fn damage_since(&self, previous: &PaintChunks) -> Option<Vec<Rect>> {
        if !self.complete || !previous.complete {
            return None;
        }
        let old: HashMap<(ChunkOwner, u32), usize> = previous
            .chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| ((chunk.owner, chunk.occurrence), index))
            .collect();
        let mut damage = Vec::new();
        let mut matched = vec![false; previous.chunks.len()];
        // Old indices of unchanged chunks, in new paint order.
        let mut kept: Vec<(usize, usize)> = Vec::new();
        for (new_index, chunk) in self.chunks.iter().enumerate() {
            match old.get(&(chunk.owner, chunk.occurrence)) {
                Some(&old_index) => {
                    matched[old_index] = true;
                    let before = &previous.chunks[old_index];
                    if before.fingerprint == chunk.fingerprint && before.bounds == chunk.bounds {
                        kept.push((old_index, new_index));
                    } else {
                        damage.push(before.bounds);
                        damage.push(chunk.bounds);
                    }
                }
                None => damage.push(chunk.bounds),
            }
        }
        for (index, chunk) in previous.chunks.iter().enumerate() {
            if !matched[index] {
                damage.push(chunk.bounds);
            }
        }
        let in_order = longest_increasing(&kept);
        for (position, &(_, new_index)) in kept.iter().enumerate() {
            if !in_order[position] {
                damage.push(self.chunks[new_index].bounds);
            }
        }
        damage.retain(|rect| !rect.is_empty());
        Some(damage)
    }
}

/// Marks the members of one longest subsequence of `kept` whose old
/// indices increase (patience sorting).
fn longest_increasing(kept: &[(usize, usize)]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![usize::MAX; kept.len()];
    for (position, &(old_index, _)) in kept.iter().enumerate() {
        let slot = tails.partition_point(|&tail| kept[tail].0 < old_index);
        if slot > 0 {
            previous[position] = tails[slot - 1];
        }
        if slot == tails.len() {
            tails.push(position);
        } else {
            tails[slot] = position;
        }
    }
    let mut member = vec![false; kept.len()];
    let mut at = tails.last().copied();
    while let Some(position) = at {
        member[position] = true;
        at = (previous[position] != usize::MAX).then_some(previous[position]);
    }
    member
}

/// The clip, layer and matrix state at one save depth.
#[derive(Clone, Copy)]
struct Level {
    state: u64,
    /// Bounds for every draw inside a layer whose output spreads past its
    /// draws' geometry.
    spread: Option<Rect>,
    /// Set on the level a layer's save pushed.
    layer: Option<LayerShape>,
}

/// What decides whether a layer can fold into its one draw.
#[derive(Clone, Copy)]
struct LayerShape {
    /// Draws inside the layer, at any depth, including clipped-out ones:
    /// they are still recorded, so they still prevent folding.
    draws: u32,
    /// A hash of the kinds of operation inside the layer, in order.
    operations: u64,
    /// The first chunk that can hold a draw of this layer.
    first_chunk: usize,
}

struct Observer {
    canvas: usize,
    base_save_count: usize,
    /// For a nested recording, its canvas's initial clip. Skia records
    /// draws outside a recording's bounds, and they replay wherever the
    /// picture is drawn, so that clip bounds nothing.
    base_clip: Option<Rect>,
    overlay: bool,
    levels: Vec<Level>,
    owners: Vec<(NodeId, bool, bool)>,
    occurrences: HashMap<ChunkOwner, u32>,
    open: Option<PaintChunk>,
    chunks: PaintChunks,
}

thread_local! {
    static OBSERVERS: RefCell<Vec<Observer>> = const { RefCell::new(Vec::new()) };
    /// The chunks of nested recordings, by picture id, until the next
    /// recording starts.
    static PICTURES: RefCell<HashMap<u32, Arc<PaintChunks>>> = RefCell::new(HashMap::new());
}

/// A draw's bounds when nothing bounds it.
const UNBOUNDED: Rect = Rect {
    left: -1.0e9,
    top: -1.0e9,
    right: 1.0e9,
    bottom: 1.0e9,
};

/// Registers the chunks of a nested recording under its picture.
pub(crate) fn register_picture(picture: &skia_safe::Picture, chunks: PaintChunks) {
    PICTURES.with(|pictures| {
        pictures
            .borrow_mut()
            .insert(picture.unique_id(), Arc::new(chunks))
    });
}

/// Forgets every registered picture; called when a recording starts.
pub(crate) fn clear_registered_pictures() {
    PICTURES.with(|pictures| pictures.borrow_mut().clear());
}

fn registered_picture(picture: &skia_safe::Picture) -> Option<Arc<PaintChunks>> {
    PICTURES.with(|pictures| pictures.borrow().get(&picture.unique_id()).cloned())
}

fn with_observer(canvas: &Canvas, action: impl FnOnce(&mut Observer)) {
    OBSERVERS.with(|observers| {
        if let Some(observer) = observers
            .borrow_mut()
            .iter_mut()
            .rev()
            .find(|o| o.canvas == canvas as *const Canvas as usize)
        {
            action(observer);
        }
    });
}

pub(crate) fn is_observing(canvas: &Canvas) -> bool {
    OBSERVERS.with(|observers| {
        observers
            .borrow()
            .iter()
            .any(|o| o.canvas == canvas as *const Canvas as usize)
    })
}

/// Observes one recording canvas for as long as it lives.
pub(crate) struct ChunkScope<'a> {
    canvas: &'a Canvas,
}

impl<'a> ChunkScope<'a> {
    pub(crate) fn new(canvas: &'a Canvas, overlay: bool) -> Self {
        OBSERVERS.with(|observers| {
            observers.borrow_mut().push(Observer {
                canvas: canvas as *const Canvas as usize,
                base_save_count: canvas.save_count(),
                base_clip: None,
                overlay,
                levels: vec![Level {
                    layer: None,
                    state: 0,
                    spread: None,
                }],
                owners: Vec::new(),
                occurrences: HashMap::new(),
                open: None,
                chunks: PaintChunks {
                    chunks: Vec::new(),
                    complete: true,
                    _retained: Vec::new(),
                },
            })
        });
        Self { canvas }
    }

    /// Observes `nested`, a recording whose picture `outer` will draw,
    /// when `outer` is observed. Its draws keep `outer`'s current owner
    /// until they enter their own.
    pub(crate) fn nested(outer: &Canvas, nested: &'a Canvas) -> Option<Self> {
        let (overlay, owners) = OBSERVERS.with(|observers| {
            observers
                .borrow()
                .iter()
                .rev()
                .find(|o| o.canvas == outer as *const Canvas as usize)
                .map(|o| (o.overlay, o.owners.clone()))
        })?;
        let scope = Self::new(nested, overlay);
        with_observer(nested, |observer| {
            observer.owners = owners;
            observer.base_clip = clip_rect(nested);
        });
        Some(scope)
    }

    /// The chunks observed so far.
    pub(crate) fn finish(self) -> PaintChunks {
        let mut result = PaintChunks::default();
        with_observer(self.canvas, |observer| {
            observer.close();
            if observer.canvas_depth(self.canvas) != Some(0) {
                observer.chunks.complete = false;
            }
            result = std::mem::take(&mut observer.chunks);
        });
        result
    }
}

impl Drop for ChunkScope<'_> {
    fn drop(&mut self) {
        OBSERVERS.with(|observers| {
            let mut observers = observers.borrow_mut();
            if let Some(index) = observers
                .iter()
                .rposition(|o| o.canvas == self.canvas as *const Canvas as usize)
            {
                observers.remove(index);
            }
        });
    }
}

/// Appends the overlay recording's chunks after the content's.
pub(crate) fn concatenate(mut content: PaintChunks, overlay: Option<PaintChunks>) -> PaintChunks {
    if let Some(overlay) = overlay {
        content.append(overlay);
    }
    content
}

impl Observer {
    fn canvas_depth(&self, canvas: &Canvas) -> Option<usize> {
        canvas.save_count().checked_sub(self.base_save_count)
    }

    fn top(&self) -> Level {
        *self.levels.last().expect("base level")
    }

    fn new_layer(&self) -> Option<LayerShape> {
        Some(LayerShape {
            draws: 0,
            operations: 0,
            first_chunk: self.chunks.chunks.len(),
        })
    }

    /// Adds one operation to the shape of every open layer.
    fn note_in_layers(&mut self, operation: impl Hash + Copy) {
        for level in &mut self.levels {
            if let Some(layer) = &mut level.layer {
                let mut h = DefaultHasher::new();
                layer.operations.hash(&mut h);
                operation.hash(&mut h);
                layer.operations = h.finish();
            }
        }
    }

    fn note_draw(&mut self) {
        self.note_in_layers(u8::MAX);
        for level in &mut self.levels {
            if let Some(layer) = &mut level.layer {
                layer.draws = layer.draws.saturating_add(1);
            }
        }
    }

    /// Pops one save level. A closing layer with one draw folds its shape
    /// into the chunks that can hold that draw.
    fn pop_level(&mut self) {
        let Some(level) = self.levels.pop() else {
            return;
        };
        self.note_in_layers(u8::MAX - 1);
        let Some(layer) = level.layer.filter(|layer| layer.draws == 1) else {
            return;
        };
        let first = layer.first_chunk.min(self.chunks.chunks.len());
        for chunk in self.chunks.chunks[first..]
            .iter_mut()
            .chain(self.open.as_mut())
        {
            let mut h = DefaultHasher::new();
            chunk.fingerprint.hash(&mut h);
            16u8.hash(&mut h);
            layer.operations.hash(&mut h);
            chunk.fingerprint = h.finish();
        }
    }

    fn close(&mut self) {
        if let Some(chunk) = self.open.take() {
            self.chunks.chunks.push(chunk);
        }
    }

    fn owner(&self) -> ChunkOwner {
        match self.owners.last() {
            Some(&(node, contents, owns_element_box)) => ChunkOwner {
                node: Some(node),
                contents,
                owns_element_box,
                overlay: self.overlay,
            },
            None => ChunkOwner {
                node: None,
                contents: false,
                owns_element_box: false,
                overlay: self.overlay,
            },
        }
    }

    fn retain_paint(&mut self, paint: &Paint, h: &mut DefaultHasher) {
        hash_paint(paint, h);
        let retained = self.chunks.retained_mut();
        if let Some(shader) = paint.shader() {
            retained.push(Retained::Shader(shader));
        }
        if let Some(filter) = paint.color_filter() {
            retained.push(Retained::ColorFilter(filter));
        }
        if let Some(filter) = paint.image_filter() {
            retained.push(Retained::ImageFilter(filter));
        }
        if let Some(filter) = paint.mask_filter() {
            retained.push(Retained::MaskFilter(filter));
        }
        if let Some(effect) = paint.path_effect() {
            retained.push(Retained::PathEffect(effect));
        }
        if let Some(blender) = paint.blender() {
            retained.push(Retained::Blender(blender));
        }
    }
}

impl Observer {
    /// Adds a registered nested recording's chunks, mapped to this canvas,
    /// in place of the one picture draw, so each keeps its owner and only
    /// changed ones damage.
    fn splice(
        &mut self,
        nested: &PaintChunks,
        matrix: Matrix,
        picture_matrix: Option<Matrix>,
        paint: Option<&Paint>,
        clip: Rect,
    ) {
        let top = self.top();
        let matrix = match picture_matrix {
            Some(picture_matrix) => Matrix::concat(&matrix, &picture_matrix),
            None => matrix,
        };
        self.chunks.complete &= nested.complete;
        self.chunks
            .retained_mut()
            .extend(nested._retained.iter().cloned());
        self.close();
        for chunk in nested.chunks() {
            let bounds = if matrix.has_perspective() {
                clip
            } else {
                let mut mapped = matrix.map_rect(chunk.bounds).0.with_outset((1.0, 1.0));
                if !mapped.intersect(clip) {
                    continue;
                }
                mapped
            };
            let mut h = DefaultHasher::new();
            15u8.hash(&mut h);
            top.state.hash(&mut h);
            hash_matrix(&matrix, &mut h);
            hash_rect(&clip, &mut h);
            if let Some(paint) = paint {
                hash_paint(paint, &mut h);
            }
            chunk.fingerprint.hash(&mut h);
            hash_rect(&chunk.bounds, &mut h);
            let owner = ChunkOwner {
                overlay: self.overlay,
                ..chunk.owner
            };
            let occurrence = self.occurrences.entry(owner).or_insert(0);
            self.chunks.chunks.push(PaintChunk {
                owner,
                occurrence: *occurrence,
                bounds,
                fingerprint: h.finish(),
            });
            *occurrence += 1;
        }
    }
}

/// Whether a picture drawn with `paint` changes only pixels its own draws
/// cover: plain source-over with at most a uniform alpha.
fn splices_through(paint: &Paint) -> bool {
    paint.shader().is_none()
        && paint.color_filter().is_none()
        && paint.image_filter().is_none()
        && paint.mask_filter().is_none()
        && paint.path_effect().is_none()
        && paint.blender().is_none()
        && paint.as_blend_mode() == Some(BlendMode::SrcOver)
}

fn identity<N, T: PointerWrapper<N>>(handle: &T) -> usize {
    handle.inner() as *const N as usize
}

fn hash_matrix(matrix: &Matrix, h: &mut impl Hasher) {
    for index in 0..9 {
        matrix[index].to_bits().hash(h);
    }
}

fn hash_rect(rect: &Rect, h: &mut impl Hasher) {
    for v in [rect.left, rect.top, rect.right, rect.bottom] {
        v.to_bits().hash(h);
    }
}

fn hash_paint(paint: &Paint, h: &mut impl Hasher) {
    let color = paint.color4f();
    for v in [color.r, color.g, color.b, color.a] {
        v.to_bits().hash(h);
    }
    paint.as_blend_mode().map(|mode| mode as i32).hash(h);
    (paint.style() as i32).hash(h);
    paint.stroke_width().to_bits().hash(h);
    paint.stroke_miter().to_bits().hash(h);
    (paint.stroke_cap() as i32).hash(h);
    (paint.stroke_join() as i32).hash(h);
    paint.is_anti_alias().hash(h);
    paint.is_dither().hash(h);
    paint.shader().map(|s| identity(&s)).hash(h);
    paint.color_filter().map(|s| identity(&s)).hash(h);
    paint.image_filter().map(|s| identity(&s)).hash(h);
    paint.mask_filter().map(|s| identity(&s)).hash(h);
    paint.path_effect().map(|s| identity(&s)).hash(h);
    paint.blender().map(|s| identity(&s)).hash(h);
}

/// Whether compositing a layer with `mode` leaves the backdrop unchanged
/// where the layer is transparent.
fn neutral_under_transparent(mode: Option<BlendMode>) -> bool {
    use BlendMode::*;
    match mode {
        None => false,
        Some(mode) => !matches!(
            mode,
            Clear | Src | SrcIn | DstIn | SrcOut | DstATop | Modulate
        ),
    }
}

/// `rect` grown by what `paint` can add around a draw's geometry, or `None`
/// when the paint cannot bound it.
fn paint_bounds(paint: &Paint, rect: Rect) -> Option<Rect> {
    if paint.mask_filter().is_some() || paint.path_effect().is_some() {
        return None;
    }
    let mut rect = rect;
    if paint.style() != PaintStyle::Fill {
        let radius = paint.stroke_width().max(1.0) / 2.0;
        let factor = paint.stroke_miter().max(std::f32::consts::SQRT_2);
        let outset = radius * factor;
        rect = rect.with_outset((outset, outset));
    }
    if let Some(filter) = paint.image_filter() {
        if !filter.can_compute_fast_bounds() {
            return None;
        }
        rect = filter.compute_fast_bounds(rect);
    }
    Some(rect)
}

fn local_bounds(draw: &OwnedDraw) -> Option<Rect> {
    match draw {
        OwnedDraw::Clear(_) | OwnedDraw::Color(..) => None,
        OwnedDraw::Rect(rect, paint) | OwnedDraw::Oval(rect, paint) => {
            paint_bounds(paint, rect.sorted())
        }
        OwnedDraw::RoundRect(rect, _, _, paint) => paint_bounds(paint, rect.sorted()),
        OwnedDraw::Line(a, b, paint) => paint_bounds(
            paint,
            Rect::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)),
        ),
        OwnedDraw::Circle(center, radius, paint) => paint_bounds(
            paint,
            Rect::new(
                center.x - radius,
                center.y - radius,
                center.x + radius,
                center.y + radius,
            ),
        ),
        OwnedDraw::RRect(rrect, paint) => paint_bounds(paint, *rrect.rect()),
        OwnedDraw::DRRect(outer, _, paint) => paint_bounds(paint, *outer.rect()),
        OwnedDraw::Path(path, paint) => path.with_path(|path| {
            if path.is_inverse_fill_type() {
                None
            } else {
                paint_bounds(paint, *path.bounds())
            }
        }),
        OwnedDraw::Image(_, _, dst, _, paint) => paint_bounds(paint, dst.sorted()),
        OwnedDraw::ImageNine(_, _, dst, _, paint) => match paint {
            Some(paint) => paint_bounds(paint, dst.sorted()),
            None => Some(dst.sorted()),
        },
        OwnedDraw::TextBlob(blob, origin, paint) => {
            paint_bounds(paint, blob.bounds().with_offset(*origin))
        }
        OwnedDraw::String(text, origin, font, paint) => {
            let (_, bounds) = font.measure_str(text, Some(paint));
            paint_bounds(paint, bounds.with_offset(*origin))
        }
        OwnedDraw::Picture(picture, matrix, paint) => {
            // A picture can draw outside its cull rect; only the chunks of
            // a registered recording bound it.
            let chunks = registered_picture(picture)?;
            let mut rect = Rect::new_empty();
            for chunk in chunks.chunks() {
                rect.join(chunk.bounds);
            }
            if let Some(matrix) = matrix {
                if matrix.has_perspective() {
                    return None;
                }
                rect = matrix.map_rect(rect).0;
            }
            match paint {
                Some(paint) => paint_bounds(paint, rect),
                None => Some(rect),
            }
        }
    }
}

fn hash_draw(observer: &mut Observer, draw: &OwnedDraw, h: &mut DefaultHasher) {
    match draw {
        OwnedDraw::Clear(color) => {
            0u8.hash(h);
            for v in [color.r, color.g, color.b, color.a] {
                v.to_bits().hash(h);
            }
        }
        OwnedDraw::Color(color, mode) => {
            1u8.hash(h);
            for v in [color.r, color.g, color.b, color.a] {
                v.to_bits().hash(h);
            }
            (*mode as i32).hash(h);
        }
        OwnedDraw::Rect(rect, paint) => {
            2u8.hash(h);
            hash_rect(rect, h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Line(a, b, paint) => {
            3u8.hash(h);
            for v in [a.x, a.y, b.x, b.y] {
                v.to_bits().hash(h);
            }
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Oval(rect, paint) => {
            4u8.hash(h);
            hash_rect(rect, h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Circle(center, radius, paint) => {
            5u8.hash(h);
            for v in [center.x, center.y, *radius] {
                v.to_bits().hash(h);
            }
            observer.retain_paint(paint, h);
        }
        OwnedDraw::RRect(rrect, paint) => {
            6u8.hash(h);
            hash_rrect(rrect, h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::RoundRect(rect, rx, ry, paint) => {
            7u8.hash(h);
            hash_rect(rect, h);
            rx.to_bits().hash(h);
            ry.to_bits().hash(h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::DRRect(outer, inner, paint) => {
            8u8.hash(h);
            hash_rrect(outer, h);
            hash_rrect(inner, h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Path(path, paint) => {
            9u8.hash(h);
            path.with_path(|path| path.serialize().as_bytes().hash(h));
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Image(image, src, dst, sampling, paint) => {
            10u8.hash(h);
            hash_image(image, h);
            if let Some((rect, constraint)) = src {
                hash_rect(rect, h);
                (*constraint as i32).hash(h);
            }
            hash_rect(dst, h);
            format!("{sampling:?}").hash(h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::ImageNine(image, center, dst, filter, paint) => {
            11u8.hash(h);
            hash_image(image, h);
            [center.left, center.top, center.right, center.bottom].hash(h);
            hash_rect(dst, h);
            (*filter as i32).hash(h);
            if let Some(paint) = paint {
                observer.retain_paint(paint, h);
            }
        }
        OwnedDraw::TextBlob(blob, origin, paint) => {
            12u8.hash(h);
            match div_text::blob_fingerprint::text_blob_fingerprint(blob) {
                Some(fingerprint) => (true, fingerprint).hash(h),
                // Blob ids are never reused within a process.
                None => (false, u64::from(blob.unique_id())).hash(h),
            }
            origin.x.to_bits().hash(h);
            origin.y.to_bits().hash(h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::String(text, origin, font, paint) => {
            13u8.hash(h);
            text.hash(h);
            origin.x.to_bits().hash(h);
            origin.y.to_bits().hash(h);
            div_text::blob_fingerprint::hash_font(font, h);
            observer.retain_paint(paint, h);
        }
        OwnedDraw::Picture(picture, matrix, paint) => {
            14u8.hash(h);
            match registered_picture(picture) {
                Some(chunks) => {
                    chunks.complete.hash(h);
                    for chunk in chunks.chunks() {
                        chunk.fingerprint.hash(h);
                        hash_rect(&chunk.bounds, h);
                    }
                    observer
                        .chunks
                        .retained_mut()
                        .extend(chunks._retained.iter().cloned());
                }
                // Picture ids are never reused within a process.
                None => picture.unique_id().hash(h),
            }
            if let Some(matrix) = matrix {
                hash_matrix(matrix, h);
            }
            if let Some(paint) = paint {
                observer.retain_paint(paint, h);
            }
        }
    }
}

/// Raster images up to this many bytes are fingerprinted by their pixels.
const HASHED_IMAGE_BYTES: usize = 1 << 20;

/// Fingerprints an image. The painter rasters some images again in every
/// recording (a composited scrollbar's track, for one), so their ids always
/// change although their pixels do not. A small raster image is hashed by
/// its pixels and image info instead: drawn the same way, the same pixels
/// give the same output (RFC 0006 A2). Any other image is hashed by its id,
/// which is never reused within a process.
fn hash_image(image: &skia_safe::Image, h: &mut impl Hasher) {
    if let Some(pixmap) = image.peek_pixels() {
        let info = pixmap.info();
        let row = usize::try_from(info.width()).unwrap_or(0) * info.bytes_per_pixel();
        let rows = usize::try_from(info.height()).unwrap_or(0);
        let stride = pixmap.row_bytes();
        if let Some(bytes) = pixmap.bytes() {
            if row > 0 && rows > 0 && row * rows <= HASHED_IMAGE_BYTES && stride >= row {
                if bytes.len() >= stride * (rows - 1) + row {
                    true.hash(h);
                    (info.width(), info.height()).hash(h);
                    (info.color_type() as i32, info.alpha_type() as i32).hash(h);
                    info.color_space()
                        .map(|space| space.serialize().as_bytes().to_vec())
                        .hash(h);
                    for y in 0..rows {
                        bytes[y * stride..y * stride + row].hash(h);
                    }
                    return;
                }
            }
        }
    }
    (false, image.unique_id()).hash(h);
}

fn hash_rrect(rrect: &skia_safe::RRect, h: &mut impl Hasher) {
    hash_rect(rrect.rect(), h);
    for radius in rrect.radii_ref() {
        radius.x.to_bits().hash(h);
        radius.y.to_bits().hash(h);
    }
}

fn clip_rect(canvas: &Canvas) -> Option<Rect> {
    canvas
        .device_clip_bounds()
        .filter(|bounds| !bounds.is_empty())
        .map(|bounds| Rect::from(bounds))
}

/// A state operation on the canvas, before Skia applies it.
pub(crate) fn observe_operation(canvas: &Canvas, operation: &ProgramOperation) {
    with_observer(canvas, |observer| {
        observer.note_in_layers(std::mem::discriminant(operation));
        let mut h = DefaultHasher::new();
        let top = observer.top();
        top.state.hash(&mut h);
        match operation {
            ProgramOperation::Save => observer.levels.push(Level { layer: None, ..top }),
            ProgramOperation::SaveLayerAlpha(bounds, alpha) => {
                20u8.hash(&mut h);
                bounds.map(|b| hash_rect(&b, &mut h));
                alpha.to_bits().hash(&mut h);
                hash_matrix(&canvas.local_to_device_as_3x3(), &mut h);
                let layer = observer.new_layer();
                observer.levels.push(Level {
                    state: h.finish(),
                    spread: top.spread,
                    layer,
                });
            }
            ProgramOperation::SaveLayer {
                bounds,
                paint,
                flags,
            } => {
                21u8.hash(&mut h);
                bounds.map(|b| hash_rect(&b, &mut h));
                flags.bits().hash(&mut h);
                let matrix = canvas.local_to_device_as_3x3();
                hash_matrix(&matrix, &mut h);
                let mut spread = top.spread;
                if let Some(paint) = paint {
                    observer.retain_paint(paint, &mut h);
                    let spreads = paint.image_filter().is_some()
                        || !neutral_under_transparent(paint.as_blend_mode())
                        || paint.blender().is_some() && paint.as_blend_mode().is_none();
                    if spreads && spread.is_none() {
                        let mut bounds_rect = clip_rect(canvas).unwrap_or_default();
                        if let Some(layer) = bounds {
                            if paint.image_filter().is_none() {
                                let mapped = matrix.map_rect(*layer).0;
                                if !bounds_rect.intersect(mapped) {
                                    bounds_rect = Rect::default();
                                }
                            }
                        }
                        spread = Some(bounds_rect);
                    }
                }
                let layer = observer.new_layer();
                observer.levels.push(Level {
                    state: h.finish(),
                    spread,
                    layer,
                });
            }
            ProgramOperation::ClipRect(rect, op, aa) => {
                22u8.hash(&mut h);
                hash_rect(rect, &mut h);
                hash_clip(*op, *aa, canvas, &mut h);
                observer.levels.last_mut().expect("base level").state = h.finish();
            }
            ProgramOperation::ClipRRect(rrect, op, aa) => {
                23u8.hash(&mut h);
                hash_rrect(rrect, &mut h);
                hash_clip(*op, *aa, canvas, &mut h);
                observer.levels.last_mut().expect("base level").state = h.finish();
            }
            ProgramOperation::ClipPath(path, op, aa) => {
                24u8.hash(&mut h);
                path.with_path(|path| path.serialize().as_bytes().hash(&mut h));
                hash_clip(*op, *aa, canvas, &mut h);
                observer.levels.last_mut().expect("base level").state = h.finish();
            }
            // The matrix is read at each draw.
            _ => {}
        }
    });
}

fn hash_clip(op: ClipOp, aa: bool, canvas: &Canvas, h: &mut impl Hasher) {
    (op as i32).hash(h);
    aa.hash(h);
    hash_matrix(&canvas.local_to_device_as_3x3(), h);
}

/// A restore, before Skia applies it.
pub(crate) fn observe_restore(canvas: &Canvas) {
    with_observer(canvas, |observer| {
        if observer.levels.len() > 1 {
            observer.pop_level();
        } else {
            observer.chunks.complete = false;
        }
    });
}

/// A `restore_to_count(count)`, before Skia applies it.
pub(crate) fn observe_restore_to(canvas: &Canvas, count: usize) {
    with_observer(canvas, |observer| {
        match count.checked_sub(observer.base_save_count) {
            Some(depth) => {
                while observer.levels.len() > depth + 1 {
                    observer.pop_level();
                }
            }
            None => observer.chunks.complete = false,
        }
    });
}

pub(crate) fn enter_owner(
    canvas: &Canvas,
    node: NodeId,
    owns_element_box: bool,
    phase: PaintOwnerPhase,
) {
    with_observer(canvas, |observer| {
        observer
            .owners
            .push((node, phase == PaintOwnerPhase::Contents, owns_element_box));
    });
}

pub(crate) fn exit_owner(canvas: &Canvas) {
    with_observer(canvas, |observer| {
        observer.owners.pop();
    });
}

/// A draw that escaped the facade: only the whole viewport is safe.
pub(crate) fn mark_incomplete(canvas: &Canvas) {
    with_observer(canvas, |observer| observer.chunks.complete = false);
}

/// One draw, before Skia records it.
pub(crate) fn observe_draw(canvas: &Canvas, draw: &OwnedDraw) {
    with_observer(canvas, |observer| {
        // The levels must follow the canvas's save stack; if they do not,
        // a save or restore escaped the facade.
        if observer.canvas_depth(canvas) != Some(observer.levels.len() - 1) {
            observer.chunks.complete = false;
        }
        observer.note_draw();
        let Some(clip) = clip_rect(canvas) else {
            // Fully clipped out: the draw changes no pixel.
            return;
        };
        let clip = if observer.base_clip == Some(clip) {
            UNBOUNDED
        } else {
            clip
        };
        let top = observer.top();
        let matrix = canvas.local_to_device_as_3x3();
        if let OwnedDraw::Picture(picture, picture_matrix, paint) = draw {
            if top.spread.is_none() && paint.as_ref().is_none_or(splices_through) {
                if let Some(chunks) = registered_picture(picture) {
                    observer.splice(&chunks, matrix, *picture_matrix, paint.as_ref(), clip);
                    return;
                }
            }
        }
        let bounds = match top.spread {
            Some(spread) => spread,
            None => match local_bounds(draw) {
                Some(local) if !matrix.has_perspective() => {
                    let mut mapped = matrix.map_rect(local).0;
                    // Anti-aliasing and hinting can touch one more pixel.
                    mapped = mapped.with_outset((1.0, 1.0));
                    if mapped.intersect(clip) {
                        mapped
                    } else {
                        return;
                    }
                }
                _ => clip,
            },
        };
        let mut h = DefaultHasher::new();
        top.state.hash(&mut h);
        hash_matrix(&matrix, &mut h);
        hash_rect(&clip, &mut h);
        hash_draw(observer, draw, &mut h);
        let fingerprint = h.finish();
        let owner = observer.owner();
        match &mut observer.open {
            Some(chunk) if chunk.owner == owner => {
                chunk.bounds.join(bounds);
                let mut fold = DefaultHasher::new();
                chunk.fingerprint.hash(&mut fold);
                fingerprint.hash(&mut fold);
                chunk.fingerprint = fold.finish();
            }
            _ => {
                observer.close();
                let occurrence = observer.occurrences.entry(owner).or_insert(0);
                observer.open = Some(PaintChunk {
                    owner,
                    occurrence: *occurrence,
                    bounds,
                    fingerprint,
                });
                *occurrence += 1;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(node: NodeId, bounds: Rect, fingerprint: u64) -> PaintChunk {
        PaintChunk {
            owner: ChunkOwner {
                node: Some(node),
                contents: false,
                owns_element_box: true,
                overlay: false,
            },
            occurrence: 0,
            bounds,
            fingerprint,
        }
    }

    fn chunks(list: Vec<PaintChunk>) -> PaintChunks {
        PaintChunks {
            chunks: list,
            complete: true,
            _retained: Vec::new(),
        }
    }

    #[test]
    fn unchanged_chunks_damage_nothing_and_changes_damage_old_and_new() {
        let mut document = div_dom::Document::new();
        let one = document.create_node(div_dom::ElementTag::default());
        let two = document.create_node(div_dom::ElementTag::default());
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(20.0, 0.0, 10.0, 10.0);
        let c = Rect::from_xywh(40.0, 0.0, 10.0, 10.0);
        let old = chunks(vec![chunk(one, a, 1), chunk(two, b, 2)]);
        assert_eq!(old.damage_since(&old), Some(vec![]));
        let moved = chunks(vec![chunk(one, a, 1), chunk(two, c, 2)]);
        assert_eq!(moved.damage_since(&old), Some(vec![b, c]));
        let gone = chunks(vec![chunk(one, a, 1)]);
        assert_eq!(gone.damage_since(&old), Some(vec![b]));
        let swapped = chunks(vec![chunk(two, b, 2), chunk(one, a, 1)]);
        assert_eq!(swapped.damage_since(&old).map(|d| d.len()), Some(1));
        let mut incomplete = old.clone();
        incomplete.complete = false;
        assert_eq!(old.damage_since(&incomplete), None);
    }
}
