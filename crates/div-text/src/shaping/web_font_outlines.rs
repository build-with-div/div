//! Web-font glyph outlines as Linux Chromium rasterizes them.
//!
//! Blink instantiates every web font on Linux with
//! `SkTypeface_Make_Fontations` (`WebFontTypefaceFactory`,
//! `web_font_typeface_factory.cc`): Skia's Fontations scaler reads the glyph
//! outlines with skrifa, hints them with skrifa, and rasterizes the resulting
//! paths. System faces keep FreeType. Div draws a web-font run through a
//! custom typeface whose glyphs are the same skrifa outlines, hinted with the
//! options `SkScalerContext_Fontations` derives from the resolved font
//! (`src/ports/SkTypeface_fontations.cpp` and
//! `src/ports/fontations/src/hinting.rs` in Skia), so Skia rasterizes the same
//! paths into the same masks.
//!
//! Building a custom typeface gives it a new Skia typeface id, which misses
//! Skia's glyph cache. The typefaces are therefore kept in a small process
//! cache:
//!
//! - **Key:** the resolved typeface's Skia id (each variable instance is its
//!   own typeface), its variation design coordinates, the physical text size,
//!   the resolved hinting and whether the mask is LCD.
//! - **Value:** the font data, the skrifa location and hinting instance, the
//!   hinted path of every glyph drawn so far, and a typeface holding those
//!   paths. A run that needs a glyph the typeface lacks rebuilds it once with
//!   the union of glyphs.
//! - **Eviction:** least recently used, at most [`ENTRY_CAPACITY`] entries.
//!   An entry whose glyph set would exceed [`GLYPH_CAPACITY`] restarts with
//!   only the glyphs of the current run.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use skia_safe::{
    font::Edging, utils::CustomTypefaceBuilder, Font, FontHinting, FontMetrics, Matrix, Path,
    Typeface,
};
use skrifa::{
    instance::{Location, LocationRef, Size},
    outline::{
        DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlineGlyphFormat,
        SmoothMode, Target,
    },
    FontRef, GlyphId, MetadataProvider,
};

use super::shape_result::SkiaPathPen;

/// Most typefaces kept at once.
pub(crate) const ENTRY_CAPACITY: usize = 32;
/// Most glyph paths one typeface keeps before it restarts.
pub(crate) const GLYPH_CAPACITY: usize = 4096;

/// Skia's `SK_MAX_SIZE_FOR_LCDTEXT`: above this device size, a glyph's LCD
/// mask becomes grayscale.
const MAX_LCD_TEXT_SIZE: f32 = 48.0;

/// Skia's `sk_relax`: a matrix entry rounded to 1/1024, so tiny changes in
/// the matrix give the same glyphs.
fn relax(value: f32) -> f32 {
    (value * 1024.0 + 0.5).floor() / 1024.0
}

/// How Skia's scaler sees a run drawn under `device`, the linear part of the
/// matrix from the run's physical-size glyphs to device pixels (`None` for
/// the identity), as `SkScalerContext::MakeRecAndEffects` records it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceRaster {
    /// The relaxed 2x2 (`fPost2x2`): scale x, skew x, skew y, scale y.
    post: [f32; 4],
    /// Whether the matrix has scale or skew bits (`checkPost2x2`).
    check_post: bool,
}

impl DeviceRaster {
    pub(crate) fn new(device: Option<&Matrix>) -> Self {
        let Some(device) = device else {
            return Self {
                post: [1.0, 0.0, 0.0, 1.0],
                check_post: false,
            };
        };
        let (scale_x, skew_x, skew_y, scale_y) = (
            device.scale_x(),
            device.skew_x(),
            device.skew_y(),
            device.scale_y(),
        );
        let scaled = scale_x != 1.0 || scale_y != 1.0;
        let affine = skew_x != 0.0 || skew_y != 0.0;
        Self {
            post: [
                if scaled { relax(scale_x) } else { 1.0 },
                if affine { relax(skew_x) } else { 0.0 },
                if affine { relax(skew_y) } else { 0.0 },
                if scaled { relax(scale_y) } else { 1.0 },
            ],
            check_post: scaled || affine,
        }
    }

