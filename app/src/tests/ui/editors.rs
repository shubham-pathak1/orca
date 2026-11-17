use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let pixels = &mut case.pixels;
    let (edit_tx, edit_rx) = std::sync::mpsc::channel();
    controllers::collections::install(ui, edit_tx);
    ui.dialogs().set_collection_dialog(true);
    ui.dialogs().set_collection_request(r#"{"action":"collection-edit","kind":"artists","key":"Artist","secondary":"","draft_id":"save-test"}"#.into());
    ui.dialogs().invoke_collection_action("fetch-cover".into());
    let Request::Operation(orca_services::operation_types::OperationRequest::FetchCover(fetch)) =
        edit_rx.try_recv().unwrap()
    else {
        panic!("expected artwork fetch");
    };
    assert!(ui.dialogs().get_collection_fetching());
    ui.dialogs().invoke_collection_action("fetch-cover".into());
    assert!(
        edit_rx.try_recv().is_err(),
        "a pending search must not enqueue duplicate requests"
    );
    let mut stale = fetch.clone();
    stale.collection_seed = "{}".into();
    assert!(!controllers::collections::finish_collection_fetch(
        ui, &stale, "stale"
    ));
    assert!(ui.dialogs().get_collection_fetching());
    assert!(controllers::collections::finish_collection_fetch(
        ui,
        &fetch,
        "Artist identified, no image"
    ));
    assert!(!ui.dialogs().get_collection_fetching());
    assert!(ui.dialogs().get_collection_status().contains("identified"));
    ui.dialogs().set_collection_name("Renamed".into());
    ui.dialogs().invoke_collection_action("save-name".into());
    assert!(
        ui.dialogs().get_collection_dialog(),
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
    assert!(ui.dialogs().get_collection_saving());
    ui.dialogs().invoke_collection_action("save-name".into());
    assert!(
        edit_rx.try_recv().is_err(),
        "pending saves must not enqueue another write"
    );
    ui.dialogs().set_collection_saving(false);
    ui.dialogs().set_collection_name("   ".into());
    ui.dialogs().invoke_collection_action("save-name".into());
    assert!(ui.dialogs().get_collection_dialog());
    assert!(
        edit_rx.try_recv().is_err(),
        "blank name must not enqueue a write"
    );
    assert!(ui.shell().get_error().contains("name"));
    ui.shell().set_error("".into());
    ui.dialogs().set_collection_kind("folders".into());
    ui.dialogs().set_collection_request(r#"{"action":"collection-edit","kind":"folders","key":"C:/Music","secondary":"","draft_id":"folder-save-test"}"#.into());
    ui.dialogs().set_collection_name("Music".into());
    ui.player()
        .set_collection_path(r"C:\Users\Test\Music".into());
    ui.dialogs().set_collection_cover_missing(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folder-editor-smoke");
    ui.dialogs().invoke_collection_action("save-name".into());
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
    ui.dialogs().set_collection_saving(false);
    ui.dialogs().set_collection_dialog(false);
    ui.preferences().set_browse_artwork(false);
    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("Artist".into());
    ui.library().set_detail_title("Artist".into());
    ui.library().set_related_albums(
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
