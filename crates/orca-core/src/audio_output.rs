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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputStatus {
    pub config: OutputConfig,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub error: String,
    pub switching: bool,
    pub revision: u64,
}

// The experimental direct WASAPI worker is deliberately not compiled: hardware
// validation found blocking driver calls and corrupt output. Keep the established
// Rodio/CPAL shared-output path on every platform until a replacement is validated.
pub fn devices() -> Result<Vec<OutputDevice>, String> {
    Ok(Vec::new())
}

/// Observe Windows default-endpoint changes without polling devices.
/// Playback continues to use shared Rodio output.
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

pub struct OutputHandle(rodio::OutputStream, Arc<Mutex<Option<String>>>);
impl OutputHandle {
    pub fn open(config: &OutputConfig) -> Result<(Self, OutputStatus), String> {
        if config.exclusive || !config.device_id.is_empty() {
            return Err("Output device selection and exclusive mode are disabled pending validation. Use shared system-default output.".into());
        }
        let errors = Arc::new(Mutex::new(None));
        let reported = errors.clone();
        let callback = move |error: rodio::cpal::StreamError| {
            if let Ok(mut slot) = reported.lock() {
                *slot = Some(error.to_string());
            }
        };
        // Retain Rodio's default-device fallback while observing stream failures.
        let stream = rodio::OutputStreamBuilder::from_default_device()
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
        let status = OutputStatus {
            config: config.clone(),
            device_name: "System default".into(),
            sample_rate: stream.config().sample_rate(),
            channels: stream.config().channel_count(),
            sample_format: format!("{:?}", stream.config().sample_format()),
            ..Default::default()
        };
        Ok((Self(stream, errors), status))
    }
    pub fn mixer(&self) -> &Mixer {
        self.0.mixer()
    }
    pub fn set_active(&self, _active: bool) {}
    pub fn take_error(&self) -> Option<String> {
        self.1.lock().ok()?.take()
    }
}
