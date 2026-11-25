//! Own the replaceable desktop window and the tray-only playback lifecycle.
use crate::{
    artwork, controllers,
    controllers::preferences::{apply_settings, settings_from_ui},
    hotkey, models, presentation,
    protocol::{Event, PlayerAction, Request, Settings},
    runtime::{self, RuntimeModels, UiRuntime},
    AppWorker, OrcaTray, OrcaWindow,
};
use orca_services::types::Track;
#[cfg(windows)]
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize, SharedString, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::atomic::Ordering,
    time::Duration,
};

thread_local! {
    static HOTKEY_DESKTOP: RefCell<Weak<Desktop>> = const { RefCell::new(Weak::new()) };
}

pub(crate) struct Desktop {
    worker: Rc<AppWorker>,
    tray: Option<OrcaTray>,
    software: bool,
    #[cfg(windows)]
    frame_pacing: Option<super::frame_pacing::FramePacing>,
    window: RefCell<Option<DesktopWindow>>,
    fullscreen_origin: Cell<Option<WindowPlacement>>,
    settings: RefCell<Settings>,
    saved_view: RefCell<Option<SavedView>>,
    global_hotkey: Cell<bool>,
    background_timer: Timer,
    background_media: RefCell<(Track, bool)>,
}

struct DesktopWindow {
    ui: OrcaWindow,
    player: Rc<crate::playback::state::PlayerModels>,
    runtime: UiRuntime,
    browse: Rc<Timer>,
    settings_timer: Rc<Timer>,
}

impl Drop for DesktopWindow {
    fn drop(&mut self) {
        self.runtime.stop();
        self.browse.stop();
        self.settings_timer.stop();
    }
}

struct SavedView {
    view: SharedString,
    settings_tab: i32,
    search: SharedString,
    sort: SharedString,
    detail_key: SharedString,
    detail_secondary: SharedString,
    detail_title: SharedString,
    detail_sort: SharedString,
    song_scroll_index: i32,
    song_scroll_fraction: f32,
    catalog_scroll: f32,
    detail_scroll: f32,
    full_player: bool,
    lyrics_open: bool,
    queue_open: bool,
    placement: WindowPlacement,
    fullscreen: bool,
}

#[derive(Clone, Copy)]
struct WindowPlacement {
    position: PhysicalPosition,
    size: PhysicalSize,
    maximized: bool,
}

impl WindowPlacement {
    fn capture(window: &slint::Window) -> Self {
        Self {
            position: window.position(),
            size: window.size(),
            maximized: window.is_maximized(),
        }
    }

    fn restore_bounds(self, window: &slint::Window) {
        window.set_position(self.position);
        if !self.maximized && self.size.width > 0 && self.size.height > 0 {
            window.set_size(self.size);
        }
    }
}

impl SavedView {
    fn capture(ui: &OrcaWindow, fullscreen_origin: Option<WindowPlacement>) -> Self {
        let library = ui.library();
        let window = ui.window();
        Self {
            view: ui.shell().get_view(),
            settings_tab: ui.shell().get_settings_tab(),
            search: library.get_search(),
            sort: library.get_sort(),
            detail_key: library.get_detail_key(),
            detail_secondary: library.get_detail_secondary(),
            detail_title: library.get_detail_title(),
            detail_sort: library.get_detail_sort(),
            song_scroll_index: library.get_library_scroll_index(),
            song_scroll_fraction: library.get_library_scroll_fraction(),
            catalog_scroll: library.get_catalog_scroll_y(),
            detail_scroll: library.get_artwork_scroll_position(),
            full_player: ui.player().get_full_player(),
            lyrics_open: ui.lyrics().get_lyrics_open(),
            queue_open: ui.player().get_queue_open(),
            placement: fullscreen_origin
                .filter(|_| window.is_fullscreen())
                .unwrap_or_else(|| WindowPlacement::capture(window)),
            fullscreen: window.is_fullscreen(),
        }
    }

    fn restore(&self, ui: &OrcaWindow) {
        let library = ui.library();
        ui.shell().set_view(self.view.clone());
        ui.shell().set_settings_tab(self.settings_tab);
        library.set_search(self.search.clone());
        library.set_sort(self.sort.clone());
        library.set_detail_key(self.detail_key.clone());
        library.set_detail_secondary(self.detail_secondary.clone());
        library.set_detail_title(self.detail_title.clone());
        library.set_detail_sort(self.detail_sort.clone());
        ui.player().set_full_player(self.full_player);
        ui.lyrics().set_lyrics_open(self.lyrics_open);
        ui.player().set_queue_open(self.queue_open);
        self.placement.restore_bounds(ui.window());
        if !self.fullscreen && !cfg!(windows) {
            ui.window().set_maximized(self.placement.maximized);
        }
        ui.window().set_fullscreen(self.fullscreen);
    }

