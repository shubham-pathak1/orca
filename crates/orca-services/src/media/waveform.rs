use crate::Backend;
use orca_core::{audio_engine, db};

impl Backend {
    pub fn waveform(&self, path: &str, buckets: u32) -> Result<Vec<f32>, String> {
        self.waveform_cancellable(path, buckets, || false)
    }
    pub fn waveform_cancellable(
        &self,
        path: &str,
        buckets: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<f32>, String> {
        if cancelled() {
            return Err("Waveform analysis cancelled".into());
        }
        self.track(path)?;
        let buckets = (buckets as usize).clamp(32, audio_engine::MAX_WAVEFORM_BUCKETS);
        if let Some(peaks) = db::get_cached_waveform(&self.conn, path, buckets)? {
            return Ok(peaks);
        }
        let peaks = audio_engine::compute_waveform_peaks_cancellable(path, buckets, &cancelled)?;
        if cancelled() {
            return Err("Waveform analysis cancelled".into());
        }
        db::save_waveform(&self.conn, path, buckets, &peaks)?;
        Ok(peaks)
    }
}
