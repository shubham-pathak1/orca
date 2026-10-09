//! Bounded provider HTTP access, shared rate limiting and validated downloads.
use super::LookupError;
use crate::artwork_cache::{persist_artwork, ArtworkPaths};
use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

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
    fn musicbrainz_slot(&self) -> Result<(), LookupError> {
        static LAST: Mutex<Option<Instant>> = Mutex::new(None);
        loop {
            self.remaining()?;
            match LAST.try_lock() {
                Ok(mut last) => {
                    if let Some(at) = *last {
                        let delay = Duration::from_secs(1).saturating_sub(at.elapsed());
                        if self.remaining()? <= delay {
                            return Err(LookupError::TimedOut);
                        }
                        std::thread::sleep(delay);
                    }
                    *last = Some(Instant::now());
                    return Ok(());
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(_) => return Err(LookupError::Unavailable),
            }
        }
    }
    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, LookupError> {
        let parsed = url::Url::parse(url).map_err(|_| LookupError::Unavailable)?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err(LookupError::Unavailable);
        }
        if parsed.host_str() == Some("musicbrainz.org") {
            self.musicbrainz_slot()?;
        }
        let response = ureq::get(url)
            .set(
                "User-Agent",
                "Orca/0.1.6-alpha (https://github.com/shubham-pathak1/orca)",
            )
            .timeout(self.remaining()?.min(Duration::from_secs(4)))
            .call()
            .map_err(|error| match error {
                ureq::Error::Status(404, _) if parsed.host_str() == Some("coverartarchive.org") => {
                    LookupError::NotFound
                }
                _ if self.remaining().is_err() => LookupError::TimedOut,
                _ => LookupError::Unavailable,
            })?;
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
