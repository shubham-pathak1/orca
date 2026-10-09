use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Navigation {
    pub order: Vec<String>,
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
    pub fn context(&mut self, paths: Vec<String>, current: &str) {
        self.order = paths;
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
            self.order = self.playable(&previous);
        }
        self.played.insert(path.into());
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
}
