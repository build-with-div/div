//! Shaping with HarfBuzz directly, as Blink's `HarfBuzzShaper` does (RFC
//! 0013).
//!
//! The HarfBuzz C API is resolved against the HarfBuzz that Skia links; only
//! functions from HarfBuzz's stable public API are declared. The run
//! segmentation, buffer setup and run order follow Skia's `SkShaper`
//! (`modules/skshaper/src/SkShaper_harfbuzz.cpp`, `ShapeThenWrap` with an
//! infinite width), which Div used before. The font follows Blink
//! (`harfbuzz_face.cc`, `skia_text_metrics.cc`, Chromium 147):
//!
//! - advances and glyph extents convert to HarfBuzz's 16.16 units by
//!   truncating (`SkiaScalarToHarfBuzzPosition`), where `SkShaper` rounds;
//! - the scale is `ClampTo<int>(size * 65536)`, and `ptem` is the specified
//!   size;
//! - each glyph keeps HarfBuzz's advance in 16.16 units, so a run's width is
//!   an exact sum.

use std::collections::{HashMap, VecDeque};
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use skia_safe::shaper::{self, RunIterator};
use skia_safe::{Font as SkFont, GlyphId, Typeface};

// ── The C API ───────────────────────────────────────────────────────────

#[repr(C)]
struct HbBlob {
    _private: [u8; 0],
}
#[repr(C)]
struct HbFace {
    _private: [u8; 0],
}
#[repr(C)]
struct HbFont {
    _private: [u8; 0],
}
#[repr(C)]
struct HbFontFuncs {
    _private: [u8; 0],
}
#[repr(C)]
struct HbBuffer {
    _private: [u8; 0],
}
#[repr(C)]
struct HbLanguageImpl {
    _private: [u8; 0],
}

