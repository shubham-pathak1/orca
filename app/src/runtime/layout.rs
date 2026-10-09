//! Pure geometry policy shared by resize and DPI updates.
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
            .round() as u32,
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
    let compact = matches!(view, "artists" | "playlists")
        || view == "folders" && (!folder_grid || has_detail);
    let gap = match view {
        "playlists" => 24.0,
        "genres" => 16.0,
        _ => 12.0,
    };
    let usable = (available - 44.0).max(1.0);
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
        ((usable - gap * (columns - 1) as f32) / columns as f32 * dpr)
            .max(48.0)
            .round() as u32
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
}
