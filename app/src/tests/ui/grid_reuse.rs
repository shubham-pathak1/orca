use super::*;

struct Cells {
    reads: Cell<usize>,
    values: VecModel<Song>,
}
impl Model for Cells {
    type Data = Song;
    fn row_count(&self) -> usize {
        self.values.row_count()
    }
    fn row_data(&self, index: usize) -> Option<Song> {
        self.reads.set(self.reads.get() + 1);
        self.values.row_data(index)
    }
    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        self.values.model_tracker()
    }
}

#[test]
fn scrolling_keeps_retained_grid_cells_and_live_source_updates() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    models::install_grid_window(&ui);
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_grid(true);
    ui.preferences().set_browse_artwork(true);
    ui.library().set_columns(6);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let cells: Vec<_> = (0..12)
        .map(|row| {
            Rc::new(Cells {
                reads: Cell::new(0),
                values: VecModel::from(
                    (0..6)
                        .map(|column| Song {
                            index: row * 6 + column,
                            title: format!("Row {row}, cell {column}").into(),
                            path: format!("row-{row}-cell-{column}").into(),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                ),
            })
        })
        .collect();
    let source = Rc::new(VecModel::from(
        cells
            .iter()
            .map(|row| row.clone().into())
            .collect::<Vec<slint::ModelRc<Song>>>(),
    ));
    ui.library().set_song_grid(source.clone().into());
    ui.show().unwrap();
    let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    for row in &cells {
        row.reads.set(0);
    }
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(100.0, 150.0),
        delta_x: 0.0,
        delta_y: -243.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    for row in &cells[1..4] {
        assert_eq!(
            row.reads.get(),
            0,
            "retained cells should not be rebound when another row enters"
        );
    }
    assert!(
        cells[4].reads.get() > 0,
        "the entering row should be loaded"
    );
    assert!(cells[5..].iter().all(|row| row.reads.get() == 0));
    let replacement = Rc::new(VecModel::from(vec![
        Song {
            path: "edited-row".into(),
            title: "Updated source row".into(),
            ..Default::default()
        };
        6
    ]));
    source.set_row_data(2, replacement.into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    let played = Rc::new(RefCell::new(String::new()));
    let observed = played.clone();
    ui.player()
        .on_play(move |path| *observed.borrow_mut() = path.to_string());
    click(&window, 100.0, 350.0);
    assert_eq!(
        &*played.borrow(),
        "edited-row",
        "a retained row must follow source replacements"
    );
    ui.shell().set_view("settings".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    for (index, row) in cells.iter().enumerate() {
        assert_eq!(
            Rc::strong_count(row),
            if index == 2 { 1 } else { 2 },
            "leaving the grid should release viewport handles for row {index}"
        );
    }
    ui.hide().unwrap();
}
