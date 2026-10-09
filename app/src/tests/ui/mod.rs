mod browsing;
mod collections;
mod display;
mod editors;
mod keyboard;
mod lyrics;
mod matrix;
mod player;
mod settings;
mod themes;
use crate::controllers::collections::{collection_seed_matches, stage_collection_draft};
use crate::controllers::preferences::register_font;
use crate::platform_integration::set_background_playback;
use crate::presentation::{reset_empty_player, theme_accent};
use crate::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent};
use slint::Model;

struct OffscreenPlatform(Rc<MinimalSoftwareWindow>);
#[test]
fn light_accents_preserve_color_and_dark_accents_stay_unchanged() {
    assert_eq!(theme_accent([240, 200, 130], false), [240, 200, 130]);
    let light = theme_accent([240, 200, 130], true);
    assert!(light[0] < 190 && light[0] > light[1] && light[1] > light[2]);
    assert_eq!(theme_accent([40, 80, 100], true), [40, 80, 100]);
}
#[test]
fn collection_preview_does_not_cancel_fetch_but_manual_cover_changes_do() {
    let seed = serde_json::json!(r#"{"kind":"artists","key":"A","draft_id":"1"}"#);
    assert!(collection_seed_matches(
        &seed,
        r#"{"kind":"artists","key":"A","draft_id":"1","preview":"full.png"}"#
    ));
    assert!(!collection_seed_matches(
        &seed,
        r#"{"kind":"artists","key":"A","draft_id":"1","cover":""}"#
    ));
    assert!(!collection_seed_matches(
        &seed,
        r#"{"kind":"artists","key":"A","draft_id":"2"}"#
    ));
}
impl Platform for OffscreenPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}
fn click(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}
fn save_screenshot(
    output: impl AsRef<std::path::Path>,
    bytes: &[u8],
    width: u32,
    height: u32,
    color: image::ColorType,
) -> image::ImageResult<()> {
    if std::env::var_os("ORCA_TEST_SCREENSHOTS").is_none() {
        return Ok(());
    }
    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(output, bytes, width, height, color)
}
fn screenshot(pixels: &[slint::Rgb8Pixel], name: &str) {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../target/slint/test-artifacts/ui/{name}.png"));
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(output, &bytes, 1280, 820, image::ColorType::Rgb8).unwrap();
}
struct UiCase {
    ui: OrcaWindow,
    window: Rc<MinimalSoftwareWindow>,
    pixels: Vec<slint::Rgb8Pixel>,
    songs: Rc<VecModel<Song>>,
    played: Rc<Cell<i32>>,
    shortcuts: Rc<Cell<i32>>,
}
#[test]
fn software_rendering_and_full_player_input_isolation() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    crate::runtime::tests::verify_completions(&ui);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let state = ui.global::<AppState>();
    let (editor_tx, editor_rx) = std::sync::mpsc::channel();
    controllers::editor::install(&ui, editor_tx);
    state.set_draft(MetadataDraft {
        path: "fixture.flac".into(),
        title: "Song".into(),
        artist: "Artist".into(),
        ..Default::default()
    });
    state.invoke_metadata_action("fetch-cover".into(), "".into());
    assert!(state.get_editor_busy());
    assert_eq!(state.get_editor_operation(), "fetch-cover");
    assert!(matches!(
        editor_rx.try_recv().unwrap(),
        Request::Operation(_)
    ));
    state.invoke_metadata_action("save".into(), "".into());
    assert!(
        editor_rx.try_recv().is_err(),
        "fetch must not submit a metadata save"
    );
    state.set_editor_busy(false);
    state.set_editor_status("".into());
    let weak = ui.as_weak();
    state.on_unfocus(move || {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_focus_shell();
        }
    });
    state.on_setting_matches(|text, search| setting_matches(&text, &search));
    let songs = Rc::new(VecModel::from(vec![Song {
        title: "Test song".into(),
        artist: "Artist".into(),
        path: "fixture.flac".into(),
        ..Default::default()
    }]));
    state.set_songs(songs.clone().into());
    let mut grid_row = vec![songs.row_data(0).unwrap()];
    grid_row.resize(6, Song::default());
    state.set_song_grid(
        Rc::new(VecModel::from(vec![
            Rc::new(VecModel::from(grid_row)).into()
        ]))
        .into(),
    );
    let played = Rc::new(Cell::new(0));
    let observed = played.clone();
    state.on_play(move |_| observed.set(observed.get() + 1));
    ui.show().unwrap();
    let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 820];
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    }));
    assert!(pixels.iter().any(|p| p.r > 100));
    let mut case = UiCase {
        ui,
        window,
        pixels,
        songs,
        played,
        shortcuts: Rc::new(Cell::new(0)),
    };
    browsing::verify(&mut case);
    lyrics::verify(&mut case);
    collections::verify(&mut case);
    settings::verify(&mut case);
    keyboard::verify(&mut case);
    display::verify(&mut case);
    player::verify(&mut case);
    themes::verify(&mut case);
    editors::verify(&mut case);
    matrix::verify_keyboard_and_display_matrix(&case.ui, &case.window);
    collections::verify_narrow_details(&mut case);
    case.ui.hide().unwrap();
}