    /// `too_big_for_lcd`: whether Skia masks a glyph of `size` in grayscale
    /// instead of LCD.
    pub(crate) fn too_big_for_lcd(self, size: f32) -> bool {
        if self.check_post {
            let [scale_x, skew_x, skew_y, scale_y] = self.post;
            let mut area = scale_x * scale_y - skew_y * skew_x;
            area *= size * size;
            area > MAX_LCD_TEXT_SIZE * MAX_LCD_TEXT_SIZE
        } else {
            size > MAX_LCD_TEXT_SIZE
        }
    }

    /// `isAxisAligned` in `SkTypeface_fontations.cpp`, for a font with
    /// `skew_x` (synthetic oblique): the Fontations scaler hints only
    /// axis-aligned glyphs.
    pub(crate) fn is_axis_aligned(self, skew_x: f32) -> bool {
        let [scale_x, post_skew_x, post_skew_y, scale_y] = self.post;
        skew_x == 0.0
            && ((post_skew_x == 0.0 && post_skew_y == 0.0) || (scale_x == 0.0 && scale_y == 0.0))
    }
}

/// The smooth target `make_hinting_instance` selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WebFontSmoothMode {
    Light,
    Normal,
    Lcd,
}

/// The hinting `SkScalerContext_Fontations` selects for a resolved font with
/// an antialiased (LCD or grayscale) mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WebFontHinting {
    /// `kNone`: the unhinted outline.
    None,
    /// A font that requires its TrueType instructions
    /// (`hinting_reliant`): strong (monochrome) hinting at every setting.
    Strong,
    /// Smooth hinting. `autohint` selects skrifa's automatic hinter
    /// (`Engine::Auto`) instead of `Engine::AutoFallback`.
    Smooth {
        mode: WebFontSmoothMode,
        autohint: bool,
    },
}

