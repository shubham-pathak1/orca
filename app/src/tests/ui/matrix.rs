use super::*;

pub(super) fn verify_keyboard_and_display_matrix(ui: &OrcaWindow, window: &MinimalSoftwareWindow) {
    fn press(window: &MinimalSoftwareWindow, text: slint::SharedString) {
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }
    let state = ui.global::<AppState>();
    state.set_full_player(false);
    state.set_metadata_open(false);
    state.set_collection_dialog(false);
    state.set_playlist_dialog(false);
    state.set_queue_open(false);
    state.set_menu_open(false);
    state.set_collection_menu_open(false);
    state.set_detail_key("".into());
    state.set_view("songs".into());
    state.set_search("".into());
    let (tx, rx) = std::sync::mpsc::channel();
    install_keyboard(ui, tx);
    ui.invoke_focus_shell();
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    press(window, slint::platform::Key::Tab.into());
    assert!(
        state.get_keyboard_navigation(),
        "Tab enables visible focus rings"
    );
    press(window, "\n".into());
    assert_eq!(state.get_view(), "songs", "first Tab target is Library");
    press(window, slint::platform::Key::Tab.into());
    press(window, " ".into());
    assert_eq!(
        state.get_view(),
        "artists",
        "Space activates focused navigation"
    );
    assert!(
        rx.try_recv().is_err(),
        "button Space must not toggle global playback"
    );
    state.set_draft(MetadataDraft {
        path: "fixture.wav".into(),
        title: "Keyboard draft".into(),
        ..Default::default()
    });
    state.set_metadata_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    press(window, " ".into());
    assert!(
        state.get_draft().title.contains(' '),
        "title field accepts spaces"
    );
    assert!(
        rx.try_recv().is_err(),
        "typing in the editor cannot control playback"
    );
    press(window, slint::platform::Key::Escape.into());
    assert!(!state.get_metadata_open());
    assert!(
        !state.get_text_input_focused(),
        "Escape restores shell keyboard handling"
    );

    let views = [
        "songs",
        "artists",
        "albums",
        "genres",
        "playlists",
        "folders",
        "settings",
    ];
    let mut cases = 0;
    for dpi in [1.0f32, 1.25, 1.5, 2.0] {
        state.set_display_dpr(dpi);
        for light in [false, true] {
            for minimal in [false, true] {
                for blur in [false, true] {
                    for (width, height) in [(500u32, 600u32), (1280, 820)] {
                        state.set_light_theme(light);
                        state.set_browse_artwork(!minimal);
                        state.set_compact_artwork(!minimal);
                        state.set_full_artwork(!minimal);
                        state.set_blurred_background(blur);
                        state.set_view(views[cases % views.len()].into());
                        state.set_full_player(cases % 8 == 0);
                        state.set_metadata_open(cases % 8 == 1);
                        state.set_collection_dialog(cases % 8 == 2);
                        state.set_playlist_dialog(cases % 8 == 3);
                        window
                            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpi });
                        let physical_width = (width as f32 * dpi) as u32;
                        let physical_height = (height as f32 * dpi) as u32;
                        window.set_size(slint::PhysicalSize::new(physical_width, physical_height));
                        let mut pixels = vec![
                            slint::Rgb8Pixel::default();
                            (physical_width * physical_height) as usize
                        ];
                        window.draw_if_needed(|renderer| {
                            renderer.render(&mut pixels, physical_width as usize);
                        });
                        assert!(
                            pixels
                                .iter()
                                .any(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0),
                            "display matrix must render content"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 64);
    state.set_metadata_open(false);
    state.set_collection_dialog(false);
    state.set_playlist_dialog(false);
    state.set_full_player(false);
    state.set_display_dpr(1.0);
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
}
