//! Preserve high-precision Windows scroll deltas without adding wheel inertia.
use i_slint_core::{
    input::{BackendMouseEvent, TouchPhase},
    lengths::LogicalPoint,
};
use slint::winit_030::winit::event::{MouseScrollDelta, WindowEvent};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct PrecisionScroll {
    position: Option<LogicalPoint>,
    precise_until: Option<Instant>,
}
impl PrecisionScroll {
    fn delta(&mut self, delta: &MouseScrollDelta, now: Instant, scale: f64) -> Option<(f32, f32)> {
        let (x, y, precise) = match delta {
            MouseScrollDelta::PixelDelta(p) => ((p.x / scale) as f32, (p.y / scale) as f32, true),
            MouseScrollDelta::LineDelta(x, y) => (
                *x * 60.0,
                *y * 60.0,
                x.fract().abs() > 0.0001 || y.fract().abs() > 0.0001,
            ),
        };
        if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
            return None;
        }
        let ongoing = self.precise_until.is_some_and(|until| now < until);
        if precise || ongoing {
            self.precise_until = Some(now + Duration::from_millis(150));
            Some((x, y))
        } else {
            self.precise_until = None;
            None
        }
    }
    pub(super) fn handle(&mut self, window: &slint::Window, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(window.scale_factor() as f64);
                self.position = Some(LogicalPoint::new(p.x, p.y));
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                self.position = None;
                self.precise_until = None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(position) = self.position else {
                    return false;
                };
                if let Some((x, y)) =
                    self.delta(delta, Instant::now(), window.scale_factor() as f64)
                {
                    dispatch_delta(window, position, x, y);
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}
fn dispatch_delta(window: &slint::Window, position: LogicalPoint, x: f32, y: f32) {
    // Slint 1.18's immediate wheel phase also stops a previous wheel animation.
    window.dispatch_event(slint::platform::WindowEvent::internal(
        BackendMouseEvent::Wheel {
            position,
            delta_x: x,
            delta_y: y,
            phase: TouchPhase::Cancelled,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precise_sequences_keep_native_distance_and_regular_wheel_steps_stay_animated() {
        let mut input = PrecisionScroll::default();
        let now = Instant::now();
        assert_eq!(
            input.delta(&MouseScrollDelta::LineDelta(0.0, -1.0), now, 1.25),
            None
        );
        assert_eq!(
            input.delta(&MouseScrollDelta::LineDelta(0.0, -0.1), now, 1.25),
            Some((0.0, -6.0))
        );
        assert_eq!(
            input.delta(
                &MouseScrollDelta::LineDelta(0.0, -1.0),
                now + Duration::from_millis(20),
                1.25
            ),
            Some((0.0, -60.0))
        );
        assert_eq!(
            input.delta(
                &MouseScrollDelta::LineDelta(0.0, -1.0),
                now + Duration::from_millis(171),
                1.25
            ),
            None
        );
        assert_eq!(
            input.delta(
                &MouseScrollDelta::PixelDelta(slint::winit_030::winit::dpi::PhysicalPosition::new(
                    2.5, -5.0
                )),
                now,
                1.25
            ),
            Some((2.0, -4.0))
        );
        assert_eq!(
            input.delta(&MouseScrollDelta::LineDelta(0.0, f32::NAN), now, 1.25),
            None
        );
    }
    #[test]
    fn whole_number_deltas_do_not_end_a_continuous_precise_gesture() {
        let mut input = PrecisionScroll::default();
        let start = Instant::now();
        assert!(input
            .delta(&MouseScrollDelta::LineDelta(0.0, -0.1), start, 1.0)
            .is_some());
        for step in 1..=100 {
            assert_eq!(
                input.delta(
                    &MouseScrollDelta::LineDelta(0.0, -1.0),
                    start + Duration::from_millis(step * 20),
                    1.0
                ),
                Some((0.0, -60.0))
            );
        }
        assert_eq!(
            input.delta(
                &MouseScrollDelta::LineDelta(0.0, -1.0),
                start + Duration::from_millis(2151),
                1.0
            ),
            None
        );
    }

    #[test]
    fn precise_input_moves_the_viewport_immediately_without_a_second_animation() {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::{
            platform::{Platform, PlatformError, WindowAdapter},
            ComponentHandle,
        };
        use std::rc::Rc;
        slint::slint! {
            export component ScrollProbe inherits Window {
                width:200px;height:200px;
                out property<float> position:area.content-y/1px;
                area:=Flickable {width:200px;height:200px;content-width:200px;content-height:2000px;}
            }
        }
        struct Offscreen(Rc<MinimalSoftwareWindow>);
        impl Platform for Offscreen {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(self.0.clone())
            }
        }
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
        let ui = ScrollProbe::new().unwrap();
        ui.show().unwrap();
        let mut pixels = vec![slint::Rgb8Pixel::default(); 200 * 200];
        window.draw_if_needed(|r| {
            r.render(&mut pixels, 200);
        });
        for expected in 1..=8 {
            dispatch_delta(ui.window(), LogicalPoint::new(50.0, 50.0), 0.0, -3.5);
            assert_eq!(ui.get_position(), -3.5 * expected as f32);
            assert!(
                !ui.window().has_active_animations(),
                "precise input must not start wheel inertia"
            );
        }
        dispatch_delta(ui.window(), LogicalPoint::new(50.0, 50.0), 0.0, 7.0);
        assert_eq!(
            ui.get_position(),
            -21.0,
            "direction changes must apply without residual movement"
        );
        ui.hide().unwrap();
    }
}
