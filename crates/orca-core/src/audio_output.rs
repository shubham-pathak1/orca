//! Output routing; decoding, mixing, EQ and queueing remain in audio_engine.
use rodio::cpal::traits::HostTrait;
use rodio::mixer::Mixer;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub device_id: String,
    pub exclusive: bool,
    pub unprocessed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputStatus {
    pub config: OutputConfig,
    pub device_name: String,
    pub sample_rate: u32,
    pub source_sample_rate: u32,
    pub notice: String,
    pub channels: u16,
    pub sample_format: String,
    pub error: String,
    pub switching: bool,
    pub revision: u64,
}

#[cfg(any(windows, test))]
pub(crate) mod buffered;
#[cfg(any(windows, test))]
pub(crate) mod pcm;
#[cfg(windows)]
mod windows_output;
#[cfg(windows)]
pub type IntegerOutput = windows_output::OutputHandle<i32>;

pub fn devices() -> Result<Vec<OutputDevice>, String> {
    #[cfg(windows)]
    return windows_output::devices();
    #[cfg(not(windows))]
    Ok(Vec::new())
}

/// Observe Windows default-endpoint changes without polling devices.
/// The audio worker decides whether the selected output follows the default.
#[cfg(target_os = "windows")]
pub struct DefaultOutputNotifications {
    registration: Option<wasapi::DeviceEventRegistration>,
}
#[cfg(target_os = "windows")]
fn default_output_changed(
    previous: &mut Option<String>,
    direction: wasapi::Direction,
    role: wasapi::Role,
    id: Option<String>,
) -> bool {
    if direction != wasapi::Direction::Render || role != wasapi::Role::Console {
        return false;
    }
    let changed = *previous != id;
    *previous = id.clone();
    changed && id.is_some()
}
#[cfg(target_os = "windows")]
impl DefaultOutputNotifications {
    pub fn start(on_change: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        wasapi::initialize_mta()
            .ok()
            .map_err(|error| error.to_string())?;
        let result = (|| {
            let enumerator = wasapi::DeviceEnumerator::new().map_err(|error| error.to_string())?;
            let initial = enumerator
                .get_default_device(&wasapi::Direction::Render)
                .ok()
                .and_then(|device| device.get_id().ok());
            let current = Mutex::new(initial);
            let mut callbacks = wasapi::DeviceEventCallbacks::new();
            callbacks.set_default_device_callback(move |direction, role, id| {
                let changed = if let Ok(mut previous) = current.lock() {
                    default_output_changed(&mut previous, direction, role, id)
                } else {
                    false
                };
                if changed {
                    on_change();
                }
            });
            enumerator
                .register_notification_callback(callbacks)
                .map(|registration| Self {
                    registration: Some(registration),
                })
                .map_err(|error| error.to_string())
        })();
        if result.is_err() {
            wasapi::deinitialize();
        }
        result
    }
}
#[cfg(target_os = "windows")]
impl Drop for DefaultOutputNotifications {
    fn drop(&mut self) {
        drop(self.registration.take());
        wasapi::deinitialize();
    }
}
#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use wasapi::{Direction, Role};
    #[test]
    fn default_output_changes_ignore_input_roles_and_duplicate_notifications() {
        let mut endpoint = Some("laptop".to_string());
        assert!(!default_output_changed(
            &mut endpoint,
            Direction::Capture,
            Role::Console,
            Some("mic".into())
        ));
        assert!(!default_output_changed(
            &mut endpoint,
            Direction::Render,
            Role::Communications,
            Some("headset".into())
        ));
        assert_eq!(endpoint.as_deref(), Some("laptop"));
        assert!(default_output_changed(
            &mut endpoint,
            Direction::Render,
            Role::Console,
            Some("bluetooth".into())
        ));
        assert!(!default_output_changed(
            &mut endpoint,
            Direction::Render,
            Role::Console,
            Some("bluetooth".into())
        ));
        assert!(!default_output_changed(
            &mut endpoint,
            Direction::Render,
            Role::Console,
            None
        ));
        assert!(default_output_changed(
            &mut endpoint,
            Direction::Render,
            Role::Console,
            Some("laptop".into())
        ));
    }
}