impl WebFontHinting {
    /// `hinting` is the resolved font's; `lcd` whether its mask is LCD;
    /// `glyf` whether the outlines are TrueType; `requires_interpreter`
    /// skrifa's `require_interpreter`; `force_autohint` the font's
    /// force-autohinting flag.
    pub(crate) fn select(
        hinting: FontHinting,
        lcd: bool,
        glyf: bool,
        requires_interpreter: bool,
        force_autohint: bool,
    ) -> Self {
        if requires_interpreter {
            return Self::Strong;
        }
        match hinting {
            FontHinting::None => Self::None,
            // kSlight forces the autohinter for glyf outlines; CFF keeps
            // its own hinter unless autohinting is forced for every format.
            FontHinting::Slight => Self::Smooth {
                mode: WebFontSmoothMode::Light,
                autohint: glyf || force_autohint,
            },
            FontHinting::Normal => Self::Smooth {
                mode: WebFontSmoothMode::Normal,
                autohint: force_autohint,
            },
            // onFilterRec collapses kFull to kNormal without an LCD mask.
            FontHinting::Full => Self::Smooth {
                mode: if lcd {
                    WebFontSmoothMode::Lcd
                } else {
                    WebFontSmoothMode::Normal
                },
                autohint: force_autohint,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OutlineKey {
    typeface: u32,
    coordinates: Vec<(u32, u32)>,
    size_bits: u32,
    hinting: FontHinting,
    lcd: bool,
    force_autohint: bool,
}

struct OutlineEntry {
    key: OutlineKey,
    data: Arc<Vec<u8>>,
    ttc_index: u32,
    location: Location,
    hinting: Option<HintingInstance>,
    paths: BTreeMap<u16, Path>,
    typeface: Option<Typeface>,
}

#[derive(Default)]
struct OutlineCache {
    // Least recently used first.
    entries: Vec<OutlineEntry>,
}

fn cache() -> &'static Mutex<OutlineCache> {
    static CACHE: OnceLock<Mutex<OutlineCache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The key of `source_font` drawn under `device`. Like the Fontations
/// scaler's record, it holds the mask Skia keeps (`lcd` is false above the
/// LCD size limit) and the hinting after `onFilterRec` (none unless the
/// glyphs are axis-aligned).
fn key(source_font: &Font, device: DeviceRaster) -> Option<OutlineKey> {
    let typeface = source_font.typeface();
    let coordinates = typeface
        .variation_design_position()?
        .iter()
        .map(|coordinate| (*coordinate.axis, coordinate.value.to_bits()))
        .collect();
    let size = source_font.size();
    Some(OutlineKey {
        typeface: typeface.unique_id().into(),
        coordinates,
        size_bits: size.to_bits(),
        hinting: if device.is_axis_aligned(source_font.skew_x()) {
            source_font.hinting()
        } else {
            FontHinting::None
        },
        lcd: source_font.edging() == Edging::SubpixelAntiAlias && !device.too_big_for_lcd(size),
        force_autohint: source_font.is_force_auto_hinting(),
    })
}

fn new_entry(source_font: &Font, key: OutlineKey) -> Option<OutlineEntry> {
    let typeface = source_font.typeface();
    let (data, ttc_index) = typeface.to_font_data()?;
    let ttc_index = ttc_index as u32;
    let font_ref = FontRef::from_index(&data, ttc_index).ok()?;
    let outlines = font_ref.outline_glyphs();
    // The resolved typeface includes authored variation settings and derived
    // weight, width, slant and optical size; its original bytes alone would
    // paint the default instance.
    let location = font_ref.axes().location(
        key.coordinates
            .iter()
            .map(|&(axis, value)| (skrifa::Tag::new(&axis.to_be_bytes()), f32::from_bits(value))),
    );
    let selected = WebFontHinting::select(
        key.hinting,
        key.lcd,
        matches!(outlines.format(), Some(OutlineGlyphFormat::Glyf)),
        outlines.require_interpreter(),
        key.force_autohint,
    );
    let size = Size::new(f32::from_bits(key.size_bits));
    let hinting = match selected {
        WebFontHinting::None => None,
        WebFontHinting::Strong => Some(
            HintingInstance::new(
                &outlines,
                size,
                LocationRef::from(&location),
                skrifa::outline::HintingMode::Strong,
            )
            .ok()?,
        ),
        WebFontHinting::Smooth { mode, autohint } => Some(
            HintingInstance::new(
                &outlines,
                size,
                LocationRef::from(&location),
                HintingOptions {
                    engine: if autohint {
                        Engine::Auto(Some(GlyphStyles::new(&outlines)))
                    } else {
                        Engine::AutoFallback
                    },
                    target: Target::Smooth {
                        mode: match mode {
                            WebFontSmoothMode::Light => SmoothMode::Light,
                            WebFontSmoothMode::Normal => SmoothMode::Normal,
                            WebFontSmoothMode::Lcd => SmoothMode::Lcd,
                        },
                        symmetric_rendering: true,
                        preserve_linear_metrics: false,
                    },
                },
            )
            .ok()?,
        ),
    };
    Some(OutlineEntry {
        key,
        data: Arc::new(data),
        ttc_index,
        location,
        hinting,
        paths: BTreeMap::new(),
        typeface: None,
    })
}

impl OutlineEntry {
    /// The typeface holding at least `glyphs`, drawing the missing ones.
    fn typeface_for(&mut self, glyphs: &[u16]) -> Option<Typeface> {
        let missing = glyphs
            .iter()
            .filter(|glyph| !self.paths.contains_key(glyph))
            .count();
        if missing == 0 {
            if let Some(typeface) = &self.typeface {
                return Some(typeface.clone());
            }
        }
        if self.paths.len() + missing > GLYPH_CAPACITY {
            self.paths.clear();
            self.typeface = None;
        }
        let font_ref = FontRef::from_index(&self.data, self.ttc_index).ok()?;
        let outlines = font_ref.outline_glyphs();
        let size = Size::new(f32::from_bits(self.key.size_bits));
        for &glyph_id in glyphs {
            if self.paths.contains_key(&glyph_id) {
                continue;
            }
            let glyph = outlines.get(GlyphId::new(u32::from(glyph_id)))?;
            let settings = match &self.hinting {
                Some(hinting) => DrawSettings::hinted(hinting, false),
                None => DrawSettings::unhinted(size, LocationRef::from(&self.location)),
            };
            let mut pen = SkiaPathPen::default();
            glyph.draw(settings, &mut pen).ok()?;
            self.paths.insert(glyph_id, pen.0.detach());
        }
        let mut builder = CustomTypefaceBuilder::new();
        builder.set_metrics(&FontMetrics::default(), 1.0);
        for (&glyph_id, path) in &self.paths {
            builder.set_glyph(glyph_id, 0.0, path);
        }
        let typeface = builder.detach()?;
        self.typeface = Some(typeface.clone());
        Some(typeface)
    }
}

/// A font whose typeface draws `glyphs` of `source_font` with the outlines,
/// hinting and mask Linux Chromium uses for a web font, at `source_font`'s
/// size (the physical text size), drawn under `device` (see
/// [`DeviceRaster::new`]). The font is unit-sized; positions are physical.
/// Skia sees only the unit size, so the mask and the hinting the Fontations
/// scaler derives from the size and the transform are chosen here:
/// grayscale above the LCD size limit (T5), and unhinted outlines unless the
/// glyphs are axis-aligned (T6).
/// `None` keeps the resolved font, for example for a glyph without an outline.
pub(crate) fn web_font(
    source_font: &Font,
    glyphs: &[u16],
    device: Option<&Matrix>,
) -> Option<Font> {
    let key = key(source_font, DeviceRaster::new(device))?;
    let lcd = key.lcd;
    let typeface = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .typeface(source_font, key, glyphs)?;
    let mut font = source_font.with_size(1.0)?;
    font.set_typeface(typeface);
    font.set_hinting(FontHinting::None);
    font.set_subpixel(source_font.is_subpixel());
    font.set_linear_metrics(source_font.is_linear_metrics());
    // The Fontations scaler drops `kGenA8FromLCD` (`onFilterRec`), so a mask
    // above the limit is plain grayscale coverage.
    font.set_edging(match source_font.edging() {
        Edging::SubpixelAntiAlias if !lcd => Edging::AntiAlias,
        edging => edging,
    });
    Some(font)
}

impl OutlineCache {
    /// The typeface drawing `glyphs` of `source_font`, from the entry for
    /// `key`, which becomes the most recently used.
    fn typeface(
        &mut self,
        source_font: &Font,
        key: OutlineKey,
        glyphs: &[u16],
    ) -> Option<Typeface> {
        let index = match self.entries.iter().position(|entry| entry.key == key) {
            Some(index) => index,
            None => {
                let entry = new_entry(source_font, key)?;
                if self.entries.len() >= ENTRY_CAPACITY {
                    self.entries.remove(0);
                }
                self.entries.push(entry);
                self.entries.len() - 1
            }
        };
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        self.entries.last_mut()?.typeface_for(glyphs)
    }

    #[cfg(test)]
    fn contents(&self) -> Vec<(OutlineKey, usize)> {
        self.entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.paths.len()))
            .collect()
    }
}

/// The process cache's keys, least recently used first, with each entry's
/// glyph count.
#[cfg(test)]
pub(crate) fn cache_contents() -> Vec<(OutlineKey, usize)> {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contents()
}

#[cfg(test)]
impl OutlineKey {
    pub(crate) fn typeface_id(&self) -> u32 {
        self.typeface
    }

    pub(crate) fn size(&self) -> f32 {
        f32::from_bits(self.size_bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hinting_follows_skias_fontations_scaler() {
        use WebFontSmoothMode::{Lcd, Light, Normal};
        let smooth = |mode, autohint| WebFontHinting::Smooth { mode, autohint };
        let select = WebFontHinting::select;
        // (hinting, lcd, glyf, requires interpreter, force autohint)
        assert_eq!(
            select(FontHinting::None, true, true, false, false),
            WebFontHinting::None
        );
        assert_eq!(
            select(FontHinting::Slight, true, true, false, false),
            smooth(Light, true)
        );
        assert_eq!(
            select(FontHinting::Slight, false, false, false, false),
            smooth(Light, false)
        );
        assert_eq!(
            select(FontHinting::Slight, false, false, false, true),
            smooth(Light, true)
        );
        assert_eq!(
            select(FontHinting::Normal, true, true, false, false),
            smooth(Normal, false)
        );
        assert_eq!(
            select(FontHinting::Normal, true, false, false, true),
            smooth(Normal, true)
        );
        assert_eq!(
            select(FontHinting::Full, true, true, false, false),
            smooth(Lcd, false)
        );
        assert_eq!(
            select(FontHinting::Full, false, true, false, false),
            smooth(Normal, false)
        );
        for hinting in [
            FontHinting::None,
            FontHinting::Slight,
            FontHinting::Normal,
            FontHinting::Full,
        ] {
            assert_eq!(
                select(hinting, true, true, true, false),
                WebFontHinting::Strong
            );
        }
    }

    fn dejavu_typeface() -> Typeface {
        skia_safe::FontMgr::default()
            .new_from_data(include_bytes!("../../fonts/DejaVuSans.ttf"), None)
            .unwrap()
    }

    fn font(typeface: &Typeface, size: f32) -> Font {
        let mut font = Font::from_typeface(typeface.clone(), size);
        font.set_edging(Edging::SubpixelAntiAlias);
        font.set_hinting(FontHinting::Slight);
        font
    }

    // Unhinted, so the eviction test does not analyse the autohinter's glyph
    // styles for each of its typefaces.
    fn dejavu(size: f32) -> Font {
        let mut font = font(&dejavu_typeface(), size);
        font.set_hinting(FontHinting::None);
        font
    }

    fn identity() -> DeviceRaster {
        DeviceRaster::new(None)
    }

    fn typeface(cache: &mut OutlineCache, font: &Font, glyphs: &[u16]) -> Option<Typeface> {
        cache.typeface(font, key(font, identity())?, glyphs)
    }

    #[test]
    fn lcd_masks_stop_at_skias_size_limit() {
        // Without a scale or skew, Skia compares the size itself.
        assert!(!identity().too_big_for_lcd(48.0));
        assert!(identity().too_big_for_lcd(48.001));
        // Under one, it compares the area the matrix gives the glyph.
        let half = DeviceRaster::new(Some(&Matrix::scale((0.5, 0.5))));
        assert!(!half.too_big_for_lcd(96.0));
        assert!(half.too_big_for_lcd(96.01));
        let stretched = DeviceRaster::new(Some(&Matrix::scale((2.0, 0.5))));
        assert!(!stretched.too_big_for_lcd(48.0));
        assert!(stretched.too_big_for_lcd(48.01));
        // A rotation keeps the area.
        let rotated = DeviceRaster::new(Some(&Matrix::rotate_deg(30.0)));
        assert!(!rotated.too_big_for_lcd(47.9));
        assert!(rotated.too_big_for_lcd(48.1));
        // A translation is the identity.
        let moved = DeviceRaster::new(Some(&Matrix::translate((3.5, 7.25))));
        assert_eq!(moved, identity());
    }

    #[test]
    fn only_axis_aligned_glyphs_are_hinted() {
        assert!(identity().is_axis_aligned(0.0));
        // Synthetic oblique skews the font itself.
        assert!(!identity().is_axis_aligned(-0.25));
        assert!(DeviceRaster::new(Some(&Matrix::scale((2.0, 0.5)))).is_axis_aligned(0.0));
        assert!(!DeviceRaster::new(Some(&Matrix::rotate_deg(30.0))).is_axis_aligned(0.0));
        assert!(!DeviceRaster::new(Some(&Matrix::skew((0.3, 0.0)))).is_axis_aligned(0.0));
        // A quarter turn swaps the axes and stays aligned; Skia relaxes the
        // matrix's rounding error (cos 90 is not exactly 0 in f32).
        assert!(DeviceRaster::new(Some(&Matrix::rotate_deg(90.0))).is_axis_aligned(0.0));
        assert!(DeviceRaster::new(Some(&Matrix::rotate_deg(-90.0))).is_axis_aligned(0.0));
        // Below 1/2048 an entry relaxes to 0.
        let tiny = Matrix::new_all(1.0, 0.0004, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
        assert!(DeviceRaster::new(Some(&tiny)).is_axis_aligned(0.0));
    }

    #[test]
    fn the_key_holds_the_mask_and_hinting_skia_keeps() {
        let typeface = dejavu_typeface();
        let lcd = key(&font(&typeface, 48.0), identity()).unwrap();
        assert!(lcd.lcd);
        assert_eq!(lcd.hinting, FontHinting::Slight);
        let large = key(&font(&typeface, 48.5), identity()).unwrap();
        assert!(!large.lcd);
        assert_eq!(large.hinting, FontHinting::Slight);
        let rotated = key(
            &font(&typeface, 16.0),
            DeviceRaster::new(Some(&Matrix::rotate_deg(30.0))),
        )
        .unwrap();
        assert!(rotated.lcd);
        assert_eq!(rotated.hinting, FontHinting::None);
        let mut oblique = font(&typeface, 16.0);
        oblique.set_skew_x(-0.25);
        assert_eq!(
            key(&oblique, identity()).unwrap().hinting,
            FontHinting::None
        );
    }

    #[test]
    fn web_fonts_above_the_lcd_limit_draw_grayscale() {
        let typeface = dejavu_typeface();
        let small = web_font(&font(&typeface, 48.0), &[36], None).unwrap();
        assert_eq!(small.edging(), Edging::SubpixelAntiAlias);
        let large = web_font(&font(&typeface, 48.5), &[36], None).unwrap();
        assert_eq!(large.edging(), Edging::AntiAlias);
        let shrunk = Matrix::scale((0.5, 0.5));
        let small_on_device = web_font(&font(&typeface, 96.0), &[36], Some(&shrunk)).unwrap();
        assert_eq!(small_on_device.edging(), Edging::SubpixelAntiAlias);
        let mut gray = font(&typeface, 16.0);
        gray.set_edging(Edging::AntiAlias);
        assert_eq!(
            web_font(&gray, &[36], None).unwrap().edging(),
            Edging::AntiAlias
        );
    }

    #[test]
    fn the_cache_evicts_the_least_recently_used_typeface() {
        let mut cache = OutlineCache::default();
        let first = dejavu(10.0);
        typeface(&mut cache, &first, &[36]).unwrap();
        for index in 1..ENTRY_CAPACITY {
            typeface(&mut cache, &dejavu(10.0 + index as f32), &[36]).unwrap();
        }
        assert_eq!(cache.entries.len(), ENTRY_CAPACITY);
        // Using the first entry again protects it; the second is evicted.
        typeface(&mut cache, &first, &[37]).unwrap();
        typeface(&mut cache, &dejavu(100.0), &[36]).unwrap();
        let sizes: Vec<f32> = cache.contents().iter().map(|(key, _)| key.size()).collect();
        assert_eq!(sizes.len(), ENTRY_CAPACITY);
        assert!(sizes.contains(&10.0));
        assert!(!sizes.contains(&11.0));
        assert_eq!(sizes.last(), Some(&100.0));
    }

    #[test]
    fn a_typeface_grows_by_union_and_restarts_past_its_glyph_capacity() {
        let mut cache = OutlineCache::default();
        let font = dejavu(13.0);
        let first = typeface(&mut cache, &font, &[36, 37]).unwrap();
        // The same glyphs reuse the typeface, keeping Skia's glyph cache.
        let again = typeface(&mut cache, &font, &[37]).unwrap();
        assert_eq!(first.unique_id(), again.unique_id());
        // A new glyph rebuilds it once with the union.
        let grown = typeface(&mut cache, &font, &[38]).unwrap();
        assert_ne!(first.unique_id(), grown.unique_id());
        assert_eq!(cache.contents()[0].1, 3);
        let glyphs: Vec<u16> = (1..=GLYPH_CAPACITY as u16).collect();
        typeface(&mut cache, &font, &glyphs).unwrap();
        assert_eq!(cache.contents()[0].1, GLYPH_CAPACITY);
        typeface(&mut cache, &font, &[36]).unwrap();
        assert_eq!(cache.contents()[0].1, GLYPH_CAPACITY);
        let beyond = GLYPH_CAPACITY as u16 + 1;
        typeface(&mut cache, &font, &[beyond, beyond + 1]).unwrap();
        assert_eq!(cache.contents()[0].1, 2);
    }

    #[test]
    fn the_key_separates_size_hinting_mask_and_instance() {
        let typeface = dejavu_typeface();
        let base = key(&font(&typeface, 13.0), identity()).unwrap();
        assert_eq!(base, key(&font(&typeface, 13.0), identity()).unwrap());
        assert_ne!(base, key(&font(&typeface, 13.5), identity()).unwrap());
        let mut hinting = font(&typeface, 13.0);
        hinting.set_hinting(FontHinting::Normal);
        assert_ne!(base, key(&hinting, identity()).unwrap());
        let mut gray = font(&typeface, 13.0);
        gray.set_edging(Edging::AntiAlias);
        assert_ne!(base, key(&gray, identity()).unwrap());
        let mut forced = font(&typeface, 13.0);
        forced.set_force_auto_hinting(true);
        assert_ne!(base, key(&forced, identity()).unwrap());
        // Each typeface, and so each variable instance, is its own key.
        assert_ne!(
            base,
            key(&font(&dejavu_typeface(), 13.0), identity()).unwrap()
        );
    }

    mod routing {
        use std::sync::Arc;

        use div_geometry::{RasterConfiguration, TextEdging};
        use div_style::{FontFamily, FontFamilyList};

        use super::super::cache_contents;
        use crate::font::{Font, FontCollection, FontDescription, FontFaceDescriptor};
        use crate::shaping::{ShapeResult, TextDirection, TextRasterPolicy, TextShaper};

        const AHEM: &[u8] = include_bytes!("../../fonts/Ahem.ttf");
        const DEJAVU: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");

        fn font(
            collection: &Arc<FontCollection>,
            family: &str,
            configuration: RasterConfiguration,
        ) -> Font {
            let mut description = FontDescription::default();
            description.raster_configuration = configuration;
            description.family =
                FontFamilyList::new([FontFamily::Named(family.to_owned())]).unwrap();
            description.size = 16.0;
            description.specified_size = 16.0;
            Font::new_in_collection(description, Arc::clone(collection))
        }

        fn web_collection(family: &str, bytes: &[u8]) -> Arc<FontCollection> {
            let collection = FontCollection::system();
            collection
                .register(Arc::from(bytes), FontFaceDescriptor::new(family))
                .unwrap();
            collection
        }

        fn is_web_font(font: &Font) -> bool {
            font.primary_font()
                .unwrap()
                .source_info()
                .application_face()
                .unwrap()
                .is_web_font()
        }

        fn primary_typeface_id(font: &Font) -> u32 {
            font.primary_font().unwrap().typeface().unique_id().into()
        }

        fn cached_sizes(typeface: u32) -> Vec<f32> {
            cache_contents()
                .into_iter()
                .filter(|(key, _)| key.typeface_id() == typeface)
                .map(|(key, _)| key.size())
                .collect()
        }

        fn render(blob: &skia_safe::TextBlob) -> Vec<u8> {
            let mut surface = skia_safe::surfaces::raster_n32_premul((160, 48)).unwrap();
            surface.canvas().clear(skia_safe::Color::WHITE);
            let mut paint = skia_safe::Paint::default();
            paint.set_color(skia_safe::Color::BLACK);
            surface.canvas().draw_text_blob(blob, (4.0, 32.0), &paint);
            let info = surface.image_info();
            let mut pixels = vec![0; info.compute_min_byte_size()];
            assert!(surface.read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0)));
            pixels
        }

        #[test]
        fn registered_faces_are_web_fonts_and_the_test_collection_is_not() {
            let configuration = RasterConfiguration::chromium_linux_lcd();
            let web = web_collection("Web Sans", DEJAVU);
            assert!(is_web_font(&font(&web, "Web Sans", configuration)));
            let census = FontCollection::deterministic_test();
            assert!(!is_web_font(&font(&census, "DejaVu Sans", configuration)));
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn linux_web_font_runs_use_physical_fontations_outlines() {
            let configuration = RasterConfiguration::chromium_linux_lcd();
            let shaper = TextShaper::new();
            let web = web_collection("Web Sans", DEJAVU);
            let web_font = font(&web, "Web Sans", configuration);
            let shaped = shaper.shape("Hamburgefonstiv", &web_font, TextDirection::Ltr);
            for scale in [1.0, 1.25, 2.0] {
                shaped
                    .to_text_blob_with_raster_policy_at_scale(
                        Some(4.0),
                        TextRasterPolicy::Skia,
                        scale,
                    )
                    .unwrap();
            }
            // Hinted at the physical text size, as Chromium hints the strike.
            let sizes = cached_sizes(primary_typeface_id(&web_font));
            for size in [16.0, 20.0, 32.0] {
                assert!(sizes.contains(&size), "{size} not in {sizes:?}");
            }
            // The census's stand-in for system fonts keeps FreeType.
            let census = FontCollection::deterministic_test();
            let census_font = font(&census, "DejaVu Sans", configuration);
            let shaped = shaper.shape("Hamburgefonstiv", &census_font, TextDirection::Ltr);
            shaped
                .to_text_blob_with_raster_policy_at_scale(Some(4.0), TextRasterPolicy::Skia, 1.0)
                .unwrap();
            assert!(cached_sizes(primary_typeface_id(&census_font)).is_empty());
        }

        // The aliased and LCD Ahem paths (`fontations_compatible_font`) do not
        // depend on provenance: Ahem registered as a web font and as the
        // census's system stand-in draws the same pixels under each of those
        // policies, and an aliased Ahem never reaches the web-font cache.
        #[test]
        fn ahem_policies_are_unchanged_by_web_font_provenance() {
            let shaper = TextShaper::new();
            let web = web_collection("Ahem", AHEM);
            let census = FontCollection::deterministic_test();
            for configuration in [
                RasterConfiguration::chromium_linux_monochrome(),
                RasterConfiguration::chromium_linux_lcd(),
            ] {
                let web_font = font(&web, "Ahem", configuration);
                let census_font = font(&census, "Ahem", configuration);
                assert!(is_web_font(&web_font));
                assert!(!is_web_font(&census_font));
                let text = "\u{c9}p X\u{c9}";
                let web_shaped = shaper.shape(text, &web_font, TextDirection::Ltr);
                let census_shaped = shaper.shape(text, &census_font, TextDirection::Ltr);
                for policy in [
                    TextRasterPolicy::ChromiumAuthorAliased,
                    TextRasterPolicy::ChromiumAuthorLcd,
                    TextRasterPolicy::ChromiumAliased,
                ] {
                    for scale in [1.0, 1.25, 2.0] {
                        let blob = |shaped: &ShapeResult| {
                            shaped
                                .to_text_blob_with_raster_policy_at_scale(Some(4.0), policy, scale)
                                .unwrap()
                        };
                        assert_eq!(
                            render(&blob(&web_shaped)),
                            render(&blob(&census_shaped)),
                            "{configuration:?} {policy:?} {scale}"
                        );
                    }
                }
                if configuration.author_text.edging == TextEdging::Alias {
                    let shaped = &web_shaped;
                    shaped
                        .to_text_blob_with_raster_policy_at_scale(
                            Some(4.0),
                            TextRasterPolicy::Skia,
                            1.0,
                        )
                        .unwrap();
                    assert!(cached_sizes(primary_typeface_id(&web_font)).is_empty());
                }
                assert!(cached_sizes(primary_typeface_id(&census_font)).is_empty());
            }
        }
    }
}
