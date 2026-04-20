use std::time::Instant;

pub struct KineticScroller {
    target_offset_buffer: f32, // The remaining distance we plan to scroll
    last_tick: Instant,
}

impl KineticScroller {
    pub fn new() -> Self {
        Self {
            target_offset_buffer: 0.0,
            last_tick: Instant::now(),
        }
    }

    pub fn add_velocity(&mut self, delta: f32) {
        // We accumulate wheel input into a target distance buffer.
        // Higher multiplier (2.2) for more distance per scroll step.
        self.target_offset_buffer += delta * 2.2;
    }

    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        // "Lenis" Web Scroll formula adaptation for 120 FPS
        // We smoothly decay the buffer while outputting the diff.
        if self.target_offset_buffer.abs() < 0.1 {
            let remain = self.target_offset_buffer;
            self.target_offset_buffer = 0.0;
            return remain;
        }

        // The lerp factor determines how "soft" the stop is.
        // 12.0 is a sweet spot for 120Hz.
        let mut lerp_factor = dt * 12.0; 
        if lerp_factor > 1.0 { lerp_factor = 1.0; }

        let step = self.target_offset_buffer * lerp_factor;
        self.target_offset_buffer -= step;

        step
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.target_offset_buffer = 0.0;
    }
}
