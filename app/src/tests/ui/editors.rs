use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let (edit_tx, edit_rx) = std::sync::mpsc::channel();
    controllers::collections::install(ui, edit_tx);
    state.set_collection_dialog(true);
    state.set_collection_request(r#"{"action":"collection-edit","kind":"artists","key":"Artist","secondary":"","draft_id":"save-test"}"#.into());
    state.invoke_collection_action("fetch-cover".into());
    let Request::Operation(orca_services::operation_types::OperationRequest::FetchCover(fetch)) =
        edit_rx.try_recv().unwrap()
    else {
        panic!("expected artwork fetch");
    };
    assert!(state.get_collection_fetching());
    state.invoke_collection_action("fetch-cover".into());
    assert!(
        edit_rx.try_recv().is_err(),
        "a pending search must not enqueue duplicate requests"
    );
    let mut stale = fetch.clone();
    stale.collection_seed = "{}".into();
    assert!(!controllers::collections::finish_collection_fetch(
        ui, &stale, "stale"
    ));
    assert!(state.get_collection_fetching());
    assert!(controllers::collections::finish_collection_fetch(
        ui,
        &fetch,
        "Artist identified, no image"
    ));
    assert!(!state.get_collection_fetching());
    assert!(state.get_collection_status().contains("identified"));
    state.set_collection_name("Renamed".into());
    state.invoke_collection_action("save-name".into());
    assert!(
        state.get_collection_dialog(),
        "pending save must preserve the editor"
    );
    let Request::Operation(request) = edit_rx.try_recv().unwrap() else {
        panic!("expected save operation");
    };
    let orca_services::operation_types::OperationRequest::CollectionEdit(request) = request else {
        panic!("expected collection edit");
    };
    assert_eq!(request.name.as_deref(), Some("Renamed"));
    assert_eq!(request.draft_id.as_deref(), Some("save-test"));
    assert!(state.get_collection_saving());
    state.invoke_collection_action("save-name".into());
    assert!(
        edit_rx.try_recv().is_err(),
        "pending saves must not enqueue another write"
    );
    state.set_collection_saving(false);
    state.set_collection_name("   ".into());
    state.invoke_collection_action("save-name".into());
    assert!(state.get_collection_dialog());
    assert!(
        edit_rx.try_recv().is_err(),
        "blank name must not enqueue a write"
    );
    assert!(state.get_error().contains("name"));
    state.set_error("".into());
    state.set_collection_kind("folders".into());
    state.set_collection_request(r#"{"action":"collection-edit","kind":"folders","key":"C:/Music","secondary":"","draft_id":"folder-save-test"}"#.into());
    state.set_collection_name("Music".into());
    state.set_collection_path(r"C:\Users\Test\Music".into());
    state.set_collection_cover_missing(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folder-editor-smoke");
    state.invoke_collection_action("save-name".into());
    let Request::Operation(orca_services::operation_types::OperationRequest::CollectionEdit(
        folder,
    )) = edit_rx.try_recv().unwrap()
    else {
        panic!("expected folder cover edit");
    };
    assert!(
        folder.name.is_none(),
        "folder editor must never write a directory alias or file tag"
    );
    state.set_collection_saving(false);
    state.set_collection_dialog(false);
    state.set_browse_artwork(false);
    state.set_view("artists".into());
    state.set_detail_key("Artist".into());
    state.set_detail_title("Artist".into());
    state.set_related_albums(
        Rc::new(VecModel::from(vec![
            Rc::new(VecModel::from(vec![
                CatalogGroup {
                    key: "One".into(),
                    title: "One".into(),
                    count: 1,
                    ..Default::default()
                },
                CatalogGroup {
                    key: "Two".into(),
                    title: "Two".into(),
                    count: 2,
                    ..Default::default()
                },
            ]))
            .into(),
            Rc::new(VecModel::from(vec![
                CatalogGroup {
                    key: "Three".into(),
                    title: "Three".into(),
                    count: 3,
                    ..Default::default()
                },
                CatalogGroup {
                    key: "Four".into(),
                    title: "Four".into(),
                    count: 4,
                    ..Default::default()
                },
            ]))
            .into(),
        ]))
        .into(),
    );
    window.set_size(slint::PhysicalSize::new(1536, 820));
    let mut wide_pixels = vec![slint::Rgb8Pixel::default(); 1536 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut wide_pixels, 1536);
    });
    let bytes: Vec<u8> = wide_pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/minimal-related-albums-smoke.png"),
        &bytes,
        1536,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
}