    fn restore_scroll(&self, ui: &OrcaWindow) {
        let library = ui.library();
        if !self.detail_key.is_empty() {
            library.set_detail_scroll_target(self.detail_scroll);
            library.set_library_view_restore(self.detail_scroll < 0.0);
        } else if self.view == "songs" {
            library.set_library_scroll_index(self.song_scroll_index);
            library.set_library_scroll_fraction(self.song_scroll_fraction);
            library.set_library_view_restore(
                self.song_scroll_index > 0 || self.song_scroll_fraction > 0.0,
            );
        } else if self.catalog_scroll < 0.0 {
            library.set_catalog_scroll_target(self.catalog_scroll);
            library.set_catalog_scroll_restore_pending(true);
        }
    }
}

impl Desktop {
    fn toggle_fullscreen(&self, window: &slint::Window) {
        if window.is_fullscreen() {
            window.set_fullscreen(false);
            if let Some(origin) = self.fullscreen_origin.take() {
                window.set_maximized(origin.maximized);
            }
        } else {
            self.fullscreen_origin
                .set(Some(WindowPlacement::capture(window)));
            window.set_fullscreen(true);
        }
    }

    pub(crate) fn new(
        worker: Rc<AppWorker>,
        settings: Settings,
        software: bool,
        #[cfg(windows)] frame_pacing: Option<super::frame_pacing::FramePacing>,
    ) -> Rc<Self> {
        let tray = OrcaTray::new()
            .map_err(|error| {
                eprintln!("Tray unavailable: {error}");
                error
            })
            .ok();
        let desktop = Rc::new(Self {
            worker,
            tray,
            software,
            #[cfg(windows)]
            frame_pacing,
            window: RefCell::new(None),
            fullscreen_origin: Cell::new(None),
            settings: RefCell::new(settings),
            saved_view: RefCell::new(None),
            global_hotkey: Cell::new(false),
            background_timer: Timer::default(),
            background_media: RefCell::new((Track::default(), false)),
        });
        if let Some(tray) = &desktop.tray {
            let weak = Rc::downgrade(&desktop);
            tray.on_action(move |action| {
                if let Some(desktop) = weak.upgrade() {
                    desktop.tray_action(action.as_str());
                }
            });
        }
        desktop
    }

    pub(crate) fn register_hotkey(self: &Rc<Self>) -> Option<hotkey::PhantomHotkey> {
        self.tray.as_ref()?;
        HOTKEY_DESKTOP.with(|desktop| *desktop.borrow_mut() = Rc::downgrade(self));
        let hotkey = hotkey::PhantomHotkey::register(|| {
            let _ = slint::invoke_from_event_loop(|| {
                HOTKEY_DESKTOP.with(|desktop| {
                    if let Some(desktop) = desktop.borrow().upgrade() {
                        desktop.toggle();
                    }
                });
            });
        })
        .map_err(|error| eprintln!("Phantom shortcut unavailable: {error}"))
        .ok();
        self.global_hotkey.set(hotkey.is_some());
        hotkey
    }

