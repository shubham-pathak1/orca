//! Clear keyboard-only focus state and route precise Windows scrolling without extra wheel inertia.
use crate::OrcaWindow;
use slint::{
    winit_030::{
        winit::event::{ElementState, Touch, TouchPhase, WindowEvent},
        EventResult, WinitWindowAccessor,
    },
    ComponentHandle,
};

pub(crate) fn install(ui: &OrcaWindow) {
    let weak = ui.as_weak();
    #[cfg(windows)]
    let precision = std::cell::RefCell::new(super::precision_scroll::PrecisionScroll::default());
    ui.window().on_winit_window_event(move |_, event| {
        if matches!(
            event,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } | WindowEvent::MouseWheel { .. }
                | WindowEvent::Touch(Touch {
                    phase: TouchPhase::Started,
                    ..
                })
        ) {
            if let Some(ui) = weak.upgrade() {
                ui.shell().set_keyboard_navigation(false);
            }
        }
        #[cfg(windows)]
        if let Some(ui) = weak.upgrade() {
            if precision.borrow_mut().handle(ui.window(), event) {
                return EventResult::PreventDefault;
            }
        }
        EventResult::Propagate
    });
}