type HbBool = c_int;
type HbCodepoint = u32;
type HbPosition = i32;
type HbTag = u32;
type HbLanguage = *const HbLanguageImpl;
type HbDestroy = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
#[derive(Clone, Copy)]
struct HbGlyphInfo {
    codepoint: HbCodepoint,
    mask: u32,
    cluster: u32,
    var1: u32,
    var2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HbGlyphPosition {
    x_advance: HbPosition,
    y_advance: HbPosition,
    x_offset: HbPosition,
    y_offset: HbPosition,
    var: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HbFeature {
    tag: HbTag,
    value: u32,
    start: c_uint,
    end: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HbVariation {
    tag: HbTag,
    value: f32,
}

#[repr(C)]
struct HbGlyphExtents {
    x_bearing: HbPosition,
    y_bearing: HbPosition,
    width: HbPosition,
    height: HbPosition,
}

const HB_MEMORY_MODE_READONLY: c_int = 1;
const HB_DIRECTION_LTR: c_int = 4;
const HB_DIRECTION_RTL: c_int = 5;
const HB_BUFFER_CONTENT_TYPE_UNICODE: c_int = 1;
const HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS: c_int = 1;
const HB_FEATURE_GLOBAL_START: c_uint = 0;
const HB_FEATURE_GLOBAL_END: c_uint = c_uint::MAX;

type HbReferenceTable = unsafe extern "C" fn(*mut HbFace, HbTag, *mut c_void) -> *mut HbBlob;
type HbNominalGlyph = unsafe extern "C" fn(
    *mut HbFont,
    *mut c_void,
    HbCodepoint,
    *mut HbCodepoint,
    *mut c_void,
) -> HbBool;
type HbVariationGlyph = unsafe extern "C" fn(
    *mut HbFont,
    *mut c_void,
    HbCodepoint,
    HbCodepoint,
    *mut HbCodepoint,
    *mut c_void,
) -> HbBool;
type HbNominalGlyphs = unsafe extern "C" fn(
    *mut HbFont,
    *mut c_void,
    c_uint,
    *const HbCodepoint,
    c_uint,
    *mut HbCodepoint,
    c_uint,
    *mut c_void,
) -> c_uint;
type HbGlyphAdvance =
    unsafe extern "C" fn(*mut HbFont, *mut c_void, HbCodepoint, *mut c_void) -> HbPosition;
type HbGlyphAdvances = unsafe extern "C" fn(
    *mut HbFont,
    *mut c_void,
    c_uint,
    *const HbCodepoint,
    c_uint,
    *mut HbPosition,
    c_uint,
    *mut c_void,
);
type HbGlyphExtentsFunc = unsafe extern "C" fn(
    *mut HbFont,
    *mut c_void,
    HbCodepoint,
    *mut HbGlyphExtents,
    *mut c_void,
) -> HbBool;

unsafe extern "C" {
    #[cfg(test)]
    fn hb_version_string() -> *const c_char;

    fn hb_blob_create(
        data: *const c_char,
        length: c_uint,
        mode: c_int,
        user_data: *mut c_void,
        destroy: HbDestroy,
    ) -> *mut HbBlob;
    fn hb_blob_make_immutable(blob: *mut HbBlob);

    fn hb_face_create_for_tables(
        reference_table: Option<HbReferenceTable>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    ) -> *mut HbFace;
    fn hb_face_set_index(face: *mut HbFace, index: c_uint);
    fn hb_face_set_upem(face: *mut HbFace, upem: c_uint);
    fn hb_face_destroy(face: *mut HbFace);

    fn hb_font_create(face: *mut HbFace) -> *mut HbFont;
    fn hb_font_create_sub_font(parent: *mut HbFont) -> *mut HbFont;
    fn hb_ot_font_set_funcs(font: *mut HbFont);
    fn hb_font_set_variations(font: *mut HbFont, variations: *const HbVariation, length: c_uint);
    fn hb_font_set_funcs(
        font: *mut HbFont,
        klass: *mut HbFontFuncs,
        font_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_set_scale(font: *mut HbFont, x_scale: c_int, y_scale: c_int);
    fn hb_font_set_ptem(font: *mut HbFont, ptem: f32);
    fn hb_font_make_immutable(font: *mut HbFont);
    fn hb_font_destroy(font: *mut HbFont);

    fn hb_font_funcs_create() -> *mut HbFontFuncs;
    fn hb_font_funcs_make_immutable(funcs: *mut HbFontFuncs);
    fn hb_font_funcs_set_nominal_glyph_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbNominalGlyph>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_funcs_set_nominal_glyphs_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbNominalGlyphs>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_funcs_set_variation_glyph_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbVariationGlyph>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_funcs_set_glyph_h_advance_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbGlyphAdvance>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_funcs_set_glyph_h_advances_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbGlyphAdvances>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );
    fn hb_font_funcs_set_glyph_extents_func(
        funcs: *mut HbFontFuncs,
        func: Option<HbGlyphExtentsFunc>,
        user_data: *mut c_void,
        destroy: HbDestroy,
    );

    fn hb_buffer_create() -> *mut HbBuffer;
    fn hb_buffer_destroy(buffer: *mut HbBuffer);
    fn hb_buffer_set_content_type(buffer: *mut HbBuffer, content_type: c_int);
    fn hb_buffer_set_cluster_level(buffer: *mut HbBuffer, level: c_int);
    fn hb_buffer_add_utf8(
        buffer: *mut HbBuffer,
        text: *const c_char,
        text_length: c_int,
        item_offset: c_uint,
        item_length: c_int,
    );
    fn hb_buffer_add(buffer: *mut HbBuffer, codepoint: HbCodepoint, cluster: c_uint);
    fn hb_buffer_set_direction(buffer: *mut HbBuffer, direction: c_int);
    fn hb_buffer_set_script(buffer: *mut HbBuffer, script: u32);
    fn hb_buffer_set_language(buffer: *mut HbBuffer, language: HbLanguage);
    fn hb_buffer_guess_segment_properties(buffer: *mut HbBuffer);
    fn hb_buffer_get_glyph_infos(buffer: *mut HbBuffer, length: *mut c_uint) -> *mut HbGlyphInfo;
    fn hb_buffer_get_glyph_positions(
        buffer: *mut HbBuffer,
        length: *mut c_uint,
    ) -> *mut HbGlyphPosition;
    fn hb_shape(
        font: *mut HbFont,
        buffer: *mut HbBuffer,
        features: *const HbFeature,
        num_features: c_uint,
    );

    fn hb_language_from_string(text: *const c_char, length: c_int) -> HbLanguage;
    fn hb_script_from_iso15924_tag(tag: HbTag) -> u32;
}

/// HarfBuzz's version, as linked through Skia.
#[cfg(test)]
pub(crate) fn version() -> String {
    // SAFETY: `hb_version_string` returns a static NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(hb_version_string()) }
        .to_string_lossy()
        .into_owned()
}

// ── Unit conversion ─────────────────────────────────────────────────────

/// Blink's `SkiaScalarToHarfBuzzPosition`: `ClampTo<int>(value * 65536)`, a
/// float multiply truncated toward zero with saturation. Rust's float to
/// integer cast truncates and saturates the same way (NaN becomes 0).
#[inline]
pub(crate) fn to_harfbuzz_position(value: f32) -> i32 {
    (value * 65536.0) as i32
}

/// `SkScalarRoundToInt`: round half up, then saturate.
#[inline]
fn round_to_int(value: f32) -> f32 {
    ((value as f64) + 0.5).floor() as i32 as f32
}

// ── Font funcs (Blink's Skia funcs; glyph lookup as Skia's) ─────────────

/// The data a sub-font's funcs read: the shaping font.
struct FuncData {
    font: SkFont,
}

unsafe fn func_font<'a>(data: *mut c_void) -> &'a SkFont {
    // SAFETY: `data` is the `FuncData` the sub-font owns for its lifetime.
    unsafe { &(*(data as *const FuncData)).font }
}

/// `skhb_glyph`: Skia's cmap, ignoring any variation selector.
unsafe extern "C" fn variation_glyph(
    _font: *mut HbFont,
    data: *mut c_void,
    unicode: HbCodepoint,
    _variation_selector: HbCodepoint,
    glyph: *mut HbCodepoint,
    _user: *mut c_void,
) -> HbBool {
    let font = unsafe { func_font(data) };
    let id = font.unichar_to_glyph(unicode as i32);
    unsafe { *glyph = HbCodepoint::from(id) };
    (id != 0) as HbBool
}

/// `skhb_nominal_glyph`.
unsafe extern "C" fn nominal_glyph(
    font: *mut HbFont,
    data: *mut c_void,
    unicode: HbCodepoint,
    glyph: *mut HbCodepoint,
    user: *mut c_void,
) -> HbBool {
    unsafe { variation_glyph(font, data, unicode, 0, glyph, user) }
}

/// `skhb_nominal_glyphs`: a batch lookup that stops at the first missing
/// glyph; HarfBuzz asks for the rest one at a time.
unsafe extern "C" fn nominal_glyphs(
    _font: *mut HbFont,
    data: *mut c_void,
    count: c_uint,
    first_unicode: *const HbCodepoint,
    unicode_stride: c_uint,
    first_glyph: *mut HbCodepoint,
    glyph_stride: c_uint,
    _user: *mut c_void,
) -> c_uint {
    let font = unsafe { func_font(data) };
    let mut done = 0;
    for index in 0..count as usize {
        let unicode = unsafe {
            *((first_unicode as *const u8).add(index * unicode_stride as usize)
                as *const HbCodepoint)
        };
        let glyph = font.unichar_to_glyph(unicode as i32);
        if glyph == 0 {
            break;
        }
        unsafe {
            *((first_glyph as *mut u8).add(index * glyph_stride as usize) as *mut HbCodepoint) =
                HbCodepoint::from(glyph);
        }
        done += 1;
    }
    done
}

/// Blink's `SkFontGetGlyphWidthForHarfBuzz`: Skia's width, rounded to whole
/// pixels when the font does not position subpixels, then truncated to
/// HarfBuzz units.
fn advance_units(font: &SkFont, width: f32) -> HbPosition {
    let width = if font.is_subpixel() {
        width
    } else {
        round_to_int(width)
    };
    to_harfbuzz_position(width)
}

unsafe extern "C" fn glyph_h_advance(
    _font: *mut HbFont,
    data: *mut c_void,
    glyph: HbCodepoint,
    _user: *mut c_void,
) -> HbPosition {
    let font = unsafe { func_font(data) };
    let mut width = [0.0f32];
    font.get_widths(&[glyph as GlyphId], &mut width);
    advance_units(font, width[0])
}

unsafe extern "C" fn glyph_h_advances(
    _font: *mut HbFont,
    data: *mut c_void,
    count: c_uint,
    first_glyph: *const HbCodepoint,
    glyph_stride: c_uint,
    first_advance: *mut HbPosition,
    advance_stride: c_uint,
    _user: *mut c_void,
) {
    let font = unsafe { func_font(data) };
    let count = count as usize;
    let glyphs: Vec<GlyphId> = (0..count)
        .map(|index| unsafe {
            *((first_glyph as *const u8).add(index * glyph_stride as usize) as *const HbCodepoint)
                as GlyphId
        })
        .collect();
    let mut widths = vec![0.0f32; count];
    font.get_widths(&glyphs, &mut widths);
    for (index, width) in widths.into_iter().enumerate() {
        unsafe {
            *((first_advance as *mut u8).add(index * advance_stride as usize) as *mut HbPosition) =
                advance_units(font, width);
        }
    }
}

/// Blink's `SkFontGetGlyphExtentsForHarfBuzz`: Skia's bounds, rounded out
/// when the font does not position subpixels, truncated to HarfBuzz units.
unsafe extern "C" fn glyph_extents(
    _font: *mut HbFont,
    data: *mut c_void,
    glyph: HbCodepoint,
    extents: *mut HbGlyphExtents,
    _user: *mut c_void,
) -> HbBool {
    let font = unsafe { func_font(data) };
    let mut bounds = [skia_safe::Rect::default()];
    font.get_bounds(&[glyph as GlyphId], &mut bounds, None);
    let mut rect = bounds[0];
    if !font.is_subpixel() {
        // `SkRect::roundOut`.
        rect = skia_safe::Rect::new(
            rect.left.floor(),
            rect.top.floor(),
            rect.right.ceil(),
            rect.bottom.ceil(),
        );
    }
    unsafe {
        (*extents).x_bearing = to_harfbuzz_position(rect.left);
        (*extents).y_bearing = to_harfbuzz_position(-rect.top);
        (*extents).width = to_harfbuzz_position(rect.width());
        (*extents).height = to_harfbuzz_position(-rect.height());
    }
    1
}

struct Funcs(*mut HbFontFuncs);
// SAFETY: the funcs are immutable after creation; HarfBuzz reads them from
// any thread.
unsafe impl Send for Funcs {}
unsafe impl Sync for Funcs {}

fn font_funcs() -> *mut HbFontFuncs {
    static FUNCS: OnceLock<Funcs> = OnceLock::new();
    FUNCS
        .get_or_init(|| unsafe {
            let funcs = hb_font_funcs_create();
            hb_font_funcs_set_variation_glyph_func(
                funcs,
                Some(variation_glyph),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_set_nominal_glyph_func(
                funcs,
                Some(nominal_glyph),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_set_nominal_glyphs_func(
                funcs,
                Some(nominal_glyphs),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_set_glyph_h_advance_func(
                funcs,
                Some(glyph_h_advance),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_set_glyph_h_advances_func(
                funcs,
                Some(glyph_h_advances),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_set_glyph_extents_func(
                funcs,
                Some(glyph_extents),
                std::ptr::null_mut(),
                None,
            );
            hb_font_funcs_make_immutable(funcs);
            Funcs(funcs)
        })
        .0
}

unsafe extern "C" fn drop_func_data(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data as *mut FuncData) });
}

// ── Faces and parent fonts ──────────────────────────────────────────────

/// Table data handed to HarfBuzz; the blob owns it.
unsafe extern "C" fn drop_table(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data as *mut skia_safe::Data) });
}

unsafe extern "C" fn reference_table(
    _face: *mut HbFace,
    tag: HbTag,
    user: *mut c_void,
) -> *mut HbBlob {
    let typeface = unsafe { &*(user as *const Typeface) };
    let Some(data) = typeface.copy_table_data(tag) else {
        return std::ptr::null_mut();
    };
    let length = data.len();
    let data = Box::new(data);
    let pointer = data.as_bytes().as_ptr() as *const c_char;
    unsafe {
        let blob = hb_blob_create(
            pointer,
            length as c_uint,
            HB_MEMORY_MODE_READONLY,
            Box::into_raw(data) as *mut c_void,
            Some(drop_table),
        );
        hb_blob_make_immutable(blob);
        blob
    }
}

unsafe extern "C" fn drop_typeface(user: *mut c_void) {
    drop(unsafe { Box::from_raw(user as *mut Typeface) });
}

/// A face over the typeface's tables, `create_hb_face`'s table path, with
/// Skia's units per em; then an OpenType parent font at the typeface's
/// variation position (`create_typeface_hb_font`).
fn create_parent_font(typeface: &Typeface) -> *mut HbFont {
    let index = typeface.to_font_data().map_or(0, |(_, index)| index);
    unsafe {
        let face = hb_face_create_for_tables(
            Some(reference_table),
            Box::into_raw(Box::new(typeface.clone())) as *mut c_void,
            Some(drop_typeface),
        );
        hb_face_set_index(face, index as c_uint);
        hb_face_set_upem(face, typeface.units_per_em().unwrap_or(0).max(0) as c_uint);
        let font = hb_font_create(face);
        hb_face_destroy(face);
        hb_ot_font_set_funcs(font);
        if let Some(coordinates) = typeface.variation_design_position() {
            let variations: Vec<HbVariation> = coordinates
                .iter()
                .map(|coordinate| HbVariation {
                    tag: *coordinate.axis,
                    value: coordinate.value,
                })
                .collect();
            if !variations.is_empty() {
                hb_font_set_variations(font, variations.as_ptr(), variations.len() as c_uint);
            }
        }
        hb_font_make_immutable(font);
        font
    }
}

struct ParentFont(*mut HbFont);
// SAFETY: an immutable `hb_font_t` is shared across threads; each shaping call
// makes its own sub-font.
unsafe impl Send for ParentFont {}

struct ParentFonts {
    fonts: HashMap<u32, ParentFont>,
    order: VecDeque<u32>,
}

/// Skia keeps 100 typeface fonts (`SKHB`'s LRU); so does Div.
const PARENT_FONT_CACHE: usize = 100;

/// A sub-font of the typeface's cached parent with Blink's funcs over
/// `font`, Blink's scale and `ptem`.
fn create_font(font: &SkFont, specified_size: f32) -> *mut HbFont {
    static PARENTS: OnceLock<Mutex<ParentFonts>> = OnceLock::new();
    let typeface = font.typeface();
    let id = u32::from(typeface.unique_id());
    let parents = PARENTS.get_or_init(|| {
        Mutex::new(ParentFonts {
            fonts: HashMap::new(),
            order: VecDeque::new(),
        })
    });
    let mut parents = parents
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !parents.fonts.contains_key(&id) {
        if parents.order.len() >= PARENT_FONT_CACHE {
            if let Some(oldest) = parents.order.pop_front() {
                if let Some(ParentFont(old)) = parents.fonts.remove(&oldest) {
                    // Sub-fonts hold their own reference to the parent.
                    unsafe { hb_font_destroy(old) };
                }
            }
        }
        parents
            .fonts
            .insert(id, ParentFont(create_parent_font(&typeface)));
        parents.order.push_back(id);
    }
    let parent = parents.fonts[&id].0;
    unsafe {
        let sub = hb_font_create_sub_font(parent);
        drop(parents);
        hb_font_set_funcs(
            sub,
            font_funcs(),
            Box::into_raw(Box::new(FuncData { font: font.clone() })) as *mut c_void,
            Some(drop_func_data),
        );
        let size = font.size();
        let scale = to_harfbuzz_position(size);
        hb_font_set_scale(sub, scale, scale);
        hb_font_set_ptem(
            sub,
            if specified_size > 0.0 {
                specified_size
            } else {
                size
            },
        );
        sub
    }
}

// ── Shaping ─────────────────────────────────────────────────────────────

/// One shaped run: a segment of one bidi level and one script.
pub(crate) struct ShapedRun {
    /// Glyphs left to right (HarfBuzz's output order).
    pub glyphs: Vec<GlyphId>,
    /// HarfBuzz's `x_advance` per glyph, in 1/65536 px.
    pub advance_units: Vec<i32>,
    /// HarfBuzz's offsets in px, y down.
    pub offsets: Vec<(f32, f32)>,
    /// Each glyph's cluster: the UTF-8 byte offset of its first character
    /// in the whole text.
    pub clusters: Vec<u32>,
    /// The run's bytes in the whole text.
    pub utf8_range: Range<usize>,
}

/// An OpenType feature applied to the whole text.
pub(crate) struct Feature {
    pub tag: u32,
    pub value: u32,
}

struct Segment {
    range: Range<usize>,
    level: u8,
    script: u32,
}

/// The runs `SkShaper`'s `RunIteratorQueue` yields: the intersection of the
/// bidi and script runs (the font and language runs cover the whole text).
fn segments(text: &str, level: u8) -> Vec<Segment> {
    let mut script = shaper::Shaper::new_hb_icu_script_run_iterator(text);
    match shaper::Shaper::new_icu_bidi_run_iterator(text, level) {
        Some(mut bidi) => intersect_runs(&mut bidi, &mut script),
        None => {
            #[allow(deprecated)]
            let mut bidi = shaper::Shaper::new_trivial_bidi_run_iterator(level, text.len());
            intersect_runs(&mut bidi, &mut script)
        }
    }
}

/// `RunIteratorQueue::advanceRuns` over the bidi and script iterators. Every
/// iterator starts at end 0, so the first step consumes both; each later
/// step consumes every iterator ending at the least end, and the queue stops
/// once the iterator ending first is exhausted.
fn intersect_runs(
    bidi: &mut shaper::BiDiRunIterator,
    script: &mut shaper::ScriptRunIterator,
) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let bidi_end = bidi.end_of_current_run();
        let script_end = script.end_of_current_run();
        let least_at_end = if bidi_end <= script_end {
            bidi.at_end()
        } else {
            script.at_end()
        };
        if least_at_end {
            break;
        }
        let least_end = bidi_end.min(script_end);
        if bidi_end <= least_end {
            bidi.consume();
        }
        if script_end <= least_end {
            script.consume();
        }
        let end = bidi.end_of_current_run().min(script.end_of_current_run());
        out.push(Segment {
            range: start..end,
            level: bidi.current_level(),
            script: *script.current_script(),
        });
        start = end;
    }
    out
}