    pub(crate) fn open(self: &Rc<Self>) -> Result<(), slint::PlatformError> {
        if let Some(window) = self.window.borrow().as_ref() {
            window.ui.window().set_minimized(false);
            window.ui.invoke_focus_shell();
            return Ok(());
        }
        let saved_view = self.saved_view.borrow();
        let ui = OrcaWindow::new()?;
        ui.shell().set_app_version(env!("CARGO_PKG_VERSION").into());
        ui.shell().set_software_rendering(self.software);
        // Slint creates the native tray icon asynchronously. On Windows the
        // registered global shortcut also provides a verified way back in.
        ui.shell().set_tray_available(
            self.tray.is_some() && (!cfg!(windows) || self.global_hotkey.get()),
        );
        ui.shell().set_phantom_global(self.global_hotkey.get());
        ui.shell().set_windows_audio(cfg!(windows));
        models::install_grid_window(&ui);
        #[cfg(windows)]
        if let Some(pacing) = &self.frame_pacing {
            pacing.attach(&ui);
        }
        super::input_modality::install(&ui);
        let weak = Rc::downgrade(self);
        let window = ui.as_weak();
        ui.shell().on_toggle_fullscreen(move || {
            if let (Some(desktop), Some(ui)) = (weak.upgrade(), window.upgrade()) {
                desktop.toggle_fullscreen(ui.window());
            }
        });
        let weak = Rc::downgrade(self);
        ui.shell().on_phantom(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(desktop) = weak.upgrade() {
                    desktop.toggle();
                }
            });
        });
        ui.window().on_close_requested(|| {
            let _ = slint::quit_event_loop();
            slint::CloseRequestResponse::HideWindow
        });
        apply_settings(&ui, self.settings.borrow().clone());
        let worker = &self.worker;
        let cache = Rc::new(RefCell::new(artwork::ArtworkCache::new(30 * 1024 * 1024)));
        let catalog = Rc::new(models::CatalogModels::new(worker.tx.clone(), cache.clone()));
        let player = Rc::new(crate::playback::state::PlayerModels::new());
        let browse = Rc::new(Timer::default());
        let settings_timer = Rc::new(Timer::default());
        ui.library().set_songs(catalog.list.clone().into());
        ui.library().set_song_grid(catalog.grid.clone().into());
        ui.library().set_group_grid(catalog.groups.clone().into());
        ui.library()
            .set_related_albums(catalog.related.clone().into());
        ui.player().set_queue(player.queue.clone().into());
        ui.player().set_waveform(presentation::waveform(&[]));
        controllers::input::install(&ui, worker.tx.clone());
        controllers::preferences::install(&ui, worker.tx.clone(), settings_timer.clone());
        controllers::collections::install(&ui, worker.tx.clone());
        controllers::editor::install(&ui, worker.tx.clone());
        controllers::playlists::install(&ui, worker.tx.clone());
        controllers::browse::install(
            &ui,
            worker.clone(),
            catalog.clone(),
            cache.clone(),
            browse.clone(),
        );
        controllers::player::install(&ui, worker.tx.clone(), player.clone());
        controllers::folders::install(&ui, worker.tx.clone());
        ui.shell().on_report_issue(open_issues);

        if let Some(saved) = saved_view.as_ref() {
            saved.restore(&ui);
        }
        ui.show()?;
        #[cfg(windows)]
        if saved_view
            .as_ref()
            .is_none_or(|view| view.placement.maximized && !view.fullscreen)
        {
            // Slint applies its preferred size on first show, which can resize an
            // already maximized Windows window. Maximize after that size is applied.
            let weak = ui.as_weak();
            let _ = slint::spawn_local(async move {
                if let Some(ui) = weak.upgrade() {
                    if ui.window().winit_window().await.is_ok() {
                        ui.window().set_maximized(true);
                    }
                }
            });
        }

        let runtime = runtime::start(
            &ui,
            self.tray.as_ref().map(OrcaTray::as_weak),
            RuntimeModels {
                worker: worker.clone(),
                cache,
                catalog,
                player: player.clone(),
            },
        );
        if let Some(saved) = saved_view.as_ref() {
            self.background_timer.stop();
            ui.library().invoke_browse();
            saved.restore_scroll(&ui);
            let _ = worker.tx.send(Request::RestoreWindow);
            ui.invoke_focus_shell();
        }
        *self.window.borrow_mut() = Some(DesktopWindow {
            ui,
            player,
            runtime,
            browse,
            settings_timer,
        });
        Ok(())
    }

    fn toggle(self: &Rc<Self>) {
        if self.window.borrow().is_some() {
            self.enter_phantom();
        } else if let Err(error) = self.open() {
            eprintln!("Could not restore Orca from Phantom mode: {error}");
        }
    }

    fn enter_phantom(self: &Rc<Self>) {
        let mut slot = self.window.borrow_mut();
        let Some(window) = slot.as_ref() else { return };
        if !window.ui.shell().get_tray_available() {
            return;
        }
        if window.ui.shell().get_modal_open() {
            window
                .ui
                .shell()
                .set_error("Close the open dialog before entering Phantom mode.".into());
            return;
        }
        let view = SavedView::capture(&window.ui, self.fullscreen_origin.get());
        if let Err(error) = window.ui.hide() {
            window.ui.shell().set_error(error.to_string().into());
            return;
        }
        let settings = settings_from_ui(&window.ui);
        let _ = self.worker.tx.send(Request::Settings(settings.clone()));
        *self.settings.borrow_mut() = settings;
        *self.saved_view.borrow_mut() = Some(view);
        *self.background_media.borrow_mut() = window.media_state();
        self.worker.latest_query.fetch_add(1, Ordering::Relaxed);
        self.worker
            .romanization_generation
            .fetch_add(1, Ordering::Relaxed);
        let _ = self.worker.tx.send(Request::SuspendWindow);
        drop(slot.take());

        let weak = Rc::downgrade(self);
        self.background_timer
            .start(TimerMode::Repeated, Duration::from_millis(200), move || {
                if let Some(desktop) = weak.upgrade() {
                    desktop.poll_background();
                }
            });
    }

    fn poll_background(&self) {
        let mut media = self.background_media.borrow_mut();
        let mut playing = None;
        for event in self.worker.rx.try_iter().take(128) {
            match event {
                Event::Playback(snapshot) => {
                    media.1 = snapshot.playing;
                    playing = Some(snapshot.playing);
                }
                Event::Now(track) => media.0 = track,
                Event::Error(error) => eprintln!("Orca: {error}"),
                _ => {}
            }
        }
        if let (Some(tray), Some(playing)) = (&self.tray, playing) {
            tray.set_playing(playing);
        }
    }

    fn tray_action(self: &Rc<Self>, action: &str) {
        match action {
            "restore" => {
                if let Err(error) = self.open() {
                    eprintln!("Could not restore Orca from Phantom mode: {error}");
                }
            }
            "quit" => {
                let _ = slint::quit_event_loop();
            }
            "toggle" | "previous" | "next" => {
                if let Ok(action) = PlayerAction::parse(action) {
                    let _ = self.worker.tx.send(Request::Command(action));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn final_settings(&self) -> Settings {
        self.window.borrow().as_ref().map_or_else(
            || self.settings.borrow().clone(),
            |window| settings_from_ui(&window.ui),
        )
    }

    #[cfg(windows)]
    pub(crate) fn media_state(&self) -> (Track, bool) {
        self.window.borrow().as_ref().map_or_else(
            || self.background_media.borrow().clone(),
            DesktopWindow::media_state,
        )
    }
}

impl DesktopWindow {
    fn media_state(&self) -> (Track, bool) {
        (
            self.player.now.borrow().clone(),
            self.ui.player().get_playing(),
        )
    }
}

fn open_issues() {
    let url = "https://github.com/shubham-pathak1/orca/issues";
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::{
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        Platform, PlatformError, WindowAdapter,
    };

    struct Offscreen;
    impl Platform for Offscreen {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
        }
    }

    #[test]
    fn phantom_releases_the_window_and_reopens_with_the_same_worker_and_view() {
        slint::platform::set_platform(Box::new(Offscreen)).unwrap();
        let (worker, events, requests) = AppWorker::test_channels();
        let desktop = Desktop::new(
            Rc::new(worker),
            Settings::default(),
            true,
            #[cfg(windows)]
            None,
        );
        desktop.open().unwrap();
        let windowed_size = PhysicalSize::new(1040, 720);
        let original = {
            let slot = desktop.window.borrow();
            let ui = &slot.as_ref().unwrap().ui;
            ui.shell().set_tray_available(false);
            ui.shell().set_view("albums".into());
            ui.shell().set_settings_tab(5);
            ui.library().set_search("test album".into());
            ui.player().set_full_player(true);
            ui.lyrics().set_lyrics_open(true);
            ui.window().set_size(windowed_size);
            ui.shell().invoke_toggle_fullscreen();
            ui.window().set_size(PhysicalSize::new(1920, 1080));
            ui.as_weak()
        };
        desktop.enter_phantom();
        assert!(
            original.upgrade().is_some(),
            "the tray is required to restore"
        );
        original.upgrade().unwrap().shell().set_tray_available(true);
        desktop.enter_phantom();
        assert!(
            original.upgrade().is_none(),
            "Phantom must release the Slint window"
        );

        desktop.tray_action("next");
        assert!(requests
            .try_iter()
            .any(|request| matches!(request, Request::Command(PlayerAction::Next))));
        events
            .send(Event::Now(Track {
                path: "next.flac".into(),
                ..Track::default()
            }))
            .unwrap();
        desktop.poll_background();
        assert_eq!(desktop.background_media.borrow().0.path, "next.flac");
        desktop.open().unwrap();
        assert!(requests
            .try_iter()
            .any(|request| matches!(request, Request::RestoreWindow)));
        let slot = desktop.window.borrow();
        let ui = &slot.as_ref().unwrap().ui;
        assert_eq!(ui.shell().get_view(), "albums");
        assert_eq!(ui.shell().get_settings_tab(), 5);
        assert_eq!(ui.library().get_search(), "test album");
        assert!(ui.player().get_full_player());
        assert!(ui.lyrics().get_lyrics_open());
        assert!(ui.window().is_fullscreen());
        assert!(ui
            .shell()
            .invoke_keyboard(slint::platform::Key::F11.into(), false));
        assert!(!ui.window().is_fullscreen());
        assert_eq!(ui.window().size(), windowed_size);
    }
}
