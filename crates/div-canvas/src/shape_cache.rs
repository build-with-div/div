//! The canvas host's text caches, ported from Blink at Chromium 147.0.7727.50
//! (`core/html/canvas/unique_font_selector.cc`,
//! `platform/fonts/plain_text_painter.cc` and
//! `platform/fonts/shaping/frame_shape_cache.cc`; Chromium BSD licence, see
//! `THIRD_PARTY.md`).
//!
//! A canvas resolves one `Font` per font description (`UniqueFontSelector`),
//! and its `PlainTextPainter` keeps one `FrameShapeCache` per font. The cache
//! maps a whole text to its `PlainTextNode` and each word to its shape result
//! and ink bounds, keyed by text and direction only. The reuse shows: a word
//! keeps the spacing it was shaped with at its first position, so a leading
//! space measured after the same space inside a text carries word spacing.
//!
//! Entries live for the frame that last used them; [`ShapeCaches::did_switch_frame`]
//! drops the older ones, as `CanvasRenderingContextHost::NotifyCachesOfSwitchingFrame`
//! does. Entries made before the first frame switch stay.

use std::collections::HashMap;
use std::sync::Arc;

use div_text::TextDirection;

use crate::text::{segment, Segment, TextItem, TextNode};

/// `CanvasFontCache::MaxFonts` (not a low-end device, memory ratio 1).
const MAX_FONTS: usize = 50;
/// `kMaximumNodeCacheEntries`.
const MAXIMUM_NODE_CACHE_ENTRIES: usize = 32768;
/// `kMaximumShapeCacheEntries`.
const MAXIMUM_SHAPE_CACHE_ENTRIES: usize = 32768;
/// `FrameShapeCache::kInitialFrame`.
const INITIAL_FRAME: u32 = 0;

type Key = (String, bool);

fn key(text: &str, direction: TextDirection) -> Key {
    (text.to_owned(), direction.is_rtl())
}

/// A map with Blink's LRU list: each entry records the frame that last used
/// it and when, most recent first.
struct LruMap<V> {
    map: HashMap<Key, (V, u32, u64)>,
    clock: u64,
}

impl<V: Clone> LruMap<V> {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            clock: 0,
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// `FindOrCreateEntry` on a hit: the entry moves to the front, in this
    /// frame.
    fn find(&mut self, key: &Key, generation: u32) -> Option<V> {
        let now = self.tick();
        let (value, entry_generation, last_use) = self.map.get_mut(key)?;
        *entry_generation = generation;
        *last_use = now;
        Some(value.clone())
    }

    /// `Register*Entry` and `LimitCacheSize`: past `limit` entries, the least
    /// recently used go until half remain.
    fn register(&mut self, key: Key, value: V, generation: u32, limit: usize) {
        let now = self.tick();
        self.map.insert(key, (value, generation, now));
        if self.map.len() <= limit {
            return;
        }
        let mut order: Vec<(u64, Key)> = self
            .map
            .iter()
            .map(|(key, &(_, _, last_use))| (last_use, key.clone()))
            .collect();
        order.sort_unstable_by_key(|&(last_use, _)| last_use);
        let excess = self.map.len() - limit / 2;
        for (_, key) in order.into_iter().take(excess) {
            self.map.remove(&key);
        }
    }

    /// `RemoveOldEntries`: keeps the entries of the initial frame and of the
    /// frame that is ending.
    fn remove_old(&mut self, generation: u32) {
        self.map.retain(|_, &mut (_, entry_generation, _)| {
            entry_generation == INITIAL_FRAME || entry_generation == generation
        });
    }
}

/// `FrameShapeCache`: one font's text nodes and shaped words.
struct FrameShapeCache {
    nodes: LruMap<Arc<TextNode>>,
    shapes: LruMap<Arc<TextItem>>,
    generation: u32,
    added_new_entries: bool,
}

impl FrameShapeCache {
    fn new() -> Self {
        Self {
            nodes: LruMap::new(),
            shapes: LruMap::new(),
            generation: INITIAL_FRAME,
            added_new_entries: false,
        }
    }

