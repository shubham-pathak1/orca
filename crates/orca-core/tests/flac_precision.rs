//! Generated complete FLAC streams exercise both shared and integer playback.
use orca_core::integer_audio::IntegerDecoder;
use std::{fs, io::BufReader, path::PathBuf};
#[derive(Default)]
struct Bits(Vec<u8>, usize);
impl Bits {
    fn put(&mut self, value: i64, width: u32) {
        for shift in (0..width).rev() {
            if self.1.is_multiple_of(8) {
                self.0.push(0);
            }
            *self.0.last_mut().unwrap() |=
                (((value as u64 >> shift) & 1) as u8) << (7 - self.1 % 8);
            self.1 += 1;
        }
    }
}
fn crc(data: &[u8], width: u32, polynomial: u32) -> u32 {
    let mut value = 0;
    for &byte in data {
        value ^= u32::from(byte) << (width - 8);
        for _ in 0..8 {
            value = (value << 1)
                ^ if value & (1 << (width - 1)) != 0 {
                    polynomial
                } else {
                    0
                };
        }
    }
    value & ((1 << width) - 1)
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn fixture(bits: u32, assignment: u8, constant: bool, inherited: bool) -> (Fixture, Vec<i32>) {
    let max = (1i64 << (bits - 1)) - 1;
    let min = -(1i64 << (bits - 1));
    let mut left = Vec::new();
    let mut right = Vec::new();
    for i in 0..32 {
        left.push(if constant {
            max
        } else {
            [min, max, 1, -1, 0, 123, -456, 789][i % 8]
        });
        right.push(if constant {
            min
        } else {
            [max, min, -1, 1, 0, -234, 567, -890][i % 8]
        });
    }
    let channels = match assignment {
        1 => [(left.clone(), bits), (right.clone(), bits)],
        8 => [
            (left.clone(), bits),
            (
                left.iter().zip(&right).map(|(l, r)| l - r).collect(),
                bits + 1,
            ),
        ],
        9 => [
            (
                left.iter().zip(&right).map(|(l, r)| l - r).collect(),
                bits + 1,
            ),
            (right.clone(), bits),
        ],
        10 => [
            (
                left.iter().zip(&right).map(|(l, r)| (l + r) >> 1).collect(),
                bits,
            ),
            (
                left.iter().zip(&right).map(|(l, r)| l - r).collect(),
                bits + 1,
            ),
        ],
        _ => unreachable!(),
    };
    let code = if inherited {
        0
    } else {
        match bits {
            16 => 4,
            24 => 6,
            32 => 7,
            _ => unreachable!(),
        }
    };
    let mut frame = vec![0xff, 0xf8, 0x6a, (assignment << 4) | (code << 1), 0, 31];
    frame.push(crc(&frame, 8, 0x07) as u8);
    let mut encoded = Bits::default();
    for (samples, width) in channels {
        encoded.put(if constant { 0 } else { 2 }, 8);
        for sample in samples.iter().take(if constant { 1 } else { 32 }) {
            encoded.put(*sample, width);
        }
    }
    frame.extend(encoded.0);
    frame.extend((crc(&frame, 16, 0x8005) as u16).to_be_bytes());
    let mut second = frame[..frame.len() - 2].to_vec();
    second[4] = 1;
    second[6] = crc(&second[..6], 8, 0x07) as u8;
    second.extend((crc(&second, 16, 0x8005) as u16).to_be_bytes());
    let mut info = vec![0, 32, 0, 32, 0, 0, 0, 0, 0, 0];
    info.extend(((48000u64 << 44) | (1 << 41) | (u64::from(bits - 1) << 36) | 64).to_be_bytes());
    info.extend([0; 16]);
    let mut flac = b"fLaC\x80\0\0\x22".to_vec();
    flac.extend(info);
    flac.extend(frame);
    flac.extend(second);
    let path = std::env::temp_dir().join(format!(
        "orca-flac-{}-{}.flac",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, flac).unwrap();
    let expected: Vec<i32> = left
        .into_iter()
        .zip(right)
        .flat_map(|(l, r)| [(l as i32) << (32 - bits), (r as i32) << (32 - bits)])
        .collect();
    let expected = expected.repeat(2);
    (Fixture(path), expected)
}
#[test]
fn stereo_flac_preserves_integer_samples_and_plays_through_rodio() {
    for bits in [16, 24, 32] {
        for assignment in [1, 8, 9, 10] {
            for constant in [false, true] {
                for inherited in [false, true] {
                    let (file, expected) = fixture(bits, assignment, constant, inherited);
                    let mut decoder = IntegerDecoder::open(&file.0).unwrap();
                    assert_eq!(u32::from(decoder.format.bits), bits);
                    let mut actual = vec![0; expected.len()];
                    assert_eq!(decoder.read_frames(&mut actual).unwrap(), 64);
                    assert_eq!(actual, expected, "{bits}-bit assignment {assignment}");
                    assert_eq!(decoder.read_frames(&mut actual).unwrap(), 0);
                    decoder.seek_frame(5).unwrap();
                    let mut tail = vec![0; 118];
                    assert_eq!(decoder.read_frames(&mut tail).unwrap(), 59);
                    assert_eq!(tail, expected[10..]);
                    let shared =
                        rodio::Decoder::new(BufReader::new(fs::File::open(&file.0).unwrap()))
                            .unwrap()
                            .collect::<Vec<_>>();
                    assert_eq!(shared.len(), expected.len());
                    for (actual, expected) in shared.into_iter().zip(expected) {
                        assert!((actual - expected as f32 / 2147483648.0).abs() <= f32::EPSILON);
                    }
                }
            }
        }
    }
}
#[test]
#[ignore = "read-only diagnostic: set ORCA_FLAC_CHECK to a local FLAC path"]
fn external_flac_verifies_without_audio_device() {
    let path = std::env::var_os("ORCA_FLAC_CHECK").expect("set ORCA_FLAC_CHECK");
    let mut decoder = IntegerDecoder::open(std::path::Path::new(&path)).unwrap();
    let mut buffer = vec![0; 4096 * usize::from(decoder.format.channels)];
    let mut frames = 0;
    loop {
        let count = decoder.read_frames(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        frames += count as u64;
    }
    assert_eq!(Some(frames), decoder.duration_frames);
    println!(
        "Verified {frames} frames, {:?}, including FLAC checksum when present",
        decoder.format
    );
}
