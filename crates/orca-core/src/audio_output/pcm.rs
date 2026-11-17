//! Endpoint PCM conversion shared by the Windows writer and signal tests.
pub(crate) fn validate_pcm(float: bool, bits: u16, bytes: usize) -> Result<(), String> {
    if (float && bits == 32 && bytes == 4)
        || (!float
            && (2..=4).contains(&bytes)
            && (16..=32).contains(&bits)
            && bits as usize <= bytes * 8)
    {
        Ok(())
    } else {
        Err("Unsupported PCM container from audio driver".into())
    }
}

/// The endpoint format and output slice width must have passed `validate_pcm`.
#[inline]
pub(crate) fn encode_sample(sample: f32, float: bool, valid_bits: u16, output: &mut [u8]) {
    let sample = if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    if float {
        output.copy_from_slice(&sample.to_le_bytes());
        return;
    }
    let scale = (1i64 << (valid_bits - 1)) as f64;
    let value = ((sample as f64 * scale).round() as i64).clamp(-(scale as i64), scale as i64 - 1);
    let padded = (value << (output.len() * 8 - valid_bits as usize)) as i32;
    output.copy_from_slice(&padded.to_le_bytes()[..output.len()]);
}
