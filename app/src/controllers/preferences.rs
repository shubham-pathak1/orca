//! UI preferences and custom fonts; debounced requests persist settings off the UI thread.
use crate::{
    protocol::{Request, Settings},
    OrcaWindow,
};
use orca_services::types::{
    romanization_language_code, RomanizationMode, RomanizationPolicy, ROMANIZATION_LANGUAGES,
};
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
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
pub(crate) fn settings_from_ui(ui: &OrcaWindow) -> Settings {
    Settings {
        light_theme: ui.preferences().get_light_theme(),
        browse_artwork: ui.preferences().get_browse_artwork(),
        compact_artwork: ui.preferences().get_compact_artwork(),
        full_artwork: ui.preferences().get_full_artwork(),
        grid: ui.preferences().get_grid(),
        folder_grid: ui.preferences().get_folder_grid(),
        sort: ui.library().get_sort().into(),
        scale: ui.preferences().get_scale(),
        quality_info: ui.preferences().get_quality_info(),
        lyrics_left_aligned: ui.preferences().get_lyrics_left_aligned(),
        blur_unfocused_lyrics: ui.preferences().get_blur_unfocused_lyrics(),
        lyrics_blur_strength: ui
            .preferences()
            .get_lyrics_blur_strength()
            .round()
            .clamp(0.0, 100.0) as u8,
        romanization_mode: RomanizationMode::from_index(ui.preferences().get_romanization_mode()),
        romanization_languages: romanization_policy(ui).languages,
        show_tooltips: ui.preferences().get_show_tooltips(),
        show_liked_songs: ui.preferences().get_show_liked_songs(),
        dynamic_accent: ui.preferences().get_dynamic_accent(),
        blurred_background: ui.preferences().get_blurred_background(),
        full_player_background: ui.preferences().get_full_player_background().into(),
        auto_hide_player_controls: ui.preferences().get_auto_hide_player_controls(),
        sidebar_pages: [
            ("songs", ui.preferences().get_sidebar_songs()),
            ("artists", ui.preferences().get_sidebar_artists()),
            ("albums", ui.preferences().get_sidebar_albums()),
            ("genres", ui.preferences().get_sidebar_genres()),
            ("playlists", ui.preferences().get_sidebar_playlists()),
            ("folders", ui.preferences().get_sidebar_folders()),
            ("composers", ui.preferences().get_sidebar_composers()),
            ("years", ui.preferences().get_sidebar_years()),
        ]
        .into_iter()
        .filter(|(_, shown)| *shown)
        .map(|(page, _)| page.to_owned())
        .collect(),
        gapless: ui.preferences().get_gapless(),
        replaygain: ui.preferences().get_replaygain(),
        output: orca_services::types::OutputPreference {
            device_id: ui.player().get_output_device_id().into(),
            exclusive: ui.player().get_output_exclusive(),
            unprocessed: ui.player().get_output_unprocessed(),
        },
        auto_artwork: ui.preferences().get_auto_artwork(),
        icon_sidebar: ui.preferences().get_icon_sidebar(),
        seek_style: ui.preferences().get_seek_style().into(),
        font_family: ui.preferences().get_font_family().into(),
        font_path: ui.preferences().get_font_path().into(),
    }
}
pub(crate) fn apply_settings(ui: &OrcaWindow, s: Settings) {
    ui.preferences().set_light_theme(s.light_theme);
    ui.preferences().set_browse_artwork(s.browse_artwork);
    ui.preferences().set_compact_artwork(s.compact_artwork);
    ui.preferences().set_full_artwork(s.full_artwork);
    ui.preferences().set_grid(s.grid);
    ui.preferences().set_folder_grid(s.folder_grid);
    ui.library().set_sort(s.sort.into());
    ui.preferences().set_scale(s.scale);
    ui.preferences().set_quality_info(s.quality_info);
    ui.preferences()
        .set_lyrics_left_aligned(s.lyrics_left_aligned);
    ui.preferences()
        .set_blur_unfocused_lyrics(s.blur_unfocused_lyrics);
    ui.preferences()
        .set_lyrics_blur_strength(s.lyrics_blur_strength.min(100) as f32);
    ui.preferences()
        .set_romanization_mode(s.romanization_mode.index());
    let selected: Vec<_> = s
        .romanization_languages
        .iter()
        .map(|code| romanization_language_code(code))
        .filter(|code| code != "und")
        .collect();
    let mut choices: Vec<_> = ROMANIZATION_LANGUAGES
        .iter()
        .map(|(code, name)| crate::RomanizationLanguage {
            code: (*code).into(),
            name: (*name).into(),
            selected: selected.iter().any(|value| value.as_str() == *code),
        })
        .collect();
    // Retain valid choices from newer profiles even if an older picker lacks a label.
    for code in selected {
        if !choices
            .iter()
            .any(|choice| choice.code.as_str() == code.as_str())
        {
            choices.push(crate::RomanizationLanguage {
                name: code.clone().into(),
                code: code.into(),
                selected: true,
            });
        }
    }
    ui.preferences().set_romanization_selected_count(
        choices.iter().filter(|choice| choice.selected).count() as i32,
    );
    ui.preferences()
        .set_romanization_languages(Rc::new(VecModel::from(choices)).into());
    ui.preferences().set_show_tooltips(s.show_tooltips);
    ui.preferences().set_show_liked_songs(s.show_liked_songs);
    ui.preferences().set_dynamic_accent(s.dynamic_accent);
    ui.preferences().set_blurred_background(false);
    let background = match s.full_player_background.as_str() {
        "artwork" | "gradient" | "color" | "off" => s.full_player_background.as_str(),
        _ if s.blurred_background => "artwork",
        _ => "off",
    };
    ui.preferences()
        .set_full_player_background(background.into());
    ui.preferences()
        .set_auto_hide_player_controls(s.auto_hide_player_controls);
    ui.preferences()
        .set_sidebar_songs(s.sidebar_pages.iter().any(|page| page == "songs"));
    ui.preferences()
        .set_sidebar_artists(s.sidebar_pages.iter().any(|page| page == "artists"));
    ui.preferences()
        .set_sidebar_albums(s.sidebar_pages.iter().any(|page| page == "albums"));
    ui.preferences()
        .set_sidebar_genres(s.sidebar_pages.iter().any(|page| page == "genres"));
    ui.preferences()
        .set_sidebar_playlists(s.sidebar_pages.iter().any(|page| page == "playlists"));
    ui.preferences()
        .set_sidebar_folders(s.sidebar_pages.iter().any(|page| page == "folders"));
    ui.preferences()
        .set_sidebar_composers(s.sidebar_pages.iter().any(|page| page == "composers"));
    ui.preferences()
        .set_sidebar_years(s.sidebar_pages.iter().any(|page| page == "years"));
    ui.preferences().set_gapless(s.gapless);
    ui.preferences().set_replaygain(s.replaygain);
    ui.player().set_output_device_id(s.output.device_id.into());
    ui.player().set_output_exclusive(s.output.exclusive);
    ui.player().set_output_unprocessed(s.output.unprocessed);
    ui.preferences().set_auto_artwork(s.auto_artwork);
    ui.preferences().set_icon_sidebar(s.icon_sidebar);
    ui.preferences().set_seek_style(s.seek_style.into());
    if !s.font_path.is_empty() {
        match std::fs::read(&s.font_path)
            .map_err(|error| error.to_string())
            .and_then(register_font)
        {
            Ok(family) => {
                ui.preferences().set_font_family(
                    if s.font_family.is_empty() || s.font_family == "Plus Jakarta Sans" {
                        s.font_family
                    } else {
                        family.clone()
                    }
                    .into(),
                );
                ui.preferences().set_custom_font_family(family.into());
                ui.preferences().set_font_path(s.font_path.into());
            }
            Err(error) => {
                ui.preferences().set_font_family("Plus Jakarta Sans".into());
                ui.preferences().set_custom_font_family("".into());
                ui.preferences().set_font_path("".into());
                ui.shell()
                    .set_error(format!("Custom font unavailable: {error}").into());
            }
        }
    } else {
        ui.preferences().set_font_family(s.font_family.into());
        ui.preferences().set_custom_font_family("".into());
        ui.preferences().set_font_path("".into());
    }
}
pub(crate) fn romanization_policy(ui: &OrcaWindow) -> RomanizationPolicy {
    let choices = ui.preferences().get_romanization_languages();
    RomanizationPolicy {
        mode: RomanizationMode::from_index(ui.preferences().get_romanization_mode()),
        languages: (0..choices.row_count())
            .filter_map(|i| choices.row_data(i))
            .filter(|choice| choice.selected)
            .map(|choice| choice.code.to_string())
            .collect(),
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
    ui.shell()
        .on_display_path(|path| clean_folder_path(&path).into());
    ui.preferences()
        .on_setting_matches(|text, search| setting_matches(&text, &search));
    {
        let weak = ui.as_weak();
        ui.preferences().on_font_scale(move |value| {
            if let Some(ui) = weak.upgrade() {
                if let Ok(value) = value.trim().parse::<f32>() {
                    if value.is_finite() {
                        ui.preferences().set_scale(value.clamp(80.0, 120.0) / 100.0);
                        ui.preferences().invoke_save_settings();
                    }
                }
            }
        });
    }

    {
        let weak = ui.as_weak();
        ui.preferences()
            .on_romanization_language(move |code, selected| {
                let Some(ui) = weak.upgrade() else {
                    return;
                };

                let model = ui.preferences().get_romanization_languages();
                let mut choices: Vec<_> = (0..model.row_count())
                    .filter_map(|i| model.row_data(i))
                    .collect();
                let Some(choice) = choices.iter_mut().find(|choice| choice.code == code) else {
                    return;
                };
                if choice.selected == selected {
                    return;
                }
                choice.selected = selected;
                ui.preferences().set_romanization_selected_count(
                    choices.iter().filter(|choice| choice.selected).count() as i32,
                );
                ui.preferences()
                    .set_romanization_languages(Rc::new(VecModel::from(choices)).into());
                ui.preferences().invoke_save_settings();
            });
    }
    {
        let tx = tx.clone();
        ui.player().on_refresh_outputs(move || {
            let _ = tx.send(Request::OutputDevices);
        });
    }
    {
        let tx = tx.clone();
        ui.player().on_retry_output(move || {
            let _ = tx.send(Request::RetryOutput);
        });
    }
    {
        let weak = ui.as_weak();
        ui.player().on_select_output(move |index| {
            use slint::Model;
            if let Some(ui) = weak.upgrade() {
                if let Some(choice) = ui
                    .player()
                    .get_output_choices()
                    .row_data(index.max(0) as usize)
                {
                    ui.player().set_output_device_id(choice.id);
                    ui.player().set_output_device_index(index);
                    ui.preferences().invoke_save_settings();
                }
            }
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        let timer = timer.clone();
        ui.preferences().on_save_settings(move || {
            let tx = tx.clone();
            let weak = weak.clone();
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(150),
                move || {
                    if let Some(ui) = weak.upgrade() {
                        let _ = tx.send(Request::Settings(settings_from_ui(&ui)));

                        if ui.shell().get_view() == "settings"
                            && !ui.shell().get_keyboard_navigation()
                            && !ui.shell().get_text_input_focused()
                            && !ui.shell().get_modal_open()
                        {
                            ui.invoke_focus_shell();
                        }
                    }
                },
            );
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.preferences().on_font_action(move |action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };

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
            } else if action == "system" || action == "preset" {
                ui.preferences().set_font_family(
                    if action == "system" {
                        ""
                    } else {
                        "Plus Jakarta Sans"
                    }
                    .into(),
                );
                ui.preferences().invoke_save_settings();
            } else if action == "custom" {
                if !ui.preferences().get_font_path().is_empty()
                    && !ui.preferences().get_custom_font_family().is_empty()
                {
                    ui.preferences()
                        .set_font_family(ui.preferences().get_custom_font_family());
                    ui.preferences().invoke_save_settings();
                }
            } else if action == "remove" {
                let was_custom = !ui.preferences().get_font_path().is_empty()
                    && ui.preferences().get_font_family()
                        == ui.preferences().get_custom_font_family();
                ui.preferences().set_font_path("".into());
                ui.preferences().set_custom_font_family("".into());
                if was_custom {
                    ui.preferences().set_font_family("Plus Jakarta Sans".into());
                }
                ui.preferences().invoke_save_settings();
            }
        });
    }
}
