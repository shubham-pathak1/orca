use orca_services::types::Query;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Navigation {
    pub order: Vec<String>,
    /// Song IDs retain the collection order without duplicating file paths.
    pub order_ids: Arc<Vec<i64>>,
    pub context_query: Option<Query>,
    pub order_cursor: Option<usize>,
    pub removed_ids: HashSet<i64>,
    pub played_ids: HashSet<i64>,
    pub order_cleared: bool,
    pub manual: Vec<String>,
    pub removed: HashSet<String>,
    pub played: HashSet<String>,
    pub history: Vec<String>,
    pub cursor: Option<usize>,
    pub shuffle: bool,
    pub repeat: u8,
    seed: u64,
}
impl Navigation {
    pub fn compact_context(&mut self, ids: Vec<i64>, query: Query, current_id: i64, current: &str) {
        self.order.clear();
        self.order_ids = Arc::new(ids);
        self.context_query = Some(query);
        self.order_cursor = self.order_ids.iter().position(|id| *id == current_id);
        self.removed_ids.clear();
        self.played_ids.clear();
        self.order_cleared = false;
        self.removed.clear();
        self.played.clear();
        self.history.clear();
        self.cursor = None;
        self.record(current);
        self.mark_id(current_id, false);
    }
    pub fn mark_id(&mut self, id: i64, was_manual: bool) {
        if self.is_compact() {
            if was_manual && self.order_ids.contains(&id) {
                self.removed_ids.insert(id);
            } else if let Some(at) = self.order_ids.iter().position(|value| *value == id) {
                self.order_cursor = Some(at);
            }
            self.played_ids.insert(id);
        }
    }
    pub fn is_compact(&self) -> bool {
        self.context_query.is_some() || !self.order_ids.is_empty()
    }
    pub fn migrate_legacy(
        &mut self,
        ids: Vec<i64>,
        current_id: Option<i64>,
        removed_ids: HashSet<i64>,
        played_ids: HashSet<i64>,
    ) {
        self.order.clear();
        self.order_ids = Arc::new(ids);
        self.order_cursor =
            current_id.and_then(|id| self.order_ids.iter().position(|value| *value == id));
        self.removed_ids = removed_ids;
        self.played_ids = played_ids;
        self.removed.clear();
        self.played.clear();
        self.manual.truncate(20);
    }
    pub fn next_compact(
        &mut self,
        current: &str,
        backwards: bool,
        automatic: bool,
        mut resolve: impl FnMut(i64) -> Option<String>,
    ) -> Option<String> {
        if !self.is_compact() {
            return self.next(current, backwards, automatic);
        }
        if automatic && self.repeat == 2 {
            return (!current.is_empty()).then(|| current.into());
        }
        if !backwards {
            if let Some(path) = self.manual.iter().find(|path| path.as_str() != current) {
                return Some(path.clone());
            }
        }
        if self.shuffle {
            if let Some(cursor) = self.cursor {
                let target = if backwards {
                    (0..cursor)
                        .rev()
                        .find(|at| !self.removed.contains(&self.history[*at]))
                } else {
                    (cursor + 1..self.history.len())
                        .find(|at| !self.removed.contains(&self.history[*at]))
                };
                if let Some(at) = target {
                    self.cursor = Some(at);
                    return Some(self.history[at].clone());
                }
            }
            if backwards || self.order_cleared {
                return None;
            }
            let len = self.order_ids.len();
            if len == 0 {
                return None;
            }
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = self.seed;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            let start = ((value ^ (value >> 31)) as usize) % len;
            for pass in 0..2 {
                for step in 0..len {
                    let at = (start + step) % len;
                    let id = self.order_ids[at];
                    if self.removed_ids.contains(&id) || pass == 0 && self.played_ids.contains(&id)
                    {
                        continue;
                    }
                    let Some(path) = resolve(id) else { continue };
                    if path == current
                        || self.manual.contains(&path)
                        || self.removed.contains(&path)
                    {
                        continue;
                    }
                    if pass == 1 && self.repeat == 0 {
                        return None;
                    }
                    if pass == 1 {
                        self.played_ids.clear();
                    }
                    self.order_cursor = Some(at);
                    return Some(path);
                }
            }
            return (self.repeat == 1).then(|| current.into());
        }
        if self.order_cleared {
            return None;
        }
        let len = self.order_ids.len();
        let cursor = self.order_cursor?;
        for step in 1..=len {
            let at = if backwards {
                if step > cursor && self.repeat != 1 {
                    break;
                }
                (cursor + len - step % len) % len
            } else {
                if cursor + step >= len && self.repeat != 1 {
                    break;
                }
                (cursor + step) % len
            };
            let id = self.order_ids[at];
            if self.removed_ids.contains(&id) {
                continue;
            }
            let Some(path) = resolve(id) else { continue };
            if self.removed.contains(&path) || self.manual.contains(&path) {
                continue;
            }
            self.order_cursor = Some(at);
            return Some(path);
        }
        None
    }
    pub fn upcoming_limited(
        &self,
        current: &str,
        limit: usize,
        mut resolve: impl FnMut(i64) -> Option<String>,
    ) -> Vec<String> {
        if !self.is_compact() {
            return self.upcoming(current).into_iter().take(limit).collect();
        }
        if limit == 0 || current.is_empty() {
            return Vec::new();
        }
        let mut result = vec![current.to_owned()];
        if self.repeat == 2 {
            return result;
        }
        for path in &self.manual {
            if result.len() >= limit {
                return result;
            }
            if path != current && !result.contains(path) {
                result.push(path.clone());
            }
        }
        if self.order_cleared {
            return result;
        }
        let Some(cursor) = self.order_cursor else {
            return result;
        };
        let len = self.order_ids.len();
        for step in 1..len {
            if result.len() >= limit {
                break;
            }
            if cursor + step >= len && self.repeat != 1 {
                break;
            }
            let id = self.order_ids[(cursor + step) % len];
            if self.removed_ids.contains(&id) {
                continue;
            }
            if let Some(path) = resolve(id) {
                if !self.removed.contains(&path) && !result.contains(&path) {
                    result.push(path);
                }
            }
        }
        result
    }
    pub fn neighbors_compact(
        &self,
        current: &str,
        mut resolve: impl FnMut(i64) -> Option<String>,
    ) -> Vec<String> {
        if !self.is_compact() {
            return self.neighbors(current);
        }
        let mut result = Vec::with_capacity(2);
        for backwards in [false, true] {
            let mut planned = self.clone();
            if let Some(path) = planned.next_compact(current, backwards, !backwards, &mut resolve) {
                if path != current && !result.contains(&path) {
                    result.push(path);
                }
            }
        }
        result
    }
    pub fn remove_compact(&mut self, current: &str, path: &str, id: Option<i64>) {
        self.remove(current, path);
        if path != current {
            if let Some(id) = id {
                self.removed_ids.insert(id);
            }
        }
    }
    pub fn clear_compact(&mut self, current: &str) {
        if self.is_compact() {
            self.order_cleared = true;
            self.manual.clear();
        } else {
            self.clear(current);
        }
    }
    pub fn reorder_compact(
        &mut self,
        current: &str,
        source: &str,
        target: &str,
        resolve: impl FnMut(i64) -> Option<String>,
        mut lookup: impl FnMut(&str) -> Option<i64>,
    ) {
        if !self.is_compact() {
            self.reorder(current, source, target);
            return;
        }
        if source == current || source == target {
            return;
        }
        // Materialize only the visible queue window. Moved tracks become an
        // explicit front queue; the collection order remains compact.
        let mut queue = self.upcoming_limited(current, 21, resolve);
        if !queue.iter().any(|path| path == source) || !queue.iter().any(|path| path == target) {
            return;
        }
        queue.retain(|path| path != current && path != source);
        let at = if target == current {
            0
        } else {
            let Some(at) = queue.iter().position(|path| path == target) else {
                return;
            };
            at
        };
        queue.insert(at, source.into());
        queue.truncate(20);
        for path in &queue {
            if let Some(id) = lookup(path) {
                self.removed_ids.insert(id);
            }
        }
        self.manual = queue;
    }
    pub fn prune_compact(
        &mut self,
        ids: Vec<i64>,
        current_id: Option<i64>,
        mut exists: impl FnMut(&str) -> bool,
    ) {
        let previous_id = self
            .order_cursor
            .and_then(|at| self.order_ids.get(at).copied());
        let allowed: HashSet<_> = ids.iter().copied().collect();
        self.order_ids = Arc::new(ids);
        self.order_cursor = current_id
            .and_then(|id| self.order_ids.iter().position(|value| *value == id))
            .or_else(|| {
                previous_id.and_then(|id| self.order_ids.iter().position(|value| *value == id))
            });
        self.removed_ids.retain(|id| allowed.contains(id));
        self.played_ids.retain(|id| allowed.contains(id));
        self.manual.retain(|path| exists(path));
        self.removed.retain(|path| exists(path));
        self.played.retain(|path| exists(path));
        let current = self.cursor.and_then(|at| self.history.get(at)).cloned();
        self.history.retain(|path| exists(path));
        self.cursor = current
            .and_then(|path| self.history.iter().rposition(|entry| entry == &path))
            .or_else(|| self.history.len().checked_sub(1));
    }
    #[cfg(test)]
    pub fn context(&mut self, paths: Vec<String>, current: &str) {
        self.order = paths;
        self.order_ids = Arc::new(Vec::new());
        self.context_query = None;
        self.order_cursor = None;
        self.removed_ids.clear();
        self.played_ids.clear();
        self.order_cleared = false;
        self.removed.clear();
        self.played.clear();
        self.history.clear();
        self.cursor = None;
        self.record(current);
    }
    pub fn record(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        if self.manual.iter().any(|queued| queued == path) {
            // Keep the insertion point after consuming an explicit queue entry,
            // including tracks that came from outside the active collection.
            let previous = self
                .cursor
                .and_then(|i| self.history.get(i))
                .cloned()
                .unwrap_or_default();
            if !self.is_compact() {
                self.order = self.playable(&previous);
            }
        }
        if !self.is_compact() {
            self.played.insert(path.into());
        }
        self.manual.retain(|p| p != path);
        if self
            .cursor
            .and_then(|i| self.history.get(i))
            .map(String::as_str)
            == Some(path)
        {
            return;
        }
        if let Some(i) = self.cursor {
            self.history.truncate(i + 1);
        }
        self.history.push(path.into());
        self.cursor = self.history.len().checked_sub(1);
    }
    pub fn playable(&self, current: &str) -> Vec<String> {
        let mut paths: Vec<_> = self
            .order
            .iter()
            .filter(|p| {
                (!self.removed.contains(*p) || p.as_str() == current) && !self.manual.contains(*p)
            })
            .cloned()
            .collect();
        let after = paths
            .iter()
            .position(|p| p == current)
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut insertion = after;
        for manual in &self.manual {
            let at = insertion.min(paths.len());
            paths.insert(at, manual.clone());
            insertion = at + 1;
        }
        paths
    }
    pub fn upcoming(&self, current: &str) -> Vec<String> {
        let paths = self.playable(current);
        if self.repeat == 2 {
            return if current.is_empty() {
                vec![]
            } else {
                vec![current.into()]
            };
        }
        let at = paths.iter().position(|p| p == current).unwrap_or(0);
        let mut next = paths[at..].to_vec();
        if self.repeat == 1 {
            next.extend_from_slice(&paths[..at]);
        }
        next
    }
    pub fn next(&mut self, current: &str, backwards: bool, automatic: bool) -> Option<String> {
        if automatic && self.repeat == 2 {
            return Some(current.into());
        }
        // Explicit queue entries take precedence for both Next and natural endings.
        if !backwards {
            if let Some(path) = self.manual.iter().find(|p| p.as_str() != current) {
                return Some(path.clone());
            }
        }
        if self.shuffle {
            if let Some(cursor) = self.cursor {
                let target = if backwards {
                    (0..cursor)
                        .rev()
                        .find(|at| !self.removed.contains(&self.history[*at]))
                } else {
                    (cursor + 1..self.history.len())
                        .find(|at| !self.removed.contains(&self.history[*at]))
                };
                if let Some(at) = target {
                    self.cursor = Some(at);
                    return Some(self.history[at].clone());
                }
            }
            if backwards {
                return None;
            }
        }
        let paths = self.playable(current);
        if self.shuffle {
            let mut pool: Vec<_> = paths
                .iter()
                .filter(|p| !self.played.contains(*p))
                .cloned()
                .collect();
            if pool.is_empty() && self.repeat != 0 {
                self.played.clear();
                self.played.insert(current.into());
                pool = paths
                    .iter()
                    .filter(|p| p.as_str() != current)
                    .cloned()
                    .collect();
            }
            if pool.is_empty() {
                return (self.repeat == 1 && paths.iter().any(|p| p == current))
                    .then(|| current.to_string());
            }
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = self.seed;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            return Some(pool[((value ^ (value >> 31)) as usize) % pool.len()].clone());
        }
        let at = paths.iter().position(|p| p == current)?;
        if backwards {
            at.checked_sub(1)
                .or_else(|| (self.repeat == 1).then(|| paths.len() - 1))
                .map(|i| paths[i].clone())
        } else if at + 1 < paths.len() {
            Some(paths[at + 1].clone())
        } else if self.repeat == 1 {
            paths.first().cloned()
        } else {
            None
        }
    }
    /// Predict neighbors without consuming shuffle state or playback history.
    pub fn neighbors(&self, current: &str) -> Vec<String> {
        let mut neighbors = Vec::with_capacity(2);
        for backwards in [false, true] {
            let mut planned = self.clone();
            if let Some(path) = planned.next(current, backwards, !backwards) {
                if path != current && !neighbors.contains(&path) {
                    neighbors.push(path);
                }
            }
        }
        neighbors
    }
    pub fn prune(&mut self, allowed: &HashSet<String>) {
        let current = self.cursor.and_then(|i| self.history.get(i)).cloned();
        self.order.retain(|p| allowed.contains(p));
        self.manual.retain(|p| allowed.contains(p));
        self.removed.retain(|p| allowed.contains(p));
        self.played.retain(|p| allowed.contains(p));
        self.history.retain(|p| allowed.contains(p));
        self.cursor = current
            .and_then(|p| self.history.iter().rposition(|v| v == &p))
            .or_else(|| self.history.len().checked_sub(1));
    }
    pub fn remove(&mut self, current: &str, path: &str) {
        if path != current {
            self.removed.insert(path.into());
            self.manual.retain(|p| p != path);
        }
    }
    pub fn clear(&mut self, current: &str) {
        for path in &self.order {
            if path != current {
                self.removed.insert(path.clone());
            }
        }
        self.manual.clear();
    }
    pub fn reorder(&mut self, current: &str, source: &str, target: &str) {
        if source == current || source == target {
            return;
        }
        let mut paths = self.playable(current);
        if !paths.iter().any(|p| p == source) || !paths.iter().any(|p| p == target) {
            return;
        }
        paths.retain(|p| p != source);
        if let Some(at) = paths.iter().position(|p| p == target) {
            paths.insert(at + usize::from(target == current), source.into());
        }
        self.order = paths;
        self.manual.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_external_tracks_resume_the_original_context_after_consumption() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.manual
            .extend(["external-one".into(), "external-two".into()]);
        for (current, expected) in [
            ("a", "external-one"),
            ("external-one", "external-two"),
            ("external-two", "b"),
            ("b", "c"),
        ] {
            let next = q.next(current, false, true).unwrap();
            assert_eq!(next, expected);
            q.record(&next);
        }
        assert!(q.next("c", false, true).is_none());
    }
    #[test]
    fn explicit_queue_wins_over_shuffle_history_for_next_but_not_previous() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.shuffle = true;
        q.record("b");
        q.next("b", true, false);
        q.record("a");
        q.manual.push("queued".into());
        assert_eq!(q.next("a", false, false).as_deref(), Some("queued"));
        assert_eq!(q.next("a", false, true).as_deref(), Some("queued"));
        q.repeat = 2;
        assert_eq!(q.next("a", false, true).as_deref(), Some("a"));
        assert_eq!(q.next("a", false, false).as_deref(), Some("queued"));
        q.record("queued");
        assert_eq!(q.next("queued", true, false).as_deref(), Some("a"));
    }
    #[test]
    fn repeat_all_replays_a_single_song_context_even_when_shuffle_is_enabled() {
        let mut q = Navigation::default();
        q.context(vec!["only-song".into()], "only-song");
        q.shuffle = true;
        assert!(q.next("only-song", false, true).is_none());
        q.repeat = 1;
        assert_eq!(
            q.next("only-song", false, true).as_deref(),
            Some("only-song")
        );
        q.repeat = 2;
        assert_eq!(
            q.next("only-song", false, true).as_deref(),
            Some("only-song")
        );
    }
    #[test]
    fn context_ends_without_entering_another_collection_unless_repeat_or_queue_requests_it() {
        for kind in ["artist", "album", "genre", "playlist"] {
            let mut q = Navigation::default();
            q.context(
                vec![format!("{kind}-first"), format!("{kind}-last")],
                &format!("{kind}-first"),
            );
            let last = q.next(&format!("{kind}-first"), false, true).unwrap();
            q.record(&last);
            assert!(q.next(&last, false, true).is_none());
            q.repeat = 1;
            assert_eq!(q.next(&last, false, true), Some(format!("{kind}-first")));
            q.repeat = 2;
            assert_eq!(q.next(&last, false, true), Some(last.clone()));
            q.repeat = 0;
            q.manual.push("explicitly-queued-other-artist".into());
            assert_eq!(
                q.next(&last, false, true).as_deref(),
                Some("explicitly-queued-other-artist")
            );
        }
    }
    #[test]
    fn artwork_neighbors_predict_shuffle_without_changing_the_next_choice() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.shuffle = true;
        let neighbors = q.neighbors("a");
        assert_eq!(neighbors, q.neighbors("a"));
        let next = q.next("a", false, true).unwrap();
        assert_eq!(neighbors.first(), Some(&next));
        q.record(&next);
        assert!(q.neighbors(&next).contains(&"a".to_string()));
        q.clear(&next);
        assert!(q.neighbors(&next).is_empty());
    }
    #[test]
    fn shuffle_previous_and_next_retrace_history() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.shuffle = true;
        let next = q.next("a", false, false).unwrap();
        q.record(&next);
        assert_eq!(q.next(&next, true, false).as_deref(), Some("a"));
        q.record("a");
        assert_eq!(q.next("a", false, false), Some(next));
    }
    #[test]
    fn reorder_preserves_current_and_clear_leaves_only_current() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.reorder("a", "c", "b");
        assert_eq!(q.upcoming("a"), ["a", "c", "b"]);
        q.clear("a");
        assert_eq!(q.upcoming("a"), ["a"]);
    }
    #[test]
    fn manual_tracks_already_before_current_are_inserted_immediately_after_it() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "b");
        q.manual.push("a".into());
        assert_eq!(q.playable("b"), ["b", "a", "c"]);
    }
    #[test]
    fn removed_tracks_are_skipped_when_retracing_shuffle_history() {
        let mut q = Navigation::default();
        q.context(vec!["a".into(), "b".into(), "c".into()], "a");
        q.shuffle = true;
        q.record("b");
        q.record("c");
        q.remove("c", "b");
        assert_eq!(q.next("c", true, false).as_deref(), Some("a"));
    }
    #[test]
    fn pruning_removed_library_paths_preserves_valid_history_and_queue() {
        let mut navigation = Navigation::default();
        navigation.context(vec!["a".into(), "b".into(), "c".into()], "a");
        navigation.record("b");
        navigation.record("c");
        navigation.manual = vec!["missing".into(), "a".into()];
        navigation.prune(&HashSet::from(["a".into(), "c".into()]));
        assert_eq!(navigation.history, ["a", "c"]);
        assert_eq!(navigation.manual, ["a"]);
        navigation.shuffle = true;
        assert_eq!(navigation.next("c", true, false).as_deref(), Some("a"));
        navigation.prune(&HashSet::new());
        assert!(navigation.order.is_empty());
        assert!(navigation.manual.is_empty());
        assert!(navigation.history.is_empty());
        assert!(navigation.cursor.is_none());
    }
    #[test]
    fn large_context_keeps_queue_and_saved_session_bounded() {
        let mut navigation = Navigation::default();
        navigation.compact_context(
            (1..=100_000).collect(),
            Query::default(),
            50_000,
            "song-50000",
        );
        assert!(navigation.order.is_empty());
        assert_eq!(
            navigation
                .upcoming_limited("song-50000", 20, |id| Some(format!("song-{id}")))
                .len(),
            20
        );
        let next =
            navigation.next_compact("song-50000", false, false, |id| Some(format!("song-{id}")));
        assert_eq!(next.as_deref(), Some("song-50001"));
        assert_eq!(
            navigation
                .next_compact("song-50001", true, false, |id| Some(format!("song-{id}")))
                .as_deref(),
            Some("song-50000")
        );
        let encoded = serde_json::to_vec(&navigation).unwrap();
        assert!(
            encoded.len() < 900_000,
            "compact context grew to {} bytes",
            encoded.len()
        );
        let restored: Navigation = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored.order_ids.len(), 100_000);
    }

    #[test]
    fn compact_queue_removal_and_reorder_keep_the_current_position() {
        let mut navigation = Navigation::default();
        navigation.compact_context(vec![1, 2, 3, 4], Query::default(), 1, "song-1");
        navigation.reorder_compact(
            "song-1",
            "song-3",
            "song-2",
            |id| Some(format!("song-{id}")),
            |path| path.strip_prefix("song-").and_then(|id| id.parse().ok()),
        );
        assert_eq!(navigation.order_cursor, Some(0));
        assert_eq!(
            navigation.upcoming_limited("song-1", 20, |id| Some(format!("song-{id}"))),
            ["song-1", "song-3", "song-2", "song-4"]
        );
        navigation.remove_compact("song-1", "song-2", Some(2));
        assert_eq!(
            navigation.upcoming_limited("song-1", 20, |id| Some(format!("song-{id}"))),
            ["song-1", "song-3", "song-4"]
        );
        navigation.clear_compact("song-1");
        assert_eq!(
            navigation.upcoming_limited("song-1", 20, |id| Some(format!("song-{id}"))),
            ["song-1"]
        );
    }
}
