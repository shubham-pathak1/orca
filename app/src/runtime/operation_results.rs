//! Apply typed operation results to the matching editor, collection or playback view.
use super::RuntimeModels;
use crate::{
    controllers::collections::{collection_seed_matches, stage_collection_draft},
    protocol::{OperationRequest, OperationResult, Request},
    OrcaWindow,
};
use orca_services::{
    operation_types::{CollectionEdit, CollectionIdentity, LyricsStatus},
    types::Group,
};

fn seed_matches(seed: &str, current: &str) -> bool {
    collection_seed_matches(&serde_json::Value::String(seed.into()), current)
}
fn matches_detail(identity: &CollectionIdentity, ui: &OrcaWindow) -> bool {
    identity.kind.as_str() == ui.shell().get_view().as_str()
        && identity.key == ui.library().get_detail_key().as_str()
        && identity.secondary == ui.library().get_detail_secondary().as_str()
}
fn completes_current_edit(edit: &CollectionEdit, current: &str) -> bool {
    edit.draft_id.is_some()
        && matches!(serde_json::from_str::<OperationRequest>(current), Ok(OperationRequest::CollectionEdit(current)) if edit.identity == current.identity && edit.draft_id == current.draft_id)
}
pub(super) fn apply_operation_result(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    request: OperationRequest,
    result: OperationResult,
) -> bool {
    let mut update_images = false;
    match (&request, result) {
        (_, OperationResult::ArtworkBulk(progress)) => {
            ui.library().set_artwork_bulk_started(progress.started);
            ui.library().set_artwork_bulk_running(progress.running);
            ui.library().set_artwork_bulk_complete(progress.complete);
            ui.library().set_artwork_bulk_progress(
                format!(
                    "{} · {} collections checked · {} images found",
                    if progress.complete {
                        "Finished"
                    } else if progress.waiting {
                        "Waiting for provider"
                    } else if progress.running {
                        "Fetching"
                    } else {
                        "Paused"
                    },
                    progress.attempted,
                    progress.found
                )
                .into(),
            );
        }
        (OperationRequest::CollectionEdit(edit), OperationResult::CollectionEdited) => {
            ui.dialogs().set_collection_saving(false);
            if matches_detail(&edit.identity, ui) {
                if let Some(name) = &edit.name {
                    if matches!(edit.identity.kind.as_str(), "artists" | "albums" | "genres") {
                        ui.library().set_detail_key(name.clone().into());
                        ui.library().set_detail_title(name.clone().into());
                    }
                }
            }
            if ui.dialogs().get_collection_dialog()
                && completes_current_edit(edit, &ui.dialogs().get_collection_request())
            {
                ui.dialogs().set_collection_saving(false);
                ui.dialogs().set_collection_dialog(false);
            }
            let _ = models.worker.tx.send(Request::Reconcile);
            ui.library().invoke_browse();
        }
        (OperationRequest::CollectionPreview(preview), OperationResult::Detail(detail))
            if ui.dialogs().get_collection_dialog()
                && seed_matches(
                    &preview.collection_seed,
                    &ui.dialogs().get_collection_request(),
                ) =>
        {
            if let Ok(mut draft) =
                serde_json::from_str::<serde_json::Value>(&ui.dialogs().get_collection_request())
            {
                draft["preview"] = serde_json::json!(detail.artwork.unwrap_or_default());
                ui.dialogs()
                    .set_collection_request(draft.to_string().into());
                update_images = true;
            }
        }
        (OperationRequest::FetchCover(fetch), OperationResult::Artwork(artwork)) => {
            if fetch.collection_draft
                && ui.dialogs().get_collection_dialog()
                && seed_matches(
                    &fetch.collection_seed,
                    &ui.dialogs().get_collection_request(),
                )
            {
                crate::controllers::collections::finish_collection_fetch(
                    ui,
                    fetch,
                    "Artwork ready. Save to apply it.",
                );
                if let Ok(mut draft) = serde_json::from_str::<serde_json::Value>(
                    &ui.dialogs().get_collection_request(),
                ) {
                    draft["cover"] = serde_json::json!(artwork.artwork);
                    update_images |= stage_collection_draft(ui, &draft);
                }
            } else if fetch.editor_path.is_some() {
                if ui.dialogs().get_metadata_open()
                    && crate::metadata::artwork_target_matches(
                        fetch,
                        &ui.dialogs().get_draft(),
                        ui.dialogs().get_editor_generation(),
                    )
                {
                    ui.dialogs().set_editor_busy(false);
                    if !crate::metadata::artwork_tags_match(fetch, &ui.dialogs().get_draft()) {
                        ui.dialogs().set_editor_status(
                            "Tags changed during search. Fetch artwork again.".into(),
                        );
                        return false;
                    }
                    let mut draft = ui.dialogs().get_draft();
                    draft.cover_to_embed = artwork.artwork.into();
                    draft.remove_cover = false;
                    ui.dialogs().set_draft(draft);
                    ui.dialogs()
                        .set_editor_status("Artwork ready — Save metadata to embed it".into());
                    update_images = true;
                }
            } else if !fetch.collection_draft {
                ui.library().invoke_browse();
            }
        }
        (OperationRequest::FetchLyrics(fetch), OperationResult::Lyrics(lyrics))
            if ui.dialogs().get_metadata_open()
                && fetch.editor_path.as_deref() == Some(ui.dialogs().get_draft().path.as_str()) =>
        {
            if fetch
                .editor_generation
                .is_some_and(|generation| generation != ui.dialogs().get_editor_generation())
            {
                return false;
            }
            let current = ui.dialogs().get_draft();
            if fetch
                .title
                .as_deref()
                .is_some_and(|title| title.trim() != current.title.trim())
                || fetch
                    .artist
                    .as_deref()
                    .is_some_and(|artist| artist.trim() != current.artist.trim())
                || fetch
                    .album
                    .as_deref()
                    .is_some_and(|album| album.trim() != current.album.trim())
            {
                ui.dialogs().set_editor_busy(false);
                ui.dialogs()
                    .set_editor_status("Tags changed during search. Fetch lyrics again.".into());
                return false;
            }
            let mut draft = ui.dialogs().get_draft();
            if let Some(text) = lyrics.lyrics {
                draft.lyrics = text.into();
            }
            ui.dialogs().set_editor_status(
                match lyrics.status {
                    LyricsStatus::Ok | LyricsStatus::Cached => {
                        "Lyrics ready - Save metadata to embed them"
                    }
                    LyricsStatus::NotFound => "No lyrics found for this song",
                    LyricsStatus::Offline => "Could not connect. Please try again.",
                }
                .into(),
            );
            ui.dialogs().set_draft(draft);
            ui.dialogs().set_editor_busy(false);
            update_images = true;
        }
        (OperationRequest::RemoveSource { .. }, OperationResult::SourceRemoved(_)) => {
            let _ = models.worker.tx.send(Request::SourceChanged);
            ui.library().invoke_browse();
        }
        (OperationRequest::ImportPlaylist { .. }, OperationResult::PlaylistImported(_)) => {
            ui.library().invoke_browse()
        }
        (
            OperationRequest::ArtistDetail { .. } | OperationRequest::GroupDetail(_),
            OperationResult::Detail(detail),
        ) if matches_detail(&detail.identity, ui) => {
            ui.library().set_detail_title(detail.title.into());
            ui.library()
                .set_detail_year(detail.year.unwrap_or_default());
            ui.library().set_detail_summary(
                format!(
                    "{}{} {} · {} {}",
                    if ui.shell().get_view() == "artists" {
                        format!(
                            "{} {} · ",
                            detail.album_count,
                            if detail.album_count == 1 {
                                "album"
                            } else {
                                "albums"
                            }
                        )
                    } else {
                        String::new()
                    },
                    detail.count,
                    if detail.count == 1 { "song" } else { "songs" },
                    detail.duration / 60000,
                    if detail.duration / 60000 == 1 {
                        "min"
                    } else {
                        "mins"
                    }
                )
                .into(),
            );
            *models.catalog.detail_art.borrow_mut() = (
                detail.artwork.unwrap_or_default(),
                detail.backdrop.unwrap_or_default(),
            );
            *models.catalog.related.groups.borrow_mut() = detail
                .albums
                .into_iter()
                .map(|album| Group {
                    year: album.year,
                    key: album.navigation_title,
                    secondary: album.artist,
                    title: album.title,
                    subtitle: String::new(),
                    artwork: album.artwork.unwrap_or_default(),
                    artwork_tiles: vec![],
                    count: album.song_count,
                })
                .collect();
            models.catalog.related.reset();
            update_images = true;
        }
        _ => {}
    }
    update_images
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn stale_edit_completion_cannot_close_a_new_editor() {
        let current = json!({"action":"collection-edit","kind":"albums","key":"Album","secondary":"Artist","draft_id":"new","preview":"full.png"});
        let OperationRequest::CollectionEdit(mut edit) =
            OperationRequest::from_value(current.clone()).unwrap()
        else {
            panic!()
        };
        assert!(completes_current_edit(&edit, &current.to_string()));
        edit.draft_id = Some("old".into());
        assert!(!completes_current_edit(&edit, &current.to_string()));
        edit.draft_id = Some("new".into());
        edit.identity.secondary = "Other Artist".into();
        assert!(!completes_current_edit(&edit, &current.to_string()));
        assert!(!completes_current_edit(&edit, "invalid draft"));
    }
}
