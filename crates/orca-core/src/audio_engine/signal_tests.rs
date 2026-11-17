//! Deterministic audio checks: disposable WAV -> production source -> Sink/mixer
//! -> endpoint PCM. No audio device, user profile or user music is accessed.
use super::output_source::{OutputFormat, PlaybackSink as Sink};
use super::sources::load_track_into_sink;
use crate::audio_output::pcm::{encode_sample, validate_pcm};
use rodio::mixer::mixer;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

struct Wav(PathBuf);
impl Wav {
    fn new(rate: u32, channels: u16, bits: u16, samples: &[i32]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "orca-signal-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let bytes = bits / 8;
        let size = samples.len() as u32 * bytes as u32;
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * channels as u32 * bytes as u32).to_le_bytes());
        wav.extend_from_slice(&(channels * bytes).to_le_bytes());
        wav.extend_from_slice(&bits.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&size.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes()[..bytes as usize]);
        }
        std::fs::write(&path, wav).unwrap();
        Self(path)
    }
}
impl Drop for Wav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn capture(wav: &Wav, rate: u32, volume: f32, samples: usize, position: Duration) -> Vec<f32> {
    let (mixer, mut output) = mixer(2, rate);
    let sink = Sink::connect_new(&mixer, OutputFormat { channels: 2, rate });
    sink.set_volume(volume);
    let gain = Arc::new(AtomicBool::new(false));
    let peaks = Arc::new(Mutex::new(Default::default()));
    load_track_into_sink(
        &sink,
        wav.0.to_str().unwrap(),
        position,
        false,
        [0.0; 5],
        &gain,
        &peaks,
    )
    .unwrap();
    sink.play();
    output.by_ref().take(samples).collect()
}

fn pcm(samples: &[f32], bits: u16, bytes: usize) -> Vec<u8> {
    validate_pcm(false, bits, bytes).unwrap();
    let mut result = vec![0; samples.len() * bytes];
    for (sample, out) in samples.iter().zip(result.chunks_exact_mut(bytes)) {
        encode_sample(*sample, false, bits, out);
    }
    result
}

fn tones(rate: u32, frames: usize, left: f64, right: f64) -> Wav {
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        for frequency in [left, right] {
            let phase = std::f64::consts::TAU * frequency * frame as f64 / rate as f64;
            samples.push((phase.sin() * 0.5 * 8_388_608.0).round() as i32);
        }
    }
    Wav::new(rate, 2, 24, &samples)
}

fn amplitude(samples: &[f32], rate: u32, channel: usize, frequency: f64) -> f64 {
    let mut sin = 0.0;
    let mut cos = 0.0;
    let frames = samples.len() / 2;
    for (frame, value) in samples.chunks_exact(2).enumerate() {
        let phase = std::f64::consts::TAU * frequency * frame as f64 / rate as f64;
        sin += *value.get(channel).unwrap() as f64 * phase.sin();
        cos += *value.get(channel).unwrap() as f64 * phase.cos();
    }
    2.0 * sin.hypot(cos) / frames as f64
}

#[test]
fn unity_playback_preserves_16_and_24_bit_samples_and_channel_order() {
    for bits in [16, 24] {
        let max = (1i32 << (bits - 1)) - 1;
        let min = -(1i32 << (bits - 1));
        let pattern = [min, 0, -1, max, 0, min, 1, 0, max, -1, 1234, -4321];
        let input = pattern.repeat(1024);
        let wav = Wav::new(48_000, 2, bits, &input);
        let output = capture(&wav, 48_000, 1.0, input.len(), Duration::ZERO);
        let expected: Vec<_> = input
            .iter()
            .flat_map(|v| v.to_le_bytes()[..bits as usize / 8].to_vec())
            .collect();
        let actual = pcm(&output, bits, bits as usize / 8);
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual.iter().zip(&expected).all(|(a, b)| a == b),
            "PCM{bits}: first differing byte {:?}",
            actual.iter().zip(&expected).position(|(a, b)| a != b)
        );
    }
}

#[test]
fn volume_and_mute_are_applied_without_channel_leakage() {
    let input = [16_384, 0, -16_384, 0].repeat(4096);
    let wav = Wav::new(48_000, 2, 16, &input);
    for volume in [0.0, 0.25, 0.5, 1.0] {
        let output = capture(&wav, 48_000, volume, input.len(), Duration::ZERO);
        for (got, original) in output.iter().zip(&input) {
            assert!(
                (*got - *original as f32 / 32768.0 * volume).abs() < 1e-7,
                "volume {volume}: {got}"
            );
        }
    }
}

#[test]
fn packed_and_padded_pcm_clip_safely_and_sanitize_nonfinite_samples() {
    for (bits, bytes) in [(16, 2), (24, 3), (24, 4), (32, 4)] {
        let output = pcm(
            &[-2.0, -1.0, 0.0, 1.0, 2.0, f32::NAN, f32::INFINITY],
            bits,
            bytes,
        );
        let frames: Vec<_> = output.chunks_exact(bytes).collect();
        assert_eq!(frames[0], frames[1]);
        assert_eq!(frames[3], frames[4]);
        assert!(frames[2]
            .iter()
            .chain(frames[5])
            .chain(frames[6])
            .all(|v| *v == 0));
        assert_eq!(*frames[1].last().unwrap(), 128);
        if bits == 24 && bytes == 4 {
            assert!(frames.iter().all(|v| v[0] == 0));
        }
    }
}

