//! Disposable pronunciation cache, separate from user-authored lyrics/tags.
use crate::types::{romanization_language_code, RomanizationMode, RomanizationPolicy};
use orca_core::romanization::{cache_key, validate, RomanizedLyrics};
use std::path::{Path, PathBuf};

pub struct RomanizationCache {
    directory: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Entry {
    version: u8,
    original: Vec<String>,
    #[serde(flatten)]
    result: RomanizedLyrics,
}

fn valid_result(result: &RomanizedLyrics, count: usize) -> bool {
    result.lines.len() == count
        && result.languages.len() == count
        && validate(&result.lines).is_ok()
        && result
            .languages
            .iter()
            .all(|code| code.len() <= 16 && code.is_ascii())
}

fn visible(
    mut result: RomanizedLyrics,
    source: &[String],
    policy: &RomanizationPolicy,
) -> Vec<String> {
    if policy.mode == RomanizationMode::All {
        return result.lines;
    }
    orca_core::romanization::normalize_languages(source, &mut result);
    let selected: std::collections::HashSet<_> = policy
        .languages
        .iter()
        .map(|code| romanization_language_code(code))
        .filter(|code| code != "und")
        .collect();
    result
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            if result
                .languages
                .get(index)
                .is_some_and(|code| selected.contains(&romanization_language_code(code)))
            {
                text
            } else {
                String::new()
            }
        })
        .collect()
}

impl RomanizationCache {
    pub fn new(profile: &Path) -> Self {
        Self {
            directory: profile.join("romanized-lyrics"),
        }
    }