pub enum OutputHandle {
    Shared(rodio::OutputStream, Arc<Mutex<Option<String>>>),
    #[cfg(windows)]
    Selected(windows_output::OutputHandle),
}
impl OutputHandle {
    pub fn open(config: &OutputConfig) -> Result<(Self, OutputStatus), String> {
        Self::open_for_rate(config, None)
    }
    pub fn open_for_rate(
        config: &OutputConfig,
        preferred_rate: Option<u32>,
    ) -> Result<(Self, OutputStatus), String> {
        if config.unprocessed {
            return Err(
                "Unprocessed output requires an integer source and exact format negotiation".into(),
            );
        }
        if preferred_rate.is_some_and(|rate| !(8000..=768000).contains(&rate)) {
            return Err("Unsupported source sample rate".into());
        }
        if config.device_id.len() > 8192 || config.device_id.contains('\0') {
            return Err("Invalid output device ID".into());
        }
        if config.exclusive || !config.device_id.is_empty() {
            #[cfg(windows)]
            return windows_output::OutputHandle::open(config, preferred_rate)
                .map(|(handle, status)| (Self::Selected(handle), status));
            #[cfg(not(windows))]
            return Err("Selected-device output is only supported on Windows.".into());
        }
        let errors = Arc::new(Mutex::new(None));
        let reported = errors.clone();
        let callback = move |error: rodio::cpal::StreamError| {
            if let Ok(mut slot) = reported.lock() {
                *slot = Some(error.to_string());
            }
        };
        // Retain Rodio's default-device fallback while observing stream failures.
        let mut stream = rodio::OutputStreamBuilder::from_default_device()
            .and_then(|builder| builder.with_error_callback(callback.clone()).open_stream())
            .or_else(|original| {
                let Ok(mut devices) = rodio::cpal::default_host().output_devices() else {
                    return Err(original);
                };
                devices
                    .find_map(|device| {
                        rodio::OutputStreamBuilder::from_device(device)
                            .and_then(|builder| {
                                builder
                                    .with_error_callback(callback.clone())
                                    .open_stream_or_fallback()
                            })
                            .ok()
                    })
                    .ok_or(original)
            })
            .map_err(|error| error.to_string())?;
        // Switching output deliberately releases this stream. Rodio's routine
        // drop diagnostic would corrupt terminal hosts; errors use the callback.
        stream.log_on_drop(false);
        let status = OutputStatus {
            config: config.clone(),
            device_name: "System default".into(),
            sample_rate: stream.config().sample_rate(),
            channels: stream.config().channel_count(),
            sample_format: format!("{:?}", stream.config().sample_format()),
            ..Default::default()
        };
        Ok((Self::Shared(stream, errors), status))
    }
    pub fn mixer(&self) -> &Mixer {
        match self {
            Self::Shared(stream, _) => stream.mixer(),
            #[cfg(windows)]
            Self::Selected(handle) => handle.mixer(),
        }
    }
    pub fn set_active(&self, active: bool) {
        #[cfg(windows)]
        if let Self::Selected(handle) = self {
            handle.set_active(active);
        }
        #[cfg(not(windows))]
        let _ = active;
    }
    /// Native buffered output replaces sources instead of waiting on a device
    /// callback to acknowledge synchronous Rodio seek/clear operations.
    pub fn buffered(&self) -> bool {
        match self {
            Self::Shared(..) => false,
            #[cfg(windows)]
            Self::Selected(_) => true,
        }
    }
    pub fn discard_pending(&self) {
        #[cfg(windows)]
        if let Self::Selected(handle) = self {
            handle.discard_pending();
        }
    }
    pub fn has_pending(&self) -> bool {
        match self {
            Self::Shared(..) => false,
            #[cfg(windows)]
            Self::Selected(handle) => handle.has_pending(),
        }
    }
    pub fn set_draining(&self, draining: bool) {
        #[cfg(windows)]
        if let Self::Selected(handle) = self {
            handle.set_draining(draining);
        }
        #[cfg(not(windows))]
        let _ = draining;
    }
    pub fn take_error(&self) -> Option<String> {
        match self {
            Self::Shared(_, errors) => errors.lock().ok()?.take(),
            #[cfg(windows)]
            Self::Selected(handle) => handle.take_error(),
        }
    }
}