#[test]
fn high_rate_downsampling_preserves_passband_and_rejects_aliases() {
    // A 30 kHz input must be removed before conversion to 48 kHz, where it
    // would otherwise fold into an audible 18 kHz tone. Compare steady state.
    let wav = tones(192_000, 48_000, 1000.0, 30_000.0);
    let output = capture(&wav, 48_000, 1.0, 24_000, Duration::ZERO);
    let steady = &output[4096..20_480];
    let wanted = amplitude(steady, 48_000, 0, 1000.0);
    let alias = amplitude(steady, 48_000, 1, 18_000.0);
    println!("192k -> 48k: wanted amplitude={wanted:.8}; alias amplitude={alias:.8}");
    assert!(
        (wanted - 0.5).abs() < 0.005,
        "wanted tone changed: {wanted}"
    );
    assert!(
        alias < 0.00005,
        "30 kHz folded into an audible 18 kHz tone: amplitude {alias}"
    );
}

#[test]
fn short_tracks_and_partial_chunks_have_exact_resampled_duration() {
    use super::{output_source::normalize, sources::create_track_source};
    for rate in [44_100u32, 48_000, 96_000, 192_000] {
        for frames in [1usize, 2, 3, 63, 255, 1023, 1024, 1025, 4097] {
            let wav = Wav::new(rate, 2, 16, &[8192, -8192].repeat(frames));
            let (_, source) = create_track_source(
                wav.0.to_str().unwrap(),
                Duration::ZERO,
                false,
                [0.0; 5],
                &Arc::new(AtomicBool::new(false)),
                &Arc::new(Mutex::new(Default::default())),
                None,
            )
            .unwrap();
            let output = normalize(
                source,
                OutputFormat {
                    channels: 2,
                    rate: 48_000,
                },
            )
            .unwrap()
            .take(100_000)
            .collect::<Vec<_>>();
            let expected = (frames as u64 * 48_000 + rate as u64 / 2) / rate as u64 * 2;
            assert_eq!(
                output.len() as u64,
                expected,
                "input={frames} frames at {rate}"
            );
            assert!(output.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn resumed_positions_never_swap_channels() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let wav = Wav::new(rate, 2, 16, &[16384, 0].repeat(rate as usize));
        for millis in [1, 7, 17, 53, 137, 501] {
            let output = capture(&wav, rate, 1.0, 512, Duration::from_millis(millis));
            assert!(
                output.chunks_exact(2).all(|v| v == [0.5, 0.0]),
                "channel alignment changed at {millis}ms / {rate}Hz; first frame {:?}",
                &output[..2]
            );
        }
    }
}

#[test]
fn queued_rate_changes_keep_channel_order_without_initializing_next_track_early() {
    use super::sources::create_track_source;
    let first = Wav::new(48_000, 2, 16, &[8192, -8192].repeat(4800));
    let second = Wav::new(192_000, 2, 16, &[-8192, 8192].repeat(19200));
    let (mixer, mut output) = mixer(2, 48_000);
    let sink = Sink::connect_new(
        &mixer,
        OutputFormat {
            channels: 2,
            rate: 48_000,
        },
    );
    let gain = Arc::new(AtomicBool::new(false));
    let peaks = Arc::new(Mutex::new(Default::default()));
    load_track_into_sink(
        &sink,
        first.0.to_str().unwrap(),
        Duration::ZERO,
        false,
        [0.0; 5],
        &gain,
        &peaks,
    )
    .unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let observed = started.clone();
    let (_, next) = create_track_source(
        second.0.to_str().unwrap(),
        Duration::ZERO,
        false,
        [0.0; 5],
        &gain,
        &peaks,
        Some(Box::new(move |_| {
            observed.store(true, Ordering::Relaxed);
        })),
    )
    .unwrap();
    sink.append(next).unwrap();
    assert!(!started.load(Ordering::Relaxed));
    sink.play();
    let before: Vec<_> = output.by_ref().take(9600).collect();
    assert!(!started.load(Ordering::Relaxed));
    assert!(before.chunks_exact(2).all(|v| v == [0.25, -0.25]));
    let after: Vec<_> = output.by_ref().take(9600).collect();
    assert!(started.load(Ordering::Relaxed));
    assert!(after[1024..8192]
        .chunks_exact(2)
        .all(|v| (v[0] + 0.25).abs() < 0.0001 && (v[1] - 0.25).abs() < 0.0001));
}

#[test]
fn common_rate_changes_preserve_audible_passband() {
    for (input, output) in [
        (44_100, 48_000),
        (48_000, 44_100),
        (96_000, 48_000),
        (192_000, 44_100),
    ] {
        let wav = tones(input, input as usize / 2, 1000.0, 20_000.0);
        let started = std::time::Instant::now();
        let result = capture(&wav, output, 1.0, output as usize, Duration::ZERO);
        println!(
            "{input}->{output}: rendered 500ms including source setup in {:.3}ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let steady = &result[4096..result.len() - 4096];
        for (channel, frequency) in [(0, 1000.0), (1, 20_000.0)] {
            let level = amplitude(steady, output, channel, frequency);
            assert!(
                (level - 0.5).abs() < 0.005,
                "{input}->{output}: {frequency} Hz amplitude={level}"
            );
        }
    }
}

#[test]
fn mono_mapping_and_in_place_resampler_seek_clear_old_samples() {
    use super::{output_source::normalize, sources::create_track_source};
    use rodio::Source;
    for rate in [44_100, 48_000, 192_000] {
        let input: Vec<i32> = (0..rate)
            .map(|frame| if frame < rate / 2 { 8192 } else { -8192 })
            .collect();
        let wav = Wav::new(rate, 1, 16, &input);
        let (_, source) = create_track_source(
            wav.0.to_str().unwrap(),
            Duration::ZERO,
            false,
            [0.0; 5],
            &Arc::new(AtomicBool::new(false)),
            &Arc::new(Mutex::new(Default::default())),
            None,
        )
        .unwrap();
        let mut source = normalize(
            source,
            OutputFormat {
                channels: 2,
                rate: 48_000,
            },
        )
        .unwrap();
        let before: Vec<_> = source.by_ref().take(4096).collect();
        assert!(before[1024..]
            .chunks_exact(2)
            .all(|v| (v[0] - 0.25).abs() < 0.0001 && v[0] == v[1]));
        source.try_seek(Duration::from_millis(700)).unwrap();
        let after: Vec<_> = source.by_ref().take(4096).collect();
        assert!(
            after[1024..]
                .chunks_exact(2)
                .all(|v| (v[0] + 0.25).abs() < 0.0001 && v[0] == v[1]),
            "old samples survived seek at {rate}Hz"
        );
    }
}

#[test]
fn buffered_endpoint_handoff_preserves_samples_across_chunk_boundaries() {
    use crate::audio_output::buffered::{produce, PcmQueue};
    use std::{thread, time::Instant};
    let input: Vec<i32> = (0..16384)
        .flat_map(|frame| {
            let value = 1000 + frame % 20000;
            [value, -value]
        })
        .collect();
    let wav = Wav::new(48_000, 2, 24, &input);
    let (mixer, output) = mixer(2, 48_000);
    let sink = Sink::connect_new(
        &mixer,
        OutputFormat {
            channels: 2,
            rate: 48_000,
        },
    );
    load_track_into_sink(
        &sink,
        wav.0.to_str().unwrap(),
        Duration::ZERO,
        false,
        [0.0; 5],
        &Arc::new(AtomicBool::new(false)),
        &Arc::new(Mutex::new(Default::default())),
    )
    .unwrap();
    sink.play();
    let queue = Arc::new(PcmQueue::default());
    let producer = produce(output, queue.clone(), 2048, 2).unwrap();
    queue.set_active(true);
    let mut captured = Vec::with_capacity(input.len());
    let result = (|| {
        while captured.len() < input.len() {
            let count = (input.len() - captured.len()).min(514);
            let deadline = Instant::now() + Duration::from_secs(2);
            while queue.queued_samples() < count {
                if Instant::now() > deadline {
                    return Err("PCM producer did not supply a complete block");
                }
                thread::yield_now();
            }
            queue.read(|value| captured.push(value), count);
        }
        Ok(())
    })();
    queue.stop();
    producer.join().unwrap();
    result.unwrap();
    let actual = pcm(&captured, 24, 3);
    let expected: Vec<_> = input
        .iter()
        .flat_map(|v| v.to_le_bytes()[..3].to_vec())
        .collect();
    assert!(actual.iter().zip(&expected).all(|(a, b)| a == b));
    assert_eq!(actual.len(), expected.len());
}

#[test]
fn float_pipeline_documents_32_bit_integer_precision_without_claiming_bit_perfect() {
    let input = [
        i32::MIN,
        i32::MAX,
        16_777_217,
        -16_777_217,
        123_456_789,
        -123_456_789,
        0,
        1,
    ]
    .repeat(1024);
    let wav = Wav::new(48_000, 2, 32, &input);
    let output = capture(&wav, 48_000, 1.0, input.len(), Duration::ZERO);
    let bytes = pcm(&output, 32, 4);
    for (encoded, original) in bytes.chunks_exact(4).zip(&input) {
        let result = i32::from_le_bytes(encoded.try_into().unwrap());
        assert!(
            (result as i64 - *original as i64).abs() <= 128,
            "unexpected f32 conversion error: {original} -> {result}"
        );
    }
    assert!(bytes
        .chunks_exact(4)
        .zip(&input)
        .any(|(v, original)| i32::from_le_bytes(v.try_into().unwrap()) != *original));
}