    pub fn get(
        &self,
        lines: &[String],
        policy: &RomanizationPolicy,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<String>, String> {
        self.get_with(lines, policy, &cancelled, |lines| {
            orca_core::romanization::romanize(lines, &cancelled)
        })
    }

    fn get_with(
        &self,
        lines: &[String],
        policy: &RomanizationPolicy,
        cancelled: impl Fn() -> bool,
        fetch: impl FnOnce(&[String]) -> Result<RomanizedLyrics, String>,
    ) -> Result<Vec<String>, String> {
        if cancelled() {
            return Err("Romanization cancelled".into());
        }
        if !policy.enabled() {
            return Ok(vec![String::new(); lines.len()]);
        }
        let key = cache_key(lines)?;
        let path = self.directory.join(format!("{key}.json"));
        if let Ok(text) = super::read_text(&path, 2 * 1024 * 1024) {
            if let Ok(entry) = serde_json::from_str::<Entry>(&text) {
                let legacy_all = entry.version == 1
                    && policy.mode == RomanizationMode::All
                    && entry.result.lines.len() == lines.len()
                    && validate(&entry.result.lines).is_ok();
                if entry.original == lines
                    && (legacy_all
                        || entry.version == 2 && valid_result(&entry.result, lines.len()))
                    && !cancelled()
                {
                    return Ok(visible(entry.result, lines, policy));
                }
            }
        }
        let result = fetch(lines)?;
        if !valid_result(&result, lines.len()) {
            return Err("Romanization line mismatch".into());
        }
        if cancelled() {
            return Err("Romanization cancelled".into());
        }
        if result.lines.iter().any(|s| !s.is_empty()) {
            let entry = Entry {
                version: 2,
                original: lines.to_vec(),
                result: result.clone(),
            };
            // Cache all returned pronunciations and their source languages, so
            // changing the selection can reuse the same entry offline.
            if std::fs::create_dir_all(&self.directory).is_ok() {
                if let Ok(bytes) = serde_json::to_vec(&entry) {
                    if !cancelled() {
                        let _ = orca_core::atomic_file::write(&path, &bytes);
                    }
                }
            }
        }
        Ok(visible(result, lines, policy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn all() -> RomanizationPolicy {
        RomanizationPolicy {
            mode: RomanizationMode::All,
            languages: vec![],
        }
    }
    fn selected(code: &str) -> RomanizationPolicy {
        RomanizationPolicy {
            mode: RomanizationMode::Selected,
            languages: vec![code.into()],
        }
    }
    #[test]
    fn selected_chinese_recovers_pronunciation_from_english_labelled_cached_batches() {
        let source = vec!["English lyric".into(), "\u{6c42}\u{6c42} for me".into()];
        let cached = RomanizedLyrics {
            lines: vec![String::new(), "qiu qiu for me".into()],
            languages: vec![String::new(), "en".into()],
        };
        assert_eq!(
            visible(cached.clone(), &source, &selected("zh")),
            cached.lines
        );
        assert!(visible(cached, &source, &selected("ja"))
            .iter()
            .all(String::is_empty));
    }
    #[test]
    fn selections_reuse_detected_languages_offline_without_mutating_cached_results() {
        let directory = std::env::temp_dir().join(format!(
            "orca-language-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = RomanizationCache::new(&directory);
        let original = vec![
            "Japanese lyric".into(),
            "Korean lyric".into(),
            "Hello".into(),
        ];
        let result = RomanizedLyrics {
            lines: vec![
                "romaji".into(),
                "Korean pronunciation".into(),
                String::new(),
            ],
            languages: vec!["ja".into(), "ko".into(), String::new()],
        };
        assert_eq!(
            cache
                .get_with(&original, &all(), || false, |_| Ok(result.clone()))
                .unwrap(),
            result.lines
        );
        let cache = RomanizationCache::new(&directory);
        assert_eq!(
            cache
                .get_with(&original, &selected("ja"), || false, |_| panic!("offline"))
                .unwrap(),
            ["romaji", "", ""]
        );
        assert_eq!(
            cache
                .get_with(&original, &selected("ko"), || false, |_| panic!("offline"))
                .unwrap(),
            ["", "Korean pronunciation", ""]
        );
        assert_eq!(
            cache
                .get_with(&original, &all(), || false, |_| panic!("offline"))
                .unwrap(),
            result.lines
        );
        assert_eq!(
            cache
                .get_with(
                    &original,
                    &RomanizationPolicy::default(),
                    || false,
                    |_| panic!("off must never fetch")
                )
                .unwrap(),
            ["", "", ""]
        );
        let empty = RomanizationPolicy {
            mode: RomanizationMode::Selected,
            languages: vec![],
        };
        assert_eq!(
            cache
                .get_with(
                    &original,
                    &empty,
                    || false,
                    |_| panic!("empty selection must never fetch")
                )
                .unwrap(),
            ["", "", ""]
        );
        assert!(cache
            .get_with(&original, &all(), || true, |_| panic!("cancelled"))
            .is_err());
        assert!(cache
            .get_with(
                &["Changed".into()],
                &all(),
                || false,
                |_| Err("offline".into())
            )
            .is_err());
        let file = cache
            .directory
            .join(format!("{}.json", cache_key(&original).unwrap()));
        std::fs::write(&file, b"broken").unwrap();
        assert!(cache
            .get_with(&original, &all(), || false, |_| Err("offline".into()))
            .is_err());
        assert!(cache
            .get_with(
                &original,
                &all(),
                || false,
                |_| Ok(RomanizedLyrics::default())
            )
            .is_err());
        // Old All-language caches stay usable. Selected mode needs source
        // metadata, so an unknown legacy language must not pass the filter.
        std::fs::write(
            &file,
            serde_json::to_vec(
                &serde_json::json!({"version":1,"original":original,"romanized":result.lines}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            cache
                .get_with(&original, &all(), || false, |_| panic!("legacy cache"))
                .unwrap(),
            result.lines
        );
        assert!(cache
            .get_with(
                &original,
                &selected("ja"),
                || false,
                |_| Err("offline".into())
            )
            .is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn language_aliases_and_unknown_sources_do_not_bypass_selection() {
        let result = RomanizedLyrics {
            lines: vec!["ni hao".into(), "shalom".into(), "unknown".into()],
            languages: vec!["zh-CN".into(), "iw".into(), "und".into()],
        };
        let source = vec![String::new(); 3];
        assert_eq!(
            visible(result.clone(), &source, &selected("zh")),
            ["ni hao", "", ""]
        );
        assert_eq!(
            visible(result, &source, &selected("he")),
            ["", "shalom", ""]
        );
    }
}
