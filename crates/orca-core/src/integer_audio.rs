//! Lossless integer decoding for unprocessed output. Samples remain left-aligned
//! signed 32-bit PCM; no float mixer, gain or resampler participates.
pub mod stream;
use std::{fs::File, path::Path};
use symphonia::core::{
    audio::{AudioBufferRef, SampleBuffer},
    codecs::{self, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
    units::TimeBase,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerFormat {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
}

/// One decoder packet is retained, never a complete track. All methods run on
/// the decoding worker, rather than the UI or WASAPI event thread.
pub struct IntegerDecoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    time_base: TimeBase,
    pub format: IntegerFormat,
    pub duration_frames: Option<u64>,
    samples: Vec<i32>,
    cursor: usize,
    discard_frames: u64,
    decoded_frames: u64,
    finished: bool,
}

impl IntegerDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let stream = MediaSourceStream::new(
            Box::new(File::open(path).map_err(message)?),
            Default::default(),
        );
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            hint.with_extension(extension);
        }
        let reader = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(message)?
            .format;
        let track = reader.default_track().ok_or("No audio track")?;
        let params = &track.codec_params;
        // Lossy and floating-point sources use the normal playback path. Never
        // label their conversion to integer PCM as an unchanged integer stream.
        let integer_pcm = matches!(
            params.codec,
            codecs::CODEC_TYPE_PCM_S8
                | codecs::CODEC_TYPE_PCM_U8
                | codecs::CODEC_TYPE_PCM_S16LE
                | codecs::CODEC_TYPE_PCM_S16BE
                | codecs::CODEC_TYPE_PCM_S24LE
                | codecs::CODEC_TYPE_PCM_S24BE
                | codecs::CODEC_TYPE_PCM_S32LE
                | codecs::CODEC_TYPE_PCM_S32BE
        );
        if !integer_pcm && params.codec != codecs::CODEC_TYPE_FLAC {
            return Err("Unprocessed output requires supported lossless integer PCM (WAV, AIFF or FLAC). Use normal output for this source.".into());
        }
        let rate = params.sample_rate.ok_or("Missing source sample rate")?;
        let channels = params
            .channels
            .ok_or("Missing source channel layout")?
            .count();
        let bits = params.bits_per_sample.ok_or("Missing source bit depth")?;
        if !(8000..=768000).contains(&rate)
            || !(1..=32).contains(&channels)
            || !(1..=32).contains(&bits)
        {
            return Err("Unsupported integer source format".into());
        }
        let time_base = params.time_base.unwrap_or(TimeBase::new(1, rate));
        // Seeking uses PCM-frame timestamps. Reject an unfamiliar time base
        // rather than silently returning a different sample position.
        if time_base.numer != 1 || time_base.denom != rate {
            return Err("Unsupported integer source time base".into());
        }
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions { verify: true })
            .map_err(message)?;
        Ok(Self {
            track: track.id,
            duration_frames: params.n_frames,
            format: IntegerFormat {
                rate,
                channels: channels as u16,
                bits: bits as u16,
            },
            time_base,
            reader,
            decoder,
            samples: Vec::new(),
            cursor: 0,
            discard_frames: 0,
            decoded_frames: 0,
            finished: false,
        })
    }

    pub fn seek_frame(&mut self, frame: u64) -> Result<(), String> {
        let target = frame.min(self.duration_frames.unwrap_or(u64::MAX));
        let position = self
            .reader
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: target,
                    track_id: self.track,
                },
            )
            .map_err(message)?;
        if position.actual_ts > target {
            return Err("Decoder seek passed the requested frame".into());
        }
        self.decoder.reset();
        self.samples.clear();
        self.cursor = 0;
        self.discard_frames = target - position.actual_ts;
        self.decoded_frames = position.actual_ts;
        self.finished = false;
        Ok(())
    }

    /// Fill complete interleaved frames. EOF is zero frames; corrupt packets are
    /// errors, not concealed silence. A caller can then stop and report failure.
    pub fn read_frames(&mut self, output: &mut [i32]) -> Result<usize, String> {
        let channels = self.format.channels as usize;
        if !output.len().is_multiple_of(channels) {
            return Err("Integer output buffer splits a frame".into());
        }
        let mut written = 0;
        while written < output.len() {
            if self.cursor == self.samples.len() && !self.decode_packet()? {
                break;
            }
            let available = self.samples.len() - self.cursor;
            if self.discard_frames > 0 {
                let frames = self.discard_frames.min((available / channels) as u64);
                self.cursor += frames as usize * channels;
                self.discard_frames -= frames;
                continue;
            }
            let count = available.min(output.len() - written);
            output[written..written + count]
                .copy_from_slice(&self.samples[self.cursor..self.cursor + count]);
            self.cursor += count;
            written += count;
        }
        Ok(written / channels)
    }

    fn decode_packet(&mut self) -> Result<bool, String> {
        if self.finished {
            return Ok(false);
        }
        loop {
            let packet = match self.reader.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.finished = true;
                    if self
                        .duration_frames
                        .is_some_and(|frames| self.decoded_frames < frames)
                    {
                        return Err("Integer audio ended before its declared duration".into());
                    }
                    if self.decoder.finalize().verify_ok == Some(false) {
                        return Err("Integer audio verification failed".into());
                    }
                    return Ok(false);
                }
                Err(error) => return Err(message(error)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let audio = self.decoder.decode(&packet).map_err(message)?;
            if matches!(audio, AudioBufferRef::F32(_) | AudioBufferRef::F64(_)) {
                return Err("Decoder produced floating-point samples in integer mode".into());
            }
            let spec = *audio.spec();
            if spec.rate != self.format.rate
                || spec.channels.count() != self.format.channels as usize
            {
                return Err("Integer source format changed during decoding".into());
            }
            if audio.capacity() > 65536 {
                return Err("Integer decoder packet exceeds the buffer limit".into());
            }
            let mut buffer = SampleBuffer::<i32>::new(audio.capacity() as u64, spec);
            buffer.copy_interleaved_ref(audio);
            self.samples.clear();
            self.samples.extend_from_slice(buffer.samples());
            self.decoded_frames = self
                .decoded_frames
                .saturating_add((self.samples.len() / self.format.channels as usize) as u64);
            self.cursor = 0;
            if !self.samples.is_empty() {
                return Ok(true);
            }
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_frames
            .map(|frames| {
                let time = self.time_base.calc_time(frames);
                time.seconds
                    .saturating_mul(1000)
                    .saturating_add((time.frac * 1000.0) as u64)
            })
            .unwrap_or_default()
    }
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Lossless repacking is allowed only if the destination can hold every source
/// bit. A 32-bit container with 24 valid bits cannot preserve a 32-bit source.
pub fn encode_integer(
    sample: i32,
    source_bits: u16,
    valid_bits: u16,
    output: &mut [u8],
) -> Result<(), String> {
    if !(1..=32).contains(&source_bits)
        || valid_bits < source_bits
        || valid_bits > 32
        || !(2..=4).contains(&output.len())
        || valid_bits as usize > output.len() * 8
    {
        return Err("Output cannot preserve the source integer precision".into());
    }
    if source_bits < 32 && sample as u32 & ((1u32 << (32 - source_bits)) - 1) != 0 {
        return Err("Integer sample contains bits outside the declared source precision".into());
    }
    // Symphonia integers are normalized by left alignment. No multiplication,
    // rounding or float conversion is involved in container repacking.
    write_integer(sample, output);
    Ok(())
}

pub(crate) fn write_integer(sample: i32, output: &mut [u8]) {
    let bytes = sample.to_le_bytes();
    let length = output.len();
    output.copy_from_slice(&bytes[4 - length..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn wav(bits: u16, float: bool, samples: &[i32]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "orca-integer-{}-{}.wav",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let bytes_per_sample = bits as usize / 8;
            let length = samples.len() * bytes_per_sample;
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + length as u32).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0");
            bytes.extend_from_slice(&(if float { 3u16 } else { 1u16 }).to_le_bytes());
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.extend_from_slice(&44100u32.to_le_bytes());
            bytes.extend_from_slice(&(44100 * 2 * bytes_per_sample as u32).to_le_bytes());
            bytes.extend_from_slice(&(2 * bytes_per_sample as u16).to_le_bytes());
            bytes.extend_from_slice(&bits.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&(length as u32).to_le_bytes());
            for sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes()[4 - bytes_per_sample..]);
            }
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn every_integer_bit_survives_decode_and_endpoint_encoding() {
        let mut samples = vec![
            i32::MIN,
            i32::MAX,
            -1,
            1,
            0,
            0x12345679,
            -0x12345679,
            0x7fffff01,
        ];
        let mut state = 0x87654321u32;
        for _ in 0..22000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            samples.push(state as i32);
        }
        for bits in [16, 24, 32] {
            let shift = 32 - bits;
            let expected: Vec<_> = samples
                .iter()
                .map(|sample| (sample >> shift) << shift)
                .collect();
            let fixture = Fixture::wav(bits, false, &expected);
            let mut decoder = IntegerDecoder::open(&fixture.0).unwrap();
            assert_eq!(decoder.format.bits, bits);
            let mut actual = Vec::new();
            let mut block = [0i32; 514];
            loop {
                let count = decoder.read_frames(&mut block).unwrap();
                if count == 0 {
                    break;
                }
                for &sample in &block[..count * 2] {
                    let mut pcm = [0u8; 4];
                    encode_integer(sample, bits, 32, &mut pcm).unwrap();
                    actual.push(i32::from_le_bytes(pcm));
                }
            }
            assert_eq!(actual, expected, "{bits}-bit samples changed");
        }
    }

    #[test]
    fn seeks_restart_at_the_exact_interleaved_frame() {
        let samples: Vec<_> = (0..44100)
            .flat_map(|frame| [frame * 12347 + 1, -(frame * 12347 + 3)])
            .collect();
        let fixture = Fixture::wav(32, false, &samples);
        let mut decoder = IntegerDecoder::open(&fixture.0).unwrap();
        let mut block = [0; 1028];
        for frame in [0, 1, 308, 749, 2345, 22050, 44000, 0] {
            decoder.seek_frame(frame).unwrap();
            let count = decoder.read_frames(&mut block).unwrap();
            let first = frame as usize * 2;
            assert_eq!(&block[..count * 2], &samples[first..first + count * 2]);
        }
    }

    #[test]
    fn precision_reduction_float_sources_and_partial_frames_are_rejected() {
        assert!(encode_integer(i32::MAX, 32, 24, &mut [0; 4]).is_err());
        assert!(encode_integer(1, 32, 32, &mut [0; 3]).is_err());
        let fixture = Fixture::wav(32, true, &[0, 0]);
        assert!(IntegerDecoder::open(&fixture.0).is_err());
        let fixture = Fixture::wav(32, false, &[1, -1]);
        assert!(IntegerDecoder::open(&fixture.0)
            .unwrap()
            .read_frames(&mut [0; 3])
            .is_err());
        let mut pcm = [0u8; 3];
        encode_integer(0x12345600, 24, 24, &mut pcm).unwrap();
        assert_eq!(pcm, [0x56, 0x34, 0x12]);
    }

    #[test]
    fn queued_integer_tracks_and_replacement_never_reuse_old_samples() {
        let a: Vec<i32> = (0..4000).map(|sample| sample * 12347 + 1).collect();
        let b: Vec<i32> = a.iter().map(|sample| -sample).collect();
        let first = Fixture::wav(32, false, &a);
        let second = Fixture::wav(32, false, &b);
        let (control, mut stream, events) =
            stream::StreamControl::new(IntegerDecoder::open(&first.0).unwrap());
        control
            .queue("second".into(), IntegerDecoder::open(&second.0).unwrap())
            .unwrap();
        let actual: Vec<i32> = stream.by_ref().take(a.len() + b.len()).collect();
        assert_eq!(actual, [a.clone(), b.clone()].concat());
        assert_eq!(events.try_recv().unwrap().path, "second");
        assert!(stream.next().is_none());
        assert!(control.ended());
        control
            .replace(IntegerDecoder::open(&first.0).unwrap())
            .unwrap();
        assert_eq!(stream.next(), Some(a[0]));
        control
            .queue("cancelled".into(), IntegerDecoder::open(&first.0).unwrap())
            .unwrap();
        let mut reader = IntegerDecoder::open(&second.0).unwrap();
        reader.seek_frame(17).unwrap();
        control.replace(reader).unwrap();
        assert_eq!(stream.collect::<Vec<_>>(), b[34..]);
        assert!(events.try_recv().is_err());
    }
}