/// UAX #9 rule L2: reverse every maximal sequence of runs at or above each
/// level, from the highest level down to the lowest odd one, as ICU's
/// `ubidi_reorderVisual` does for `ShapeThenWrap`.
fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&highest) = levels.iter().max() else {
        return order;
    };
    let lowest_odd = levels.iter().copied().filter(|level| level % 2 == 1).min();
    let Some(lowest_odd) = lowest_odd else {
        return order;
    };
    let mut level = highest;
    while level >= lowest_odd {
        let mut index = 0;
        while index < order.len() {
            if levels[order[index]] >= level {
                let start = index;
                while index < order.len() && levels[order[index]] >= level {
                    index += 1;
                }
                order[start..index].reverse();
            } else {
                index += 1;
            }
        }
        if level == 0 {
            break;
        }
        level -= 1;
    }
    order
}

/// Shape `text` in `font` at the paragraph `level` (0 for LTR, 1 for RTL),
/// returning the runs in visual order.
pub(crate) fn shape(
    text: &str,
    font: &SkFont,
    specified_size: f32,
    level: u8,
    language: &str,
    features: &[Feature],
) -> Vec<ShapedRun> {
    if text.is_empty() {
        return Vec::new();
    }
    let segments = segments(text, level);
    let features: Vec<HbFeature> = features
        .iter()
        .map(|feature| HbFeature {
            tag: feature.tag,
            value: feature.value,
            start: HB_FEATURE_GLOBAL_START,
            end: HB_FEATURE_GLOBAL_END,
        })
        .collect();
    let language = std::ffi::CString::new(language).unwrap_or_default();
    let hb_font = create_font(font, specified_size);
    let mut runs = Vec::with_capacity(segments.len());
    unsafe {
        let mut hb_language = hb_language_from_string(language.as_ptr(), -1);
        if hb_language.is_null() {
            hb_language = hb_language_from_string(c"und".as_ptr(), -1);
        }
        let buffer = hb_buffer_create();
        for segment in &segments {
            runs.push(shape_segment(
                buffer,
                hb_font,
                text,
                segment,
                hb_language,
                &features,
            ));
        }
        hb_buffer_destroy(buffer);
        hb_font_destroy(hb_font);
    }
    let levels: Vec<u8> = segments.iter().map(|segment| segment.level).collect();
    let mut ordered: Vec<Option<ShapedRun>> = runs.into_iter().map(Some).collect();
    visual_order(&levels)
        .into_iter()
        .filter_map(|index| ordered[index].take())
        .collect()
}

