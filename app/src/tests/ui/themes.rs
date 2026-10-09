use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    state.set_detail_key("".into());
    state.set_view("songs".into());
    state.set_light_theme(true);
    state.set_blurred_background(false);
    state.set_full_player(false);
    state.set_grid(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(pixels[300 * 1280 + 600].r > 240 && pixels[300 * 1280 + 600].b > 230);
    screenshot(pixels, "light-library-smoke");
    state.set_view("settings".into());
    state.set_search("".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "light-settings-smoke");
    state.set_collection_dialog(true);
    state.set_collection_kind("artists".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "light-editor-smoke");
    state.set_collection_dialog(false);
    state.set_now(Song {
        title: "Player navigation".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Album artist".into(),
        path: "a.wav".into(),
        cover_missing: true,
        ..Default::default()
    });
    state.set_full_player(true);
    state.set_full_artwork(false);
    state.set_lyrics_open(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "light-minimal-player-smoke");
    let light_minimal = pixels.clone();
    state.set_light_theme(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        light_minimal[20 * 1280 + 20].r > 240,
        "light player without blur must use the warm background"
    );
    assert!(
        pixels[20 * 1280 + 20].r < 20,
        "dark player without blur must keep its dark background"
    );
    state.set_light_theme(true);

    let targets = Rc::new(RefCell::new(Vec::new()));
    let observed = targets.clone();
    state.on_open_group(move |key, secondary, _| {
        observed
            .borrow_mut()
            .push((key.to_string(), secondary.to_string()))
    });
    click(window, 640.0, 352.0);
    assert!(!state.get_full_player());
    assert_eq!(state.get_view(), "artists");
    assert_eq!(
        targets.borrow().last(),
        Some(&("Artist".to_string(), String::new()))
    );
    state.set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 640.0, 370.0);
    assert!(!state.get_full_player());
    assert_eq!(state.get_view(), "albums");
    assert_eq!(
        targets.borrow().last(),
        Some(&("Album".to_string(), "Album artist".to_string()))
    );
    state.set_full_artwork(true);
    state.set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "light-full-player-smoke");
    click(window, 640.0, 514.0);
    assert!(
        !state.get_full_player(),
        "full artwork artist link must leave the player"
    );
    assert_eq!(state.get_view(), "artists");
    state.set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 640.0, 533.0);
    assert!(
        !state.get_full_player(),
        "full artwork album link must leave the player"
    );
    assert_eq!(state.get_view(), "albums");
    state.set_full_player(true);
    state.set_blurred_background(true);
    state.set_lyrics_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "light-lyrics-player-smoke");
    let light_immersive = pixels.clone();
    state.set_light_theme(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        *pixels == light_immersive,
        "full player artwork, blur, lyrics and controls must match across themes"
    );
    state.set_full_player(false);
    state.set_light_theme(true);
    state.set_blurred_background(false);
    state.set_view("songs".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[300 * 1280 + 600].r > 240,
        "leaving the player must preserve the light browsing theme"
    );

    // Saving is asynchronous: retain the visible draft until persistence succeeds.
}
