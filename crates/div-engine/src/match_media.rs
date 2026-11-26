//! `matchMedia()` lists (RFC 0014, step 3): the engine side.
//!
//! A list is registered by [`Engine::match_media`] and stays registered
//! until [`Engine::release_media_query_list`]. The framework's frame loop
//! calls [`Engine::evaluate_media_queries_and_report_changes`] at the HTML
//! "update the rendering" step of that name and fires `change` events for
//! the lists it returns.
//!
//! The timing follows pinned Chromium 147 as `match-media-v1` records it
//! (`tests/fidelity/fixtures/match-media-v1`, scripted sequences over CDP):
//!
//! - Each list caches whether it matches. Reading `matches` evaluates the
//!   list against the current inputs and updates the cache.
//! - A viewport change and a reduced-motion change evaluate every list at
//!   once. A list whose cached result changes is queued for the next
//!   report, once, however often it flips.
//! - A color scheme change waits for the report. A read before the report
//!   updates the cache, so if it already saw the new result, no change is
//!   reported.
//! - A device scale factor change goes through the viewport and behaves the
//!   same way (pinned with CDP-emulated scale changes).
//! - A list's baseline is its result at creation, so a list created after a
//!   change and before the report has nothing to report.
//! - The report evaluates every list, queues those whose cache changed, and
//!   returns the queued lists in creation order, each with its current
//!   result, which is recorded as reported.
//!
//! Nothing inside the engine reads a list between an input change and the
//! report: only a caller's read may move a list's cache.

use crate::{Engine, EngineError};
use div_style::media_query::{parse_media_query_list, MediaQueryListData, MediaValues};

/// A list created by [`Engine::match_media`]. Ids are never reused, and
/// they increase in creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaQueryListId(u64);

#[derive(Debug)]
struct Entry {
    id: MediaQueryListId,
    list: MediaQueryListData,
    media: String,
    matches: bool,
    queued: bool,
}

/// Every live `matchMedia()` list of a document, in creation order.
#[derive(Debug, Default)]
pub(crate) struct MatchMediaLists {
    entries: Vec<Entry>,
    next_id: u64,
}

impl MatchMediaLists {
    fn index(&self, id: MediaQueryListId) -> Result<usize, EngineError> {
        self.entries
            .binary_search_by_key(&id, |entry| entry.id)
            .map_err(|_| EngineError::StaleHandle)
    }

    /// Evaluate every list; queue those whose cached result changed.
    pub(crate) fn evaluate(&mut self, values: &MediaValues) {
        for entry in &mut self.entries {
            let matches = entry.list.matches(values);
            if matches != entry.matches {
                entry.matches = matches;
                entry.queued = true;
            }
        }
    }
}

impl Engine {
    /// `window.matchMedia(query)`: register a list for `query`. Text that
    /// is not a valid media query list is `not all`, as in Chromium, so
    /// this never fails.
    pub fn match_media(&mut self, query: &str) -> MediaQueryListId {
        let list = parse_media_query_list(query).list;
        let media = list.to_string();
        let matches = list.matches(&self.media_values());
        let lists = &mut self.match_media_lists;
        let id = MediaQueryListId(lists.next_id);
        lists.next_id += 1;
        lists.entries.push(Entry {
            id,
            list,
            media,
            matches,
            queued: false,
        });
        id
    }

    /// The list's `media`: its query, serialized as CSSOM specifies and
    /// Chromium writes it.
    pub fn media_query_list_media(&self, id: MediaQueryListId) -> Result<&str, EngineError> {
        let lists = &self.match_media_lists;
        Ok(&lists.entries[lists.index(id)?].media)
    }

    /// The list's `matches`: whether it matches the current inputs. As in
    /// Chromium, the read updates the list's cached result, so a change it
    /// has already seen is not reported again. After a color scheme change,
    /// a read before the next report means no change is reported for this
    /// list.
    pub fn media_query_list_matches(&mut self, id: MediaQueryListId) -> Result<bool, EngineError> {
        let values = self.media_values();
        let lists = &mut self.match_media_lists;
        let index = lists.index(id)?;
        let entry = &mut lists.entries[index];
        entry.matches = entry.list.matches(&values);
        Ok(entry.matches)
    }

    /// Unregister a list. Its pending change, if any, is dropped.
    pub fn release_media_query_list(&mut self, id: MediaQueryListId) -> Result<(), EngineError> {
        let lists = &mut self.match_media_lists;
        let index = lists.index(id)?;
        lists.entries.remove(index);
        Ok(())
    }

    /// The HTML "evaluate media queries and report changes" step: the lists
    /// whose result changed since it was last reported or read, oldest
    /// first, each with its result now, which the caller gives the `change`
    /// event as its `matches`.
    pub fn evaluate_media_queries_and_report_changes(
        &mut self,
    ) -> Vec<(MediaQueryListId, bool)> {
        let values = self.media_values();
        let lists = &mut self.match_media_lists;
        lists.evaluate(&values);
        lists
            .entries
            .iter_mut()
            .filter_map(|entry| {
                std::mem::take(&mut entry.queued).then_some((entry.id, entry.matches))
            })
            .collect()
    }

    /// A viewport or reduced-motion change: Chromium evaluates its lists at
    /// once (see the module documentation).
    pub(crate) fn evaluate_match_media_lists(&mut self) {
        if self.match_media_lists.entries.is_empty() {
            return;
        }
        let values = self.media_values();
        self.match_media_lists.evaluate(&values);
    }
}
