//! Convert the current UI selection into a backend query.
use crate::{artwork, models::CatalogModels, protocol::Request, worker::AppWorker, OrcaWindow};
use orca_services::types::Query;
use slint::{ComponentHandle, Model, Timer, TimerMode};
use std::{cell::RefCell, rc::Rc, sync::atomic::Ordering, time::Duration};
pub(crate) fn query(ui: &OrcaWindow) -> (Query, String) {
    let view = ui.shell().get_view().to_string();
    (
        Query {
            search: ui.library().get_search().into(),
            sort: if ui.library().get_detail_key().is_empty() {
                ui.library().get_sort().into()
            } else if ui.library().get_detail_sort().is_empty() {
                ui.library()
                    .get_sort()
                    .split('-')
                    .next()
                    .unwrap_or("artist")
                    .into()
            } else {
                ui.library().get_detail_sort().into()
            },
            kind: if ui.library().get_detail_key().is_empty()
                || !matches!(
                    view.as_str(),
                    "artists"
                        | "albums"
                        | "genres"
                        | "playlists"
                        | "folders"
                        | "composers"
                        | "years"
                ) {
                String::new()
            } else {
                view.clone()
            },
            key: ui.library().get_detail_key().into(),
            secondary: ui.library().get_detail_secondary().into(),
        },
        view,
    )
}

