//! Bounded provider HTTP access, shared rate limiting and validated downloads.
use super::LookupError;
use crate::artwork_cache::{persist_artwork, ArtworkPaths};
use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

#[derive(Default)]
struct RateState {
    next: Option<Instant>,
    failures: u32,
}
static RATES: [Mutex<RateState>; 7] = [const {
    Mutex::new(RateState {
        next: None,
        failures: 0,
    })
}; 7];
pub(super) fn retry_after() -> Duration {
    RATES
        .iter()
        .filter_map(|rate| rate.lock().ok())
        .filter(|state| state.failures > 0)
        .filter_map(|state| state.next)
        .map(|next| next.saturating_duration_since(Instant::now()))
        .max()
        .unwrap_or_default()
}
fn provider_rate(host: &str) -> (usize, Duration) {
    let (bucket, millis) = match host {
        "musicbrainz.org" => (0, 1000),
        "itunes.apple.com" => (1, 3100),
        "api.deezer.com" => (2, 300),
        "coverartarchive.org" => (3, 1000),
        "music.apple.com" => (4, 500),
        host if host.ends_with(".mzstatic.com") => (5, 100),
        _ => (6, 250),
    };
    (bucket, Duration::from_millis(millis))
}
fn retry_delay(header: Option<&str>, failures: u32, now: SystemTime) -> Duration {
    let requested = header.and_then(|value| {
        value
            .trim()
            .parse::<u64>()
            .ok()
            .map(Duration::from_secs)
            .or_else(|| {
                httpdate::parse_http_date(value)
                    .ok()
                    .and_then(|at| at.duration_since(now).ok())
            })
    });
    requested
        .unwrap_or_default()
        .max(Duration::from_secs(2u64.pow(failures.min(7))))
        .min(Duration::from_secs(86400))
}

pub(super) trait Transport {
    fn cancelled(&self) -> bool {
        false
    }
    fn json(&self, url: &str) -> Result<Value, LookupError>;
    fn image(&self, url: &str) -> Result<Vec<u8>, LookupError>;
    fn text(&self, _url: &str) -> Result<String, LookupError> {
        Err(LookupError::NotFound)
    }
}
pub(super) struct Http<'a> {
    deadline: Instant,
    stop: Option<&'a std::sync::atomic::AtomicBool>,
}
impl<'a> Http<'a> {
    pub fn new(budget: Duration) -> Self {
        Self {
            deadline: Instant::now() + budget,
            stop: None,
        }
    }
    pub fn with_stop(budget: Duration, stop: &'a std::sync::atomic::AtomicBool) -> Self {
        Self {
            deadline: Instant::now() + budget,
            stop: Some(stop),
        }
    }
    fn remaining(&self) -> Result<Duration, LookupError> {
        if self.cancelled() {
            return Err(LookupError::Cancelled);
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Err(LookupError::TimedOut)
        } else {
            Ok(remaining)
        }
    }
    fn provider_slot(&self, bucket: usize, interval: Duration) -> Result<(), LookupError> {
        loop {
            self.remaining()?;
            let delay = {
                let mut state = RATES[bucket].lock().map_err(|_| LookupError::Unavailable)?;
                let now = Instant::now();
                let delay = state
                    .next
                    .map_or(Duration::ZERO, |next| next.saturating_duration_since(now));
                if delay.is_zero() {
                    state.next = Some(now + interval);
                    return Ok(());
                }
                delay
            };
            if self.remaining()? <= delay {
                return Err(LookupError::TimedOut);
            }
            std::thread::sleep(delay.min(Duration::from_millis(25)));
        }
    }
    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, LookupError> {
        let parsed = url::Url::parse(url).map_err(|_| LookupError::Unavailable)?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err(LookupError::Unavailable);
        }
        let (bucket, interval) = provider_rate(parsed.host_str().unwrap_or_default());
        self.provider_slot(bucket, interval)?;
        let response = ureq::get(url)
            .set("User-Agent", crate::USER_AGENT)
            .timeout(self.remaining()?.min(Duration::from_secs(4)))
            .call()
            .map_err(|error| match error {
                ureq::Error::Status(404, _) if parsed.host_str() == Some("coverartarchive.org") => {
                    LookupError::NotFound
                }
                ureq::Error::Status(status @ (429 | 503), response) => {
                    if let Ok(mut state) = RATES[bucket].lock() {
                        state.failures = state.failures.saturating_add(1);
                        let delay = retry_delay(
                            response.header("Retry-After"),
                            state.failures,
                            SystemTime::now(),
                        );
                        let next = Instant::now() + delay;
                        state.next = Some(state.next.map_or(next, |current| current.max(next)));
                    }
                    let _ = status;
                    LookupError::Unavailable
                }
                _ if self.remaining().is_err() => LookupError::TimedOut,
                _ => LookupError::Unavailable,
            })?;
        if let Ok(mut state) = RATES[bucket].lock() {
            state.failures = 0;
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LookupError::Unavailable)?;
        self.remaining()?;
        if bytes.len() as u64 > limit {
            return Err(LookupError::InvalidImage);
        }
        Ok(bytes)
    }
}
impl Transport for Http<'_> {
    fn cancelled(&self) -> bool {
        self.stop
            .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed))
    }
    fn json(&self, url: &str) -> Result<Value, LookupError> {
        serde_json::from_slice(&self.bytes(url, 2 * 1024 * 1024)?)
            .map_err(|_| LookupError::Unavailable)
    }
    fn image(&self, url: &str) -> Result<Vec<u8>, LookupError> {
        self.bytes(url, 16 * 1024 * 1024)
    }
    fn text(&self, url: &str) -> Result<String, LookupError> {
        String::from_utf8(self.bytes(url, 2 * 1024 * 1024)?).map_err(|_| LookupError::Unavailable)
    }
}
pub(super) fn download(
    client: &impl Transport,
    url: &str,
    directory: &Path,
) -> Result<ArtworkPaths, LookupError> {
    if client.cancelled() {
        return Err(LookupError::Cancelled);
    }
    let bytes = client.image(url)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(LookupError::InvalidImage);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| LookupError::InvalidImage)?;
    let format = reader.format().ok_or(LookupError::InvalidImage)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| LookupError::InvalidImage)?;
    if client.cancelled() {
        return Err(LookupError::Cancelled);
    }
    persist_artwork(directory, &bytes, Some(format.to_mime_type())).map_err(LookupError::Storage)
}

#[cfg(test)]
mod rate_tests {
    use super::*;
    #[test]
    fn retry_after_supports_seconds_dates_and_exponential_fallback() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(retry_delay(Some("60"), 1, now), Duration::from_secs(60));
        let date = httpdate::fmt_http_date(now + Duration::from_secs(120));
        assert_eq!(retry_delay(Some(&date), 1, now), Duration::from_secs(120));
        assert_eq!(retry_delay(Some("bad"), 4, now), Duration::from_secs(16));
        assert_eq!(retry_delay(None, u32::MAX, now), Duration::from_secs(128));
    }
    #[test]
    fn provider_pacing_respects_apple_and_musicbrainz_limits() {
        assert!(provider_rate("itunes.apple.com").1 >= Duration::from_secs(3));
        assert!(provider_rate("musicbrainz.org").1 >= Duration::from_secs(1));
        let stop = std::sync::atomic::AtomicBool::new(true);
        assert_eq!(
            Http::with_stop(Duration::from_secs(4), &stop).provider_slot(0, Duration::from_secs(1)),
            Err(LookupError::Cancelled)
        );
    }
}
