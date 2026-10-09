//! Windows media controls are owned by the UI lifetime; callbacks send service requests.
#[cfg(windows)]
mod windows_session {
    use crate::{protocol::Request, AppState, OrcaWindow};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::{ComponentHandle, Timer, TimerMode};
    use std::{sync::mpsc::Sender, time::Duration};
    use windows::{
        core::{w, HSTRING},
        Foundation::TypedEventHandler,
        Media::*,
        Storage::{StorageFile, Streams::RandomAccessStreamReference},
        Win32::{
            Foundation::HWND,
            System::{
                Registry::*,
                WinRT::{
                    ISystemMediaTransportControlsInterop, RoInitialize, RoUninitialize,
                    RO_INIT_SINGLETHREADED,
                },
            },
            UI::Shell::SetCurrentProcessExplicitAppUserModelID,
        },
    };

    const APP_ID: &str = "Orca.MusicPlayer";
    pub(crate) fn identity() -> Result<(), Box<dyn std::error::Error>> {
        // Register this portable executable's display identity without changing associations.
        unsafe {
            SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_ID))?;
        }
        let exe = std::env::current_exe()?;
        let icon = format!("{},0", exe.display());
        for (name, value) in [(w!("DisplayName"), "Orca"), (w!("IconUri"), icon.as_str())] {
            let bytes: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    w!("Software\\Classes\\AppUserModelId\\Orca.MusicPlayer"),
                    name,
                    REG_SZ.0,
                    Some(bytes.as_ptr().cast()),
                    (bytes.len() * 2) as u32,
                )
            };
            status.ok()?;
        }
        Ok(())
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    pub(crate) struct Session {
        controls: SystemMediaTransportControls,
        token: i64,
        timer: Timer,
        _apartment: Apartment,
    }
    impl Session {
        pub(crate) fn diagnose(&self) -> windows::core::Result<()> {
            let updater = self.controls.DisplayUpdater()?;
            updater.SetType(MediaPlaybackType::Music)?;
            updater
                .MusicProperties()?
                .SetTitle(&HSTRING::from("Orca media controls check"))?;
            updater.Update()?;
            self.controls.SetIsEnabled(true)?;
            self.controls
                .SetPlaybackStatus(MediaPlaybackStatus::Playing)?;
            eprintln!("Windows media controls: metadata and playback published successfully");
            Ok(())
        }
        pub(crate) fn new(
            ui: &OrcaWindow,
            tx: Sender<Request>,
            track: std::rc::Rc<std::cell::RefCell<orca_services::types::Track>>,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            unsafe {
                RoInitialize(RO_INIT_SINGLETHREADED)?;
            }
            let apartment = Apartment;
            let provider = ui.window().window_handle();
            let RawWindowHandle::Win32(handle) = provider.window_handle()?.as_raw() else {
                return Err("Windows media controls require a native window".into());
            };
            let interop: ISystemMediaTransportControlsInterop = windows::core::factory::<
                SystemMediaTransportControls,
                ISystemMediaTransportControlsInterop,
            >()?;
            let controls: SystemMediaTransportControls =
                unsafe { interop.GetForWindow(HWND(handle.hwnd.get() as *mut _))? };
            controls.SetIsEnabled(true)?;
            controls.SetIsPlayEnabled(true)?;
            controls.SetIsPauseEnabled(true)?;
            controls.SetIsNextEnabled(true)?;
            controls.SetIsPreviousEnabled(true)?;
            let token = controls.ButtonPressed(&TypedEventHandler::new(
                move |_,
                      args: windows::core::Ref<
                    '_,
                    SystemMediaTransportControlsButtonPressedEventArgs,
                >| {
                    if let Some(args) = args.as_ref() {
                        let action = match args.Button()? {
                            SystemMediaTransportControlsButton::Play => Some("resume"),
                            SystemMediaTransportControlsButton::Pause => Some("pause"),
                            SystemMediaTransportControlsButton::Next => Some("next"),
                            SystemMediaTransportControlsButton::Previous => Some("previous"),
                            _ => None,
                        };
                        if let Some(action) = action {
                            if let Ok(action) = crate::protocol::PlayerAction::parse(action) {
                                let _ = tx.send(Request::Command(action));
                            }
                        }
                    }
                    Ok(())
                },
            ))?;
            let timer = Timer::default();
            let weak = ui.as_weak();
            let polling = controls.clone();
            let mut last: Option<(String, String, String, String, String, bool)> = None;
            timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
                let Some(ui) = weak.upgrade() else {
                    return;
                };
                let s = ui.global::<AppState>();
                let song = track.borrow();
                let cover = [&song.artwork, &song.artwork_original, &song.artwork_thumb]
                    .into_iter()
                    .find(|path| !path.is_empty())
                    .map(|path| path.as_str())
                    .unwrap_or("");
                let current = (
                    song.path.clone(),
                    song.title.clone(),
                    song.artist.clone(),
                    song.album.clone(),
                    cover.to_string(),
                    s.get_playing(),
                );
                if last.as_ref() == Some(&current) {
                    return;
                }
                let metadata_changed = last.as_ref().is_none_or(|previous| {
                    previous.0 != current.0
                        || previous.1 != current.1
                        || previous.2 != current.2
                        || previous.3 != current.3
                        || previous.4 != current.4
                });
                let result = (|| -> windows::core::Result<()> {
                    polling.SetIsEnabled(!song.path.is_empty())?;
                    polling.SetPlaybackStatus(if song.path.is_empty() {
                        MediaPlaybackStatus::Stopped
                    } else if s.get_playing() {
                        MediaPlaybackStatus::Playing
                    } else {
                        MediaPlaybackStatus::Paused
                    })?;
                    if !metadata_changed {
                        return Ok(());
                    }
                    let updater = polling.DisplayUpdater()?;
                    updater.ClearAll()?;
                    // AppMediaId is an application-defined content id, not an
                    // AUMID registration API. Identity comes from the process.
                    updater.SetType(MediaPlaybackType::Music)?;
                    let properties = updater.MusicProperties()?;
                    properties.SetTitle(&HSTRING::from(song.title.as_str()))?;
                    properties.SetArtist(&HSTRING::from(song.artist.as_str()))?;
                    properties.SetAlbumTitle(&HSTRING::from(song.album.as_str()))?;
                    if let Some(path) = cover_path(cover) {
                        if let Ok(file) = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))
                            .and_then(|load| load.get())
                        {
                            if let Ok(stream) = RandomAccessStreamReference::CreateFromFile(&file) {
                                updater.SetThumbnail(&stream)?;
                            }
                        }
                    }
                    updater.Update()
                })();
                if let Err(error) = &result {
                    eprintln!("Media controls update: {error}");
                }
                if result.is_ok() {
                    last = Some(current);
                }
            });
            Ok(Self {
                controls,
                token,
                timer,
                _apartment: apartment,
            })
        }
    }
    fn cover_path(path: &str) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        let path = path.replace('/', "\\");
        Some(if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
        })
    }
    impl Drop for Session {
        fn drop(&mut self) {
            self.timer.stop();
            let _ = self.controls.RemoveButtonPressed(self.token);
            let _ = self.controls.SetIsEnabled(false);
        }
    }
    #[cfg(test)]
    mod tests {
        use super::cover_path;
        #[test]
        fn cover_paths_preserve_unicode_and_unc_paths() {
            let name = format!(r"C:\Music\a #{}.png", '\u{e9}');
            assert_eq!(cover_path(&format!(r"\\?\{name}")).unwrap(), name);
            assert_eq!(
                cover_path(r"\\?\UNC\server\music\a.png").unwrap(),
                r"\\server\music\a.png"
            );
            assert!(cover_path("").is_none());
        }
    }
}
#[cfg(windows)]
pub(crate) use windows_session::{identity, Session};