pub(crate) fn install(
    ui: &OrcaWindow,
    service: Rc<AppWorker>,
    catalog: Rc<CatalogModels>,
    cache: Rc<RefCell<artwork::ArtworkCache>>,
    browse: Rc<Timer>,
) {
    install_refresh(ui, service.clone());

    let CatalogModels {
        tracks,
        list,
        grid,
        groups,
        related,
        detail_art,
    } = catalog.as_ref();
    {
        let weak = ui.as_weak();
        let service = service.clone();
        let timer = browse.clone();
        let last_browse = Rc::new(RefCell::new(None::<(orca_services::types::Query, String)>));
        let tracks = tracks.clone();
        let list = list.clone();
        let grid = grid.clone();
        let groups = groups.clone();
        let related = related.clone();
        let detail_art = detail_art.clone();
        ui.library().on_browse(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (query, view) = query(&ui);
            let generation = service.latest_query.fetch_add(1, Ordering::Relaxed) + 1;
            let previous = last_browse.borrow().clone();
            let changed = previous.as_ref() != Some(&(query.clone(), view.clone()));
            let search_changed = previous.as_ref().is_some_and(|(old, old_view)| {
                old_view == &view
                    && old.kind == query.kind
                    && old.key == query.key
                    && old.secondary == query.secondary
                    && old.sort == query.sort
                    && old.search != query.search
            });
            let detail_changed = previous.as_ref().is_none_or(|(old, old_view)| {
                old_view != &view
                    || old.kind != query.kind
                    || old.key != query.key
                    || old.secondary != query.secondary
            });
            if detail_changed && view == "settings" {
                service.artwork_bulk(orca_services::operation_types::ArtworkBulkAction::Status);
                let _ = service.tx.send(Request::OutputDevices);
            }
            *last_browse.borrow_mut() = Some((query.clone(), view.clone()));

            ui.library().set_catalog_loading(view != "settings");
            if changed && !search_changed {
                ui.library().set_library_scroll_index(0);
                ui.library().set_library_scroll_fraction(0.0);
                ui.library().set_library_view_restore(false);
                tracks.clear(generation);
                list.notify.reset();
                grid.reset();
                groups.groups.borrow_mut().clear();
                groups.reset();
                let count = match view.as_str() {
                    "artists" => ui.library().get_known_artists(),
                    "albums" => ui.library().get_known_albums(),
                    "genres" => ui.library().get_known_genres(),
                    "playlists" => ui.library().get_known_playlists(),
                    _ => -1,
                };
                ui.library()
                    .set_catalog_summary(if count >= 0 && query.search.is_empty() {
                        format!(
                            "{count} {}",
                            if count == 1 {
                                view.trim_end_matches('s')
                            } else {
                                &view
                            }
                        )
                        .into()
                    } else {
                        "".into()
                    });
            }
            if search_changed {
                ui.library().set_library_scroll_index(0);
                ui.library().set_library_scroll_fraction(0.0);
                ui.library().set_library_view_restore(false);
                ui.library().set_jump_index(0);
                ui.library()
                    .set_jump_revision(ui.library().get_jump_revision().wrapping_add(1));
            }
            if detail_changed {
                *detail_art.borrow_mut() = (String::new(), String::new());
                related.groups.borrow_mut().clear();
                related.reset();

                ui.library().set_detail_cover(slint::Image::default());
                ui.library().set_detail_backdrop(slint::Image::default());
                ui.library().set_detail_summary("Loading...".into());
            }
            let service = service.clone();
            let delay = if search_changed { 100 } else { 0 };
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(delay),
                move || {
                    let _ =
                        service
                            .tx
                            .send(Request::Browse(generation, query.clone(), view.clone()));
                },
            );
        });
    }
    let catalog_scroll = Rc::new(RefCell::new(None::<(String, f32)>));
    {
        let weak = ui.as_weak();
        let header_art = detail_art.clone();
        let header_albums = related.clone();
        let catalog_groups = groups.clone();
        let header_cache = cache.clone();
        let catalog_scroll = catalog_scroll.clone();
        ui.library().on_open_group(move |key, secondary, title| {
            if let Some(ui) = weak.upgrade() {
                if ui.library().get_catalog_loading() {
                    return;
                }
                let selected = catalog_groups
                    .groups
                    .borrow()
                    .iter()
                    .chain(header_albums.groups.borrow().iter())
                    .find(|group| {
                        group.key == key.as_str() && group.secondary == secondary.as_str()
                    })
                    .cloned();
                if ui.library().get_detail_key().is_empty() {
                    *catalog_scroll.borrow_mut() = Some((
                        ui.shell().get_view().to_string(),
                        ui.library().get_catalog_scroll_y(),
                    ));
                }
                header_art.borrow_mut().0.clear();
                header_art.borrow_mut().1.clear();
                header_albums.groups.borrow_mut().clear();
                header_albums.reset();
                ui.library().set_detail_cover(slint::Image::default());
                ui.library().set_detail_backdrop(slint::Image::default());
                ui.library().set_detail_cover_missing(false);
                ui.library().set_detail_year(0);
                ui.library().set_detail_summary("Loading...".into());
                ui.library().set_detail_key(key);
                ui.library().set_detail_sort("".into());
                ui.library().set_detail_secondary(secondary);
                ui.library().set_detail_title(title);
                ui.library().set_search("".into());
                ui.library().invoke_browse();
                if let Some(group) = selected {
                    if ui.preferences().get_browse_artwork() {
                        ui.library().set_detail_cover(
                            header_cache.borrow_mut().cached_cover(&[&group.artwork]),
                        );
                    }
                    *header_art.borrow_mut() = (group.artwork.clone(), group.artwork);
                    ui.library()
                        .set_detail_summary(format!("{} songs", group.count).into());
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let catalog_scroll = catalog_scroll.clone();
        ui.library().on_go_back(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.shell().get_view() == "folders"
                    && !ui.library().get_detail_secondary().is_empty()
                {
                    ui.library().set_detail_sort("".into());
                    let parent = ui.library().get_detail_secondary().to_string();
                    let root = ui
                        .library()
                        .get_folders()
                        .iter()
                        .any(|root| root.replace('\\', "/").trim_end_matches('/') == parent);
                    let ancestor = if root {
                        String::new()
                    } else {
                        parent
                            .rsplit_once('/')
                            .map(|(path, _)| path.to_string())
                            .unwrap_or_default()
                    };
                    ui.library()
                        .set_detail_title(parent.rsplit('/').next().unwrap_or(&parent).into());
                    ui.library().set_detail_key(parent.into());
                    ui.library().set_detail_secondary(ancestor.into());
                } else {
                    ui.library().set_detail_key("".into());
                    ui.library().set_detail_secondary("".into());
                    ui.library().set_catalog_scroll_restore_pending(false);
                    if let Some((view, position)) = catalog_scroll.borrow().as_ref() {
                        if view == ui.shell().get_view().as_str() {
                            ui.library().set_catalog_scroll_target(*position);
                            ui.library().set_catalog_scroll_restore_pending(true);
                        }
                    }
                }
                ui.library().set_search("".into());
                ui.library().invoke_browse();
            }
        });
    }
}

fn install_refresh(ui: &OrcaWindow, service: Rc<AppWorker>) {
    {
        let tx = service.tx.clone();
        ui.library().on_refresh(move || {
            let _ = tx.send(Request::Refresh);
        });
    }
    {
        let service = service.clone();
        ui.library().on_artwork_bulk(move |action| {
            use orca_services::operation_types::ArtworkBulkAction;
            let action = match action.as_str() {
                "start" => ArtworkBulkAction::Start,
                "pause" => ArtworkBulkAction::Pause,
                "resume" => ArtworkBulkAction::Resume,
                _ => ArtworkBulkAction::Status,
            };
            service.artwork_bulk(action);
        });
    }
}
