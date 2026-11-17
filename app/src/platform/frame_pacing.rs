use crate::OrcaWindow;
use slint::{
    winit_030::{
        winit::{
            event::{StartCause, WindowEvent},
            event_loop::{ActiveEventLoop, ControlFlow},
            window::{Window, WindowId},
        },
        CustomApplicationHandler, EventResult, WinitWindowAccessor,
    },
    ComponentHandle,
};
use std::{
    cell::RefCell,
    fs::File,
    io::{BufWriter, Write},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Schedule {
    next: Option<Instant>,
    deferred: Option<Instant>,
}
impl Schedule {
    fn admit(&mut self, now: Instant, interval: Duration) -> bool {
        if let Some(deadline) = self.next.filter(|at| *at > now) {
            self.deferred = Some(deadline);
            return false;
        }
        self.next = Some(match self.next {
            Some(deadline) if now.duration_since(deadline) < interval / 4 => deadline + interval,
            _ => now + interval,
        });
        self.deferred = None;
        true
    }
    fn take_due(&mut self, now: Instant) -> bool {
        if self.deferred.is_some_and(|at| at <= now) {
            self.deferred = None;
            true
        } else {
            false
        }
    }
}

fn interval(refresh_millihertz: u32) -> Duration {
    Duration::from_secs_f64(1000.0 / f64::from(refresh_millihertz))
}

struct Trace {
    output: BufWriter<File>,
    origin: Instant,
    started: Option<(Instant, f32, u32)>,
    samples: usize,
}
impl Trace {
    fn open() -> Option<Self> {
        let path = std::env::var_os("ORCA_FRAME_TRACE")?;
        let result = (|| {
            let mut output = BufWriter::new(File::create(path)?);
            writeln!(
                output,
                "start_us,event_to_idle_us,scroll_position,refresh_millihertz"
            )?;
            output.flush()?;
            Ok::<_, std::io::Error>(Self {
                output,
                origin: Instant::now(),
                started: None,
                samples: 0,
            })
        })();
        match result {
            Ok(trace) => Some(trace),
            Err(error) => {
                eprintln!("Could not open frame trace: {error}");
                None
            }
        }
    }
    fn finish(&mut self) -> std::io::Result<()> {
        if let Some((started, position, refresh)) = self.started.take() {
            writeln!(
                self.output,
                "{},{},{position},{refresh}",
                started.duration_since(self.origin).as_micros(),
                started.elapsed().as_micros()
            )?;
            self.samples += 1;
            if self.samples.is_multiple_of(120) {
                self.output.flush()?;
            }
        }
        Ok(())
    }
}

struct State {
    enabled: bool,
    ui: Option<slint::Weak<OrcaWindow>>,
    window_id: Option<WindowId>,
    refresh: u32,
    checked: Option<Instant>,
    occluded: bool,
    minimized: bool,
    schedule: Schedule,
    trace: Option<Trace>,
}
impl State {
    fn finish_trace(&mut self) {
        if let Some(trace) = &mut self.trace {
            if trace.finish().is_err() {
                self.trace = None;
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct FramePacing(Rc<RefCell<State>>);
impl FramePacing {
    pub fn new(enabled: bool) -> Self {
        Self(Rc::new(RefCell::new(State {
            enabled,
            ui: None,
            window_id: None,
            refresh: 60_000,
            checked: None,
            occluded: false,
            minimized: false,
            schedule: Schedule::default(),
            trace: Trace::open(),
        })))
    }
    pub fn attach(&self, ui: &OrcaWindow) {
        self.0.borrow_mut().ui = Some(ui.as_weak());
    }
}
impl CustomApplicationHandler for FramePacing {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        window: Option<&Window>,
        _slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        let mut state = self.0.borrow_mut();
        if !state.enabled && state.trace.is_none() {
            return EventResult::Propagate;
        }
        if state.window_id.is_none() {
            state.window_id = state
                .ui
                .as_ref()
                .and_then(slint::Weak::upgrade)
                .and_then(|ui| ui.window().with_winit_window(|window| window.id()));
        }
        if state.window_id != Some(window_id) {
            return EventResult::Propagate;
        }
        match event {
            WindowEvent::Destroyed => {
                state.window_id = None;
                state.schedule = Schedule::default();
            }
            WindowEvent::Occluded(occluded) => {
                state.occluded = *occluded;
                state.schedule = Schedule::default();
            }
            WindowEvent::Resized(size) => {
                state.minimized = size.width == 0 || size.height == 0;
                state.schedule = Schedule::default();
                state.checked = None;
            }
            WindowEvent::Moved(_)
            | WindowEvent::Focused(true)
            | WindowEvent::ScaleFactorChanged { .. } => {
                state.checked = None;
            }
            WindowEvent::RedrawRequested if !state.occluded && !state.minimized => {
                let now = Instant::now();
                if state
                    .checked
                    .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1))
                {
                    let refresh = window
                        .and_then(Window::current_monitor)
                        .and_then(|monitor| monitor.refresh_rate_millihertz())
                        .filter(|rate| (1_000..=1_000_000).contains(rate))
                        .unwrap_or(60_000);
                    if state.refresh != refresh {
                        state.refresh = refresh;
                        state.schedule = Schedule::default();
                    }
                    state.checked = Some(now);
                }
                let frame_interval = interval(state.refresh);
                if state.enabled && !state.schedule.admit(now, frame_interval) {
                    return EventResult::PreventDefault;
                }
                if state.trace.is_some() {
                    state.finish_trace();
                    let position = state
                        .ui
                        .as_ref()
                        .and_then(slint::Weak::upgrade)
                        .map_or(0.0, |ui| ui.library().get_artwork_scroll_position());
                    let refresh = state.refresh;
                    if let Some(trace) = &mut state.trace {
                        trace.started = Some((now, position, refresh));
                    }
                }
            }
            _ => {}
        }
        EventResult::Propagate
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) -> EventResult {
        let ui = {
            let mut state = self.0.borrow_mut();
            if !event_loop.exiting() && state.schedule.take_due(Instant::now()) {
                state.ui.clone()
            } else {
                None
            }
        };
        if let Some(ui) = ui.and_then(|weak| weak.upgrade()) {
            ui.window().with_winit_window(|window| {
                if window.is_visible() != Some(false) && window.is_minimized() != Some(true) {
                    window.request_redraw();
                }
            });
        }
        EventResult::Propagate
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> EventResult {
        let deadline = {
            let mut state = self.0.borrow_mut();
            state.finish_trace();
            state.schedule.deferred
        };
        if !event_loop.exiting() {
            if let Some(deadline) = deadline {
                let deadline = slint::platform::duration_until_next_timer_update()
                    .map_or(deadline, |delay| deadline.min(Instant::now() + delay));
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
        EventResult::Propagate
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) -> EventResult {
        let mut state = self.0.borrow_mut();
        state.schedule = Schedule::default();
        state.finish_trace();
        if let Some(trace) = &mut state.trace {
            let _ = trace.output.flush();
        }
        EventResult::Propagate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_redraws_wait_for_the_full_refresh_interval() {
        for rate in [
            59_940, 60_000, 75_000, 90_000, 120_000, 144_000, 165_000, 240_000, 360_000,
        ] {
            let mut schedule = Schedule::default();
            let start = Instant::now();
            let period = interval(rate);
            assert!(schedule.admit(start, period));
            assert!(!schedule.admit(start + period - Duration::from_nanos(1), period));
            assert!(!schedule.take_due(start + period - Duration::from_nanos(1)));
            assert!(schedule.take_due(start + period));
            assert!(!schedule.take_due(start + period));
            assert!(schedule.admit(start + period, period));
        }
        assert!(interval(144_000) > Duration::from_micros(6944));
        assert!(interval(144_000) < Duration::from_micros(6945));
    }

    #[test]
    fn late_and_idle_frames_do_not_accumulate_catch_up_draws() {
        let mut schedule = Schedule::default();
        let start = Instant::now();
        let period = interval(144_000);
        assert!(schedule.admit(start, period));
        assert!(schedule.admit(start + Duration::from_secs(2), period));
        assert!(schedule.deferred.is_none());
        assert!(!schedule.admit(
            start + Duration::from_secs(2) + Duration::from_millis(1),
            period
        ));
        assert_eq!(
            schedule.deferred,
            Some(start + Duration::from_secs(2) + period)
        );
        schedule = Schedule::default();
        assert!(!schedule.take_due(start + Duration::from_secs(3)));
    }

    #[test]
    fn substantially_late_frames_do_not_create_short_catch_up_intervals() {
        for rate in [60_000, 120_000, 144_000, 165_000, 240_000] {
            let mut schedule = Schedule::default();
            let start = Instant::now();
            let period = interval(rate);
            assert!(schedule.admit(start, period));
            let late = start + period + period * 3 / 4;
            assert!(schedule.admit(late, period));
            assert!(!schedule.admit(start + period * 2, period));
            assert_eq!(schedule.deferred, Some(late + period));
            assert!(schedule.admit(late + period, period));
        }
    }

    #[test]
    fn small_wake_delays_do_not_accumulate_across_frames() {
        for rate in [59_940, 60_000, 120_000, 144_000, 165_000, 240_000] {
            let mut schedule = Schedule::default();
            let start = Instant::now();
            let period = interval(rate);
            assert!(schedule.admit(start, period));
            for frame in 1..=1_000 {
                let deadline = start + period * frame;
                let delayed = deadline + Duration::from_micros(250);
                assert!(schedule.admit(delayed, period));
                assert_eq!(schedule.next, Some(deadline + period));
                assert!(!schedule.admit(delayed + Duration::from_micros(100), period));
                assert_eq!(schedule.deferred, Some(deadline + period));
            }
        }
    }
}
