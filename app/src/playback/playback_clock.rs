use std::time::Instant;

/// Interpolate only the visible lyric animation between backend snapshots.
/// Audio, seeking, and stored playback positions remain owned by the engine.
pub struct PlaybackClock {
    position: f32,
    duration: f32,
    playing: bool,
    updated: Instant,
}
impl PlaybackClock {
    pub fn new() -> Self {
        Self {
            position: 0.0,
            duration: 0.0,
            playing: false,
            updated: Instant::now(),
        }
    }
    pub fn update(&mut self, position: f32, duration: f32, playing: bool, now: Instant) {
        self.position = position;
        self.duration = duration;
        self.playing = playing;
        self.updated = now;
    }
    pub fn position(&self, now: Instant) -> f32 {
        let position = self.position
            + if self.playing {
                now.saturating_duration_since(self.updated).as_secs_f32() * 1000.0
            } else {
                0.0
            };
        if self.duration > 0.0 {
            position.clamp(0.0, self.duration)
        } else {
            position.max(0.0)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn interpolation_stops_on_pause_and_resets_on_seek() {
        let now = Instant::now();
        let mut clock = PlaybackClock::new();
        clock.update(1000.0, 5000.0, true, now);
        assert_eq!(clock.position(now + Duration::from_millis(150)), 1150.0);
        clock.update(1150.0, 5000.0, false, now + Duration::from_millis(150));
        assert_eq!(clock.position(now + Duration::from_secs(1)), 1150.0);
        clock.update(3000.0, 5000.0, true, now + Duration::from_secs(1));
        assert_eq!(clock.position(now + Duration::from_millis(1100)), 3100.0);
        assert_eq!(clock.position(now + Duration::from_secs(20)), 5000.0);
    }
}
