//! Partial raster (RFC 0006 A2).
//!
//! A [`RetainedRaster`] keeps the last frame's tiles and, for the next
//! recording, rasters again only the tiles whose visible rect meets the
//! damage. Every physical pixel belongs to exactly one tile, and a tile is
//! always rastered whole from the new recording, so a damaged tile ends with
//! the pixels a full raster gives it. An undamaged tile keeps the previous
//! frame's pixels, which equal the full raster's wherever the damage covers
//! every changed pixel (RFC 0006 A1's damage gate). A recording that rasters
//! untiled is rastered whole, as is any change of viewport, scale, raster
//! configuration or raster strategy.
//!
//! Viewport controls are drawn over the tiles, so when a recording has them
//! the tiles are kept in their own surface and the controls are composed
//! over a copy each frame.

use skia_safe::{IRect, Surface};

use crate::render::{
    compose_viewport_overlay, raster_size, rasterize_picture, replay_tiles, replays_untiled,
    viewport_surface,
};
use crate::RecordedPicture;

/// What [`RetainedRaster::update`] rastered.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterUpdate {
    /// The tiles rastered again, or `None` after a full raster.
    pub tiles: Option<usize>,
    /// The physical rects whose pixels can differ from the previous frame,
    /// or `None` when any pixel can.
    pub changed: Option<Vec<IRect>>,
}

impl RasterUpdate {
    fn full() -> Self {
        Self {
            tiles: None,
            changed: None,
        }
    }
}

/// The recording properties a retained raster depends on. A change to any
/// of them rasters the next frame in full.
#[derive(Clone, Copy, PartialEq)]
struct RasterKey {
    viewport: div_geometry::ViewportMetrics,
    raster_configuration: div_geometry::RasterConfiguration,
    lcd_surface: bool,
    direct_replay: bool,
    content_requires_tiling: bool,
}

impl RasterKey {
    fn of(recording: &RecordedPicture) -> Self {
        Self {
            viewport: recording.viewport,
            raster_configuration: recording.raster_configuration,
            lcd_surface: recording.lcd_surface,
            direct_replay: recording.direct_replay,
            content_requires_tiling: recording.content_requires_tiling,
        }
    }
}

/// The raster of the last frame, kept so the next frame rasters only its
/// damaged tiles.
#[doc(hidden)]
pub struct RetainedRaster {
    key: RasterKey,
    untiled: bool,
    /// The document tiles; with no viewport controls, the whole frame.
    content: Surface,
    /// The tiles with the viewport controls composed over them.
    composed: Option<Surface>,
}

impl RetainedRaster {
    /// Rasters `recording` in full.
    pub fn new(recording: &RecordedPicture) -> Result<Self, String> {
        crate::initialize_skia();
        let key = RasterKey::of(recording);
        if replays_untiled(recording)? {
            return Ok(Self {
                key,
                untiled: true,
                content: rasterize_picture(recording)?,
                composed: None,
            });
        }
        let mut content = viewport_surface(recording)?;
        replay_tiles(recording, &mut content, &|_| true, false)?;
        let mut raster = Self {
            key,
            untiled: false,
            content,
            composed: None,
        };
        raster.compose(recording)?;
        Ok(raster)
    }

    /// Rasters `recording`, which follows the last recording, given the
    /// physical rects that can differ between the two. Only the tiles that
    /// meet `damage` are rastered again.
    pub fn update(
        &mut self,
        recording: &RecordedPicture,
        damage: &[IRect],
    ) -> Result<RasterUpdate, String> {
        if RasterKey::of(recording) != self.key || self.untiled {
            *self = Self::new(recording)?;
            return Ok(RasterUpdate::full());
        }
        let damage: Vec<IRect> = damage.iter().copied().filter(|r| !r.is_empty()).collect();
        let drawn = if damage.is_empty() {
            Vec::new()
        } else {
            replay_tiles(
                recording,
                &mut self.content,
                &|tile| damage.iter().any(|rect| overlaps(&tile, rect)),
                true,
            )?
        };
        let tiles = drawn.len();
        let closed = self.compose(recording)?;
        let changed = match (&self.composed, closed) {
            (Some(_), _) => None,
            (None, true) => {
                let (width, height, _) = raster_size(recording)?;
                let mut changed = drawn;
                changed.push(IRect::from_xywh(0, height - 1, width, 1));
                Some(changed)
            }
            (None, false) => Some(drawn),
        };
        Ok(RasterUpdate {
            tiles: Some(tiles),
            changed,
        })
    }

    /// The frame's pixels.
    pub fn surface(&mut self) -> &mut Surface {
        self.composed.as_mut().unwrap_or(&mut self.content)
    }

    /// Draws the viewport controls and the scanline closure, over a copy of
    /// the tiles when the recording has controls. Returns whether the last
    /// scanline was rewritten in the tile surface itself.
    fn compose(&mut self, recording: &RecordedPicture) -> Result<bool, String> {
        if recording.viewport_overlay.is_none() {
            self.composed = None;
            return Ok(compose_viewport_overlay(recording, &mut self.content));
        }
        let mut composed = match self.composed.take() {
            Some(surface) => surface,
            None => viewport_surface(recording)?,
        };
        let pixels = self
            .content
            .peek_pixels()
            .ok_or_else(|| "tile surface has no pixels".to_string())?;
        // Both surfaces have the same image info, so this copies the bytes.
        composed.write_pixels_from_pixmap(&pixels, (0, 0));
        compose_viewport_overlay(recording, &mut composed);
        self.composed = Some(composed);
        Ok(false)
    }
}

fn overlaps(a: &IRect, b: &IRect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}
