//! Pure geometry shared by resize and DPI updates; no model reads or image decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GridGeometry {
    pub columns: usize,
    pub edge: u32,
}
pub(super) fn songs(available: f32, dpr: f32) -> GridGeometry {
    let width = (available - 44.0).max(1.0);
    let columns = (((width + 16.0) / 148.0).floor() as usize).max(1);
    GridGeometry {
        columns,
        edge: ((width - 16.0 * (columns - 1) as f32) / columns as f32 * dpr)
            .max(48.0)
            .floor() as u32,
    }
}
pub(super) fn related(available: f32, dpr: f32, shelf: bool) -> GridGeometry {
    let columns = if shelf {
        2
    } else {
        (((available + 12.0) / 200.0).ceil() as usize).clamp(1, 10)
    };
    let card = if shelf {
        ((available - 48.0) / 6.0).clamp(128.0, 200.0)
    } else {
        ((available - 12.0 * (columns - 1) as f32) / columns as f32).clamp(128.0, 176.0)
    };
    GridGeometry {
        columns,
        edge: ((card - 16.0) * dpr.max(1.0)).ceil() as u32,
    }
}
pub(super) fn catalog(
    width: f32,
    available: f32,
    dpr: f32,
    view: &str,
    folder_grid: bool,
    has_detail: bool,
) -> GridGeometry {
    let compact = matches!(view, "playlists" | "composers" | "years")
        || view == "folders" && (!folder_grid || has_detail);
    let gap = match view {
        "playlists" | "artists" => 24.0,
        "genres" => 16.0,
        _ => 12.0,
    };
    let usable = (available - if view == "folders" { 0.0 } else { 44.0 }).max(1.0);
    let columns = if view == "folders" && (!folder_grid || has_detail) {
        1
    } else if compact {
        if width >= 1536.0 {
            5
        } else if available >= 1024.0 {
            4
        } else if available >= 768.0 {
            3
        } else if available >= 420.0 {
            2
        } else {
            1
        }
    } else {
        let minimum = if matches!(view, "genres" | "folders") {
            220.0
        } else {
            132.0
        };
        (((usable + gap) / (minimum + gap)).floor() as usize).max(1)
    };
    let edge = if compact {
        crate::models::physical_artwork_edge(44, dpr)
    } else {
        let logical = (usable - gap * (columns - 1) as f32) / columns as f32;
        let logical = if view == "artists" {
            logical.min(144.0)
        } else {
            logical
        };
        (logical * dpr).max(48.0).floor() as u32
    };
    GridGeometry { columns, edge }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_and_dpi_keep_logical_columns_and_scale_cover_resolution() {
        for width in [0.0, 320.0, 800.0, 1280.0, 1920.0] {
            let available = (width - 96.0_f32).max(1.0);
            for view in [
                "songs",
                "albums",
                "artists",
                "genres",
                "playlists",
                "folders",
            ] {
                let normal = catalog(width, available, 1.0, view, true, false);
                let scaled = catalog(width, available, 2.0, view, true, false);
                assert!(normal.columns >= 1 && normal.edge >= 44);
                assert_eq!(normal.columns, scaled.columns);
                assert!(scaled.edge >= normal.edge);
            }
            assert!(songs(available, 1.0).columns >= 1);
        }
        assert_eq!(
            catalog(1920.0, 1800.0, 1.0, "folders", false, false).columns,
            1
        );
        assert_eq!(
            catalog(1920.0, 1800.0, 1.0, "folders", false, true).columns,
            1
        );
    }
    #[test]
    fn related_covers_match_the_visible_card_size() {
        let grid = related(1440.0, 1.0, false);
        let shelf = related(1100.0, 1.0, true);
        assert!(grid.columns > 2);
        assert_eq!(shelf.columns, 2);
        assert_eq!(grid.edge, 154);
        assert_eq!(shelf.edge, 160);
    }
}