/// `ShaperHarfBuzz::shape` for one segment: the whole text as context,
/// UTF-8 byte clusters, characters kept monotone.
unsafe fn shape_segment(
    buffer: *mut HbBuffer,
    font: *mut HbFont,
    text: &str,
    segment: &Segment,
    language: HbLanguage,
    features: &[HbFeature],
) -> ShapedRun {
    let range = segment.range.clone();
    let base = text.as_ptr() as *const c_char;
    unsafe {
        // `hb_buffer_clear_contents` keeps the settings below; set them
        // each time so the buffer's state never leaks between segments.
        hb_buffer_set_content_type(buffer, HB_BUFFER_CONTENT_TYPE_UNICODE);
        hb_buffer_set_cluster_level(buffer, HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS);
        hb_buffer_add_utf8(buffer, base, range.start as c_int, range.start as c_uint, 0);
        for (offset, character) in text[range.clone()].char_indices() {
            hb_buffer_add(
                buffer,
                character as HbCodepoint,
                (range.start + offset) as c_uint,
            );
        }
        hb_buffer_add_utf8(
            buffer,
            base.add(range.end),
            (text.len() - range.end) as c_int,
            0,
            0,
        );
        hb_buffer_set_direction(
            buffer,
            if segment.level % 2 == 0 {
                HB_DIRECTION_LTR
            } else {
                HB_DIRECTION_RTL
            },
        );
        hb_buffer_set_script(buffer, hb_script_from_iso15924_tag(segment.script));
        hb_buffer_set_language(buffer, language);
        hb_buffer_guess_segment_properties(buffer);
        hb_shape(font, buffer, features.as_ptr(), features.len() as c_uint);
        let mut length = 0;
        let infos = hb_buffer_get_glyph_infos(buffer, &mut length);
        let positions = hb_buffer_get_glyph_positions(buffer, &mut length);
        let count = length as usize;
        let mut run = ShapedRun {
            glyphs: Vec::with_capacity(count),
            advance_units: Vec::with_capacity(count),
            offsets: Vec::with_capacity(count),
            clusters: Vec::with_capacity(count),
            utf8_range: range,
        };
        if count > 0 {
            let infos = std::slice::from_raw_parts(infos, count);
            let positions = std::slice::from_raw_parts(positions, count);
            for (info, position) in infos.iter().zip(positions) {
                run.glyphs.push(info.codepoint as GlyphId);
                run.advance_units.push(position.x_advance);
                run.offsets.push((
                    position.x_offset as f32 / 65536.0,
                    -(position.y_offset as f32) / 65536.0,
                ));
                run.clusters.push(info.cluster);
            }
        }
        hb_buffer_clear_contents(buffer);
        run
    }
}

unsafe extern "C" {
    fn hb_buffer_clear_contents(buffer: *mut HbBuffer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harfbuzz_resolves_against_skia() {
        let version = version();
        assert!(version.split('.').count() >= 2, "{version}");
    }

    #[test]
    fn positions_truncate_like_blink() {
        assert_eq!(to_harfbuzz_position(1.5), 98304);
        // 0.3 * 65536 = 19660.8 in f32: Blink truncates, SkShaper rounds up.
        assert_eq!(to_harfbuzz_position(0.3), 19660);
        assert_eq!(to_harfbuzz_position(-1.7), -111411);
        assert_eq!(to_harfbuzz_position(f32::INFINITY), i32::MAX);
    }

    #[test]
    fn runs_reorder_visually_by_level() {
        assert_eq!(visual_order(&[0, 1, 1, 0]), vec![0, 2, 1, 3]);
        assert_eq!(visual_order(&[1, 2, 1]), vec![2, 1, 0]);
        assert_eq!(visual_order(&[0, 0]), vec![0, 1]);
        assert_eq!(visual_order(&[1]), vec![0]);
    }
}
