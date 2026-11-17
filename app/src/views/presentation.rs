//! UI image/color helpers; no database or playback commands.
use crate::{OrcaWindow, Song};
pub(crate) fn waveform(peaks: &[f32]) -> slint::Image {
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(720, 48);
    let pixels = buffer.make_mut_slice();
    for x in 0..720usize {
        let peak = if peaks.is_empty() {
            0.15 + ((x * 31) % 47) as f32 / 100.0
        } else {
            peaks[x * peaks.len() / 720].abs()
        };
        let height = (peak.clamp(0.0, 1.0).powf(0.55) * 23.0).max(1.0) as usize;
        for y in 24 - height..24 + height {
            pixels[y * 720 + x] = slint::Rgba8Pixel::new(255, 255, 255, 255);
        }
    }
    slint::Image::from_rgba8(buffer)
}

pub(crate) fn theme_accent(mut rgb: [u8; 3], light: bool) -> [u8; 3] {
    if light {
        // Keep artwork-derived colors legible on the warm light background.
        let channel = |value: u8| {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        while 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]) > 0.24
        {
            rgb = rgb.map(|value| (value as f32 * 0.9) as u8);
        }
    }
    rgb
}

pub(crate) fn player_background_tone(rgb: [u8; 3]) -> [u8; 3] {
    let maximum = *rgb.iter().max().unwrap() as f32;
    let minimum = *rgb.iter().min().unwrap() as f32;
    if maximum == 0.0 {
        return [64; 3];
    }

    // Color mode needs a dark, recognizable version of the cover color. Pale
    // covers otherwise become a flat gray wash after blending over black.
    let value = (maximum * 0.52).clamp(72.0, 112.0);
    let saturation = if maximum - minimum < 8.0 {
        0.0
    } else {
        ((maximum - minimum) / maximum).clamp(0.46, 0.88)
    };
    let original_saturation = (maximum - minimum) / maximum;
    rgb.map(|channel| {
        let chroma_position = if original_saturation > 0.0 {
            (maximum - channel as f32) / (maximum - minimum)
        } else {
            0.0
        };
        (value * (1.0 - saturation * chroma_position)).round() as u8
    })
}
pub(crate) fn reset_empty_player(ui: &OrcaWindow) {
    ui.player().set_now(Song {
        cover_missing: true,
        ..Default::default()
    });
    ui.player().set_waveform(slint::Image::default());
    ui.player().set_original_cover(slint::Image::default());
    ui.player().set_original_cover_missing(true);
    ui.player().set_full_backdrop(slint::Image::default());
    ui.player().set_backdrop(slint::Image::default());
    ui.player().set_position(0.0);
    ui.lyrics().set_lyric_position(0.0);
    ui.player().set_duration(0.0);
    ui.player().set_playing(false);
    ui.player().set_elapsed("0:00".into());
    ui.player().set_total("0:00".into());
}
