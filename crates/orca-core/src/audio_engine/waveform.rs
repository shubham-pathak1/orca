use rodio::{Decoder, Source};
use std::fs::File;

pub fn compute_waveform_peaks(path: &str, buckets: usize) -> Result<Vec<f32>, String> {
    compute_waveform_peaks_cancellable(path, buckets, || false)
}
pub fn compute_waveform_peaks_cancellable(
    path: &str,
    buckets: usize,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<f32>, String> {
    if cancelled() {
        return Err("Waveform analysis cancelled".into());
    }
    let buckets = buckets.clamp(24, super::MAX_WAVEFORM_BUCKETS);
    let file = File::open(path).map_err(|e| format!("Failed to open '{path}': {e}"))?;
    let decoder = Decoder::try_from(file).map_err(|e| format!("Decode error for '{path}': {e}"))?;
    let channels = decoder.channels().max(1) as f64;
    let sample_rate = decoder.sample_rate().max(1) as f64;
    let samples_per_bucket = decoder
        .total_duration()
        .map(|duration| {
            ((duration.as_secs_f64() * sample_rate * channels) / buckets as f64).ceil() as usize
        })
        .unwrap_or(4096)
        .max(1);

    let mut peaks = Vec::with_capacity(buckets);
    let mut current_peak = 0.0_f32;
    let mut sample_count = 0_usize;

    for (index, sample) in decoder.enumerate() {
        if index % 4096 == 0 && cancelled() {
            return Err("Waveform analysis cancelled".into());
        }
        current_peak = current_peak.max(sample.abs());
        sample_count += 1;

        if sample_count >= samples_per_bucket {
            peaks.push(current_peak.min(1.0));
            current_peak = 0.0;
            sample_count = 0;

            if peaks.len() >= buckets {
                break;
            }
        }
    }

    if sample_count > 0 && peaks.len() < buckets {
        peaks.push(current_peak.min(1.0));
    }

    if peaks.is_empty() {
        return Ok(vec![0.0; buckets]);
    }

    let max_peak = peaks.iter().copied().fold(0.0_f32, f32::max).max(0.001);
    for peak in &mut peaks {
        *peak = (*peak / max_peak).clamp(0.0, 1.0);
    }

    peaks.resize(buckets, 0.0);
    Ok(peaks)
}
