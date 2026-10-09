//! UI image/color helpers; no database or playback commands.
use crate::{AppState, OrcaWindow, Song};
use slint::ComponentHandle;
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
pub(crate) fn reset_empty_player(ui: &OrcaWindow) {
    let s = ui.global::<AppState>();
    s.set_now(Song {
        cover_missing: true,
        ..Default::default()
    });
    s.set_waveform(slint::Image::default());
    s.set_original_cover(slint::Image::default());
    s.set_original_cover_missing(true);
    s.set_full_backdrop(slint::Image::default());
    s.set_backdrop(slint::Image::default());
    s.set_position(0.0);
    s.set_lyric_position(0.0);
    s.set_duration(0.0);
    s.set_playing(false);
    s.set_elapsed("0:00".into());
    s.set_total("0:00".into());
}