    fn did_switch_frame(&mut self) {
        if !self.added_new_entries {
            return;
        }
        self.added_new_entries = false;
        if self.generation > INITIAL_FRAME {
            self.nodes.remove_old(self.generation);
            self.shapes.remove_old(self.generation);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == INITIAL_FRAME {
            self.generation += 1;
        }
    }

    fn register_shape(&mut self, item: Arc<TextItem>) {
        self.added_new_entries = true;
        let key = key(&item.text, item.direction);
        self.shapes
            .register(key, item, self.generation, MAXIMUM_SHAPE_CACHE_ENTRIES);
    }

    /// `FindOrCreateShapeEntry`, then `RegisterShapeEntry` on a miss.
    fn shape(
        &mut self,
        text: &str,
        direction: TextDirection,
        make: impl FnOnce() -> TextItem,
    ) -> Arc<TextItem> {
        if let Some(item) = self.shapes.find(&key(text, direction), self.generation) {
            return item;
        }
        let item = Arc::new(make());
        self.register_shape(item.clone());
        item
    }
}

/// The fonts a canvas resolved (`UniqueFontSelector`), each with its
/// `FrameShapeCache`.
pub(crate) struct ShapeCaches {
    fonts: HashMap<String, (FrameShapeCache, u32, u64)>,
    clock: u64,
    generation: u32,
}

impl ShapeCaches {
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            clock: 0,
            generation: 0,
        }
    }

    /// `PlainTextPainter::DidSwitchFrame` and
    /// `UniqueFontSelector::DidSwitchFrame`.
    pub fn did_switch_frame(&mut self) {
        for (cache, _, _) in self.fonts.values_mut() {
            cache.did_switch_frame();
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// `UniqueFontSelector::FindOrCreateFont`: the cache of the font for
    /// `description_key`. A font evicted past the size limit is a new font
    /// when it comes back, with an empty cache.
    fn font(&mut self, description_key: &str) -> &mut FrameShapeCache {
        self.clock += 1;
        let now = self.clock;
        if let Some(entry) = self.fonts.get_mut(description_key) {
            entry.1 = self.generation;
            entry.2 = now;
        } else {
            self.fonts.insert(
                description_key.to_owned(),
                (FrameShapeCache::new(), self.generation, now),
            );
            self.evict_excess_fonts(description_key);
        }
        &mut self
            .fonts
            .get_mut(description_key)
            .expect("font just used")
            .0
    }

    /// `UniqueFontSelector::EvictExcessEntries`: least recently used first;
    /// fonts used in this frame may take the cache up to twice its limit.
    fn evict_excess_fonts(&mut self, keep: &str) {
        while self.fonts.len() > MAX_FONTS {
            let Some((oldest, &(_, generation, _))) = self
                .fonts
                .iter()
                .filter(|(key, _)| key.as_str() != keep)
                .min_by_key(|(_, &(_, _, last_use))| last_use)
            else {
                return;
            };
            if generation == self.generation && self.fonts.len() <= MAX_FONTS * 2 {
                return;
            }
            let oldest = oldest.clone();
            self.fonts.remove(&oldest);
        }
    }

    /// `PlainTextPainter::CreateNode`: the cached node for `text`, or a new
    /// one (`PlainTextNode::SegmentText` and `Shape`) whose words come from
    /// the cache when it has them and from `shape_item` when it does not.
    pub fn node(
        &mut self,
        description_key: &str,
        text: &str,
        direction: TextDirection,
        shape_item: impl Fn(Segment) -> TextItem,
    ) -> Arc<TextNode> {
        let cache = self.font(description_key);
        let node_key = key(text, direction);
        if let Some(node) = cache.nodes.find(&node_key, cache.generation) {
            // `FindOrCreateNodeEntry` touches the node's words.
            for item in &node.items {
                if cache
                    .shapes
                    .find(&key(&item.text, item.direction), cache.generation)
                    .is_none()
                {
                    cache.register_shape(item.clone());
                }
            }
            return node;
        }
        let (segments, base_rtl) = segment(text, direction);
        let items = segments
            .into_iter()
            .map(|segment| {
                let word = segment.text.clone();
                cache.shape(&word, segment.direction, || shape_item(segment))
            })
            .collect();
        let node = Arc::new(TextNode { items, base_rtl });
        cache.added_new_entries = true;
        cache.nodes.register(
            node_key,
            node.clone(),
            cache.generation,
            MAXIMUM_NODE_CACHE_ENTRIES,
        );
        node
    }
}
