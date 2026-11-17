use std::{
    io::Read,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://translate.googleapis.com/translate_a/single";
const MAX_BYTES: usize = 128 * 1024;
const MAX_RESPONSE: u64 = 512 * 1024;
const BATCH_BYTES: usize = 3500;

pub fn validate(lines: &[String]) -> Result<(), String> {
    if lines.len() > 2048
        || lines
            .iter()
            .any(|s| s.len() > BATCH_BYTES || s.contains(['\n', '\r', '\u{e000}', '\u{e001}']))
        || lines.iter().map(String::len).sum::<usize>() > MAX_BYTES
    {
        return Err("Lyrics exceed romanization limits".into());
    }
    Ok(())
}

fn non_latin(text: &str) -> bool {
    text.chars().any(|c| {
        c.is_alphabetic()
            && !matches!(c as u32,
        0x0000..=0x024f | 0x1d00..=0x1dbf | 0x1e00..=0x1eff | 0xab30..=0xab6f | 0xff21..=0xff5a)
    })
}

/// Content-addressed, versioned cache identity, independent of song metadata.
pub fn cache_key(lines: &[String]) -> Result<String, String> {
    validate(lines)?;
    let bytes = serde_json::to_vec(&(1u8, lines)).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", md5::compute(bytes)))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RomanizedLyrics {
    #[serde(rename = "romanized")]
    pub lines: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

pub fn canonical_language(language: &str) -> String {
    let code = language
        .trim()
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match code.as_str() {
        "iw" => "he".into(),
        "in" => "id".into(),
        "ji" => "yi".into(),
        _ if (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase()) => code,
        _ => "und".into(),
    }
}

/// Google can label an English/non-Latin batch as English while still returning
/// pronunciation. Preserve per-line East Asian scripts for language filtering;
/// kana elsewhere in the lyrics disambiguates Japanese kanji-only lines.
pub fn normalize_languages(source: &[String], result: &mut RomanizedLyrics) {
    let kana = |c: char| matches!(c as u32, 0x3040..=0x30ff | 0x31f0..=0x31ff | 0xff66..=0xff9d);
    let japanese = source.iter().any(|line| line.chars().any(kana));
    result.languages.resize(source.len(), String::new());
    for (index, text) in source.iter().enumerate() {
        let detected = canonical_language(&result.languages[index]);
        let language = if text.chars().any(kana) {
            "ja"
        } else if text
            .chars()
            .any(|c| matches!(c as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af))
        {
            "ko"
        } else if text
            .chars()
            .any(|c| matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x20000..=0x2fa1f))
        {
            match detected.as_str() {
                "ja" | "zh" => detected.as_str(),
                _ if japanese => "ja",
                _ => "zh",
            }
        } else {
            continue;
        };
        result.languages[index] = language.to_owned();
    }
}

fn marker(index: usize) -> String {
    format!("\u{e000}{index:04}\u{e001}")
}

fn parse(response: &str, source: &[&str]) -> Result<(Vec<String>, String), String> {
    let root: serde_json::Value = serde_json::from_str(response).map_err(|e| e.to_string())?;
    let segments = root[0].as_array().ok_or("Missing pronunciation response")?;
    let text: String = segments.iter().filter_map(|s| s[3].as_str()).collect();
    let mut remaining = text.as_str();
    let mut parts = vec![];
    for i in 1..source.len() {
        // Google can insert spaces around the numeric marker. Keep checking
        // its identity/order rather than splitting arbitrary markers.
        let (part, marker_tail) = remaining
            .split_once('\u{e000}')
            .ok_or("Romanization line mismatch")?;
        let (number, rest) = marker_tail
            .split_once('\u{e001}')
            .ok_or("Romanization line mismatch")?;
        if number.split_whitespace().collect::<String>() != format!("{i:04}") {
            return Err("Romanization line mismatch".into());
        }
        parts.push(part.trim().to_owned());
        remaining = rest;
    }
    parts.push(remaining.trim().to_owned());
    if parts.len() != source.len() || parts.iter().any(|s| s.contains(['\u{e000}', '\u{e001}'])) {
        return Err("Romanization line mismatch".into());
    }
    for (part, original) in parts.iter_mut().zip(source) {
        if part.is_empty() || part == original || non_latin(part) {
            part.clear();
        }
    }
    Ok((parts, canonical_language(root[2].as_str().unwrap_or("und"))))
}

// A short, cancellable retry handles transient provider/network failures without
// requiring the user to reopen the song. Permanent HTTP errors are not retried.
fn request_with_retry<T>(
    mut request: impl FnMut() -> Result<T, Box<ureq::Error>>,
    check: impl Fn() -> Result<(), String>,
) -> Result<T, String> {
    check()?;
    match request() {
        Ok(response) => Ok(response),
        Err(error)
            if matches!(
                error.as_ref(),
                ureq::Error::Transport(_) | ureq::Error::Status(500..=599, _)
            ) =>
        {
            for _ in 0..5 {
                check()?;
                std::thread::sleep(Duration::from_millis(50));
            }
            check()?;
            request().map_err(|_| "Romanization unavailable".into())
        }
        Err(_) => Err("Romanization unavailable".into()),
    }
}

/// Empty entries mean already Latin or unsupported; indices always match input.
/// Network waits are bounded; cancellation is checked before every batch/read.
pub fn romanize(lines: &[String], cancelled: impl Fn() -> bool) -> Result<RomanizedLyrics, String> {
    validate(lines)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let check = || {
        if cancelled() || Instant::now() >= deadline {
            Err("Romanization cancelled".to_owned())
        } else {
            Ok(())
        }
    };
    check()?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .build();
    let indices: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, text)| non_latin(text))
        .map(|(i, _)| i)
        .collect();
    let mut output = RomanizedLyrics {
        lines: vec![String::new(); lines.len()],
        languages: vec![String::new(); lines.len()],
    };
    let mut start = 0;
    while start < indices.len() {
        check()?;
        let mut end = start + 1;
        let mut bytes = lines[indices[start]].len();
        while end < indices.len() && bytes + lines[indices[end]].len() + 16 <= BATCH_BYTES {
            bytes += lines[indices[end]].len() + 16;
            end += 1;
        }
        let source: Vec<_> = indices[start..end]
            .iter()
            .map(|&i| lines[i].as_str())
            .collect();
        let mut payload = String::new();
        for (i, text) in source.iter().enumerate() {
            if i > 0 {
                payload.push_str(&format!("\n{}\n", marker(i)));
            }
            payload.push_str(text);
        }
        let response = request_with_retry(
            || {
                agent
                    .post(ENDPOINT)
                    .set("User-Agent", crate::USER_AGENT)
                    .send_form(&[
                        ("client", "dict-chrome-ex"),
                        ("sl", "auto"),
                        ("tl", "en"),
                        ("dt", "rm"),
                        ("q", &payload),
                    ])
                    .map_err(Box::new)
            },
            check,
        )?;
        check()?;
        let mut reader = response.into_reader().take(MAX_RESPONSE + 1);
        let mut bytes = Vec::new();
        let mut block = [0; 8192];
        loop {
            check()?;
            let count = reader
                .read(&mut block)
                .map_err(|_| "Cannot read romanization response")?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&block[..count]);
            if bytes.len() as u64 > MAX_RESPONSE {
                return Err("Romanization response too large".into());
            }
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid romanization response")?;
        let (pronunciation, language) = parse(text, &source)?;
        for (&index, text) in indices[start..end].iter().zip(pronunciation) {
            output.lines[index] = text;
            output.languages[index] = language.clone();
        }
        start = end;
    }
    check()?;
    normalize_languages(lines, &mut output);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_english_batches_preserve_each_east_asian_source_language() {
        let source = vec!["English".into(), "\u{6c42}\u{6c42} for me".into()];
        let mut result = RomanizedLyrics {
            lines: vec![String::new(), "qiu qiu for me".into()],
            languages: vec![String::new(), "en".into()],
        };
        normalize_languages(&source, &mut result);
        assert_eq!(result.languages, ["", "zh"]);
        let source = vec![
            "\u{4e16}\u{754c}".into(),
            "\u{304b}\u{306a}".into(),
            "\u{d55c}".into(),
        ];
        result.languages = vec!["en".into(); 3];
        normalize_languages(&source, &mut result);
        assert_eq!(result.languages, ["ja", "ja", "ko"]);
        result.languages[0] = "zh-CN".into();
        normalize_languages(&source, &mut result);
        assert_eq!(result.languages[0], "zh");
    }
    #[test]
    fn temporary_request_failure_retries_once_and_permanent_failure_does_not() {
        let mut calls = 0;
        let result = request_with_retry(
            || {
                calls += 1;
                if calls == 1 {
                    Err(Box::new(ureq::Error::Status(
                        503,
                        ureq::Response::new(503, "Unavailable", "").unwrap(),
                    )))
                } else {
                    Ok("pronunciation")
                }
            },
            || Ok(()),
        );
        assert_eq!(result.unwrap(), "pronunciation");
        assert_eq!(calls, 2);
        for status in [403, 503] {
            let mut calls = 0;
            let result = request_with_retry::<()>(
                || {
                    calls += 1;
                    Err(Box::new(ureq::Error::Status(
                        status,
                        ureq::Response::new(status, "Failed", "").unwrap(),
                    )))
                },
                || Ok(()),
            );
            assert!(result.is_err());
            assert_eq!(calls, if status == 503 { 2 } else { 1 });
        }
    }
    #[test]
    fn cancelled_retry_does_not_send_another_request() {
        let checks = std::cell::Cell::new(0);
        let mut calls = 0;
        let result = request_with_retry::<()>(
            || {
                calls += 1;
                Err(Box::new(ureq::Error::Status(
                    503,
                    ureq::Response::new(503, "Unavailable", "").unwrap(),
                )))
            },
            || {
                checks.set(checks.get() + 1);
                if checks.get() > 1 {
                    Err("Romanization cancelled".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "Romanization cancelled");
        assert_eq!(calls, 1);
    }
    #[test]
    fn reads_pronunciation_not_translation_and_preserves_line_order() {
        let response = serde_json::json!([
            [[
                "translation",
                "original",
                null,
                format!("konnichiwa\n{}\nsekai", marker(1))
            ]],
            null,
            "ja"
        ]);
        assert_eq!(
            parse(&response.to_string(), &["こんにちは", "世界"])
                .unwrap()
                .0,
            ["konnichiwa", "sekai"]
        );
        assert_eq!(
            parse(&response.to_string(), &["source1", "source2"])
                .unwrap()
                .1,
            "ja"
        );
        assert_eq!(canonical_language("zh-CN"), "zh");
        assert_eq!(canonical_language("iw"), "he");
        assert_eq!(canonical_language("invalid source"), "und");
        let spaced =
            serde_json::json!([[[null, null, null, "Kon'nichiwa \u{e000} 0001 \u{e001} sekai"]]]);
        assert_eq!(
            parse(&spaced.to_string(), &["source1", "source2"])
                .unwrap()
                .0,
            ["Kon'nichiwa", "sekai"]
        );
        assert!(parse(&response.to_string(), &["こんにちは"]).is_err());
        assert!(parse(r#"[[["translation","source"]]]"#, &["世界"])
            .unwrap()
            .0[0]
            .is_empty());
    }
    #[test]
    fn unsupported_scripts_and_missing_boundaries_never_replace_originals() {
        assert!(parse(r#"[[[null,null,null,"世界"]]]"#, &["世界"])
            .unwrap()
            .0[0]
            .is_empty());
        assert!(parse(r#"[[[null,null,null,"one two"]]]"#, &["一", "二"]).is_err());
        assert!(non_latin("こんにちは Ελληνικά العربية हिंदी 한국어"));
        assert!(!non_latin("Café déjà vu — Łódź 123"));
    }
    #[test]
    fn latin_skips_network_and_cancellation_and_limits_are_enforced() {
        assert_eq!(romanize(&["Café".into()], || false).unwrap().lines, [""]);
        assert!(romanize(&["こんにちは".into()], || true).is_err());
        assert!(validate(&["x".repeat(BATCH_BYTES + 1)]).is_err());
        assert!(validate(&["line\nline".into()]).is_err());
        assert_ne!(
            cache_key(&["一".into()]).unwrap(),
            cache_key(&["二".into()]).unwrap()
        );
    }
}
