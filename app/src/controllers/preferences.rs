//! UI preferences and custom fonts. Persistence is owned by the service.
use crate::{
    protocol::{Request, Settings},
    AppState, OrcaWindow,
};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{rc::Rc, sync::mpsc::Sender, time::Duration};
pub(crate) fn clean_folder_path(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}
pub(crate) fn setting_matches(text: &str, search: &str) -> bool {
    let text = text.to_lowercase();
    search
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}
pub(crate) fn settings(ui: &OrcaWindow) -> Settings {
    let s = ui.global::<AppState>();
    Settings {
        light_theme: s.get_light_theme(),
        browse_artwork: s.get_browse_artwork(),
        compact_artwork: s.get_compact_artwork(),
        full_artwork: s.get_full_artwork(),
        grid: s.get_grid(),
        folder_grid: s.get_folder_grid(),
        sort: s.get_sort().into(),
        scale: s.get_scale(),
        quality_info: s.get_quality_info(),
        dynamic_accent: s.get_dynamic_accent(),
        blurred_background: s.get_blurred_background(),
        gapless: s.get_gapless(),
        auto_artwork: s.get_auto_artwork(),
        icon_sidebar: s.get_icon_sidebar(),
        seek_style: s.get_seek_style().into(),
        font_family: s.get_font_family().into(),
        font_path: s.get_font_path().into(),
    }
}
pub(crate) fn apply_settings(ui: &OrcaWindow, s: Settings) {
    let a = ui.global::<AppState>();
    a.set_light_theme(s.light_theme);
    a.set_browse_artwork(s.browse_artwork);
    a.set_compact_artwork(s.compact_artwork);
    a.set_full_artwork(s.full_artwork);
    a.set_grid(s.grid);
    a.set_folder_grid(s.folder_grid);
    a.set_sort(s.sort.into());
    a.set_scale(s.scale);
    a.set_quality_info(s.quality_info);
    a.set_dynamic_accent(s.dynamic_accent);
    a.set_blurred_background(s.blurred_background);
    a.set_gapless(s.gapless);
    a.set_auto_artwork(s.auto_artwork);
    a.set_icon_sidebar(s.icon_sidebar);
    a.set_seek_style(s.seek_style.into());
    if !s.font_path.is_empty() {
        match std::fs::read(&s.font_path)
            .map_err(|error| error.to_string())
            .and_then(register_font)
        {
            Ok(family) => {
                a.set_font_family(family.into());
                a.set_font_path(s.font_path.into());
            }
            Err(error) => a.set_error(format!("Custom font unavailable: {error}").into()),
        }
    } else {
        a.set_font_family(s.font_family.into());
    }
}
pub(crate) fn register_font(bytes: Vec<u8>) -> Result<String, String> {
    let mut collection = slint::fontique_011::shared_collection();
    let fonts = collection.register_fonts(
        slint::fontique_011::fontique::Blob::new(std::sync::Arc::new(bytes)),
        None,
    );
    let family = fonts.first().ok_or("File contains no supported fonts")?.0;
    collection
        .family_name(family)
        .map(str::to_string)
        .ok_or("Font has no family name".into())
}

pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>, timer: Rc<Timer>) {
    let state = ui.global::<AppState>();
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        let timer = timer.clone();
        state.on_save_settings(move || {
            let tx = tx.clone();
            let weak = weak.clone();
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(150),
                move || {
                    if let Some(ui) = weak.upgrade() {
                        let _ = tx.send(Request::Settings(settings(&ui)));
                    }
                },
            );
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        state.on_font_action(move |action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let s = ui.global::<AppState>();
            if action == "browse" {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Fonts", &["ttf", "otf", "ttc"])
                        .pick_file()
                    {
                        let _ = tx.send(Request::Font(path.to_string_lossy().into()));
                    }
                });
            } else {
                s.set_font_family(
                    if action == "system" {
                        ""
                    } else {
                        "Plus Jakarta Sans"
                    }
                    .into(),
                );
                s.set_font_path("".into());
                s.invoke_save_settings();
            }
        });
    }
}
