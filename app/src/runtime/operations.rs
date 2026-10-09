//! Present typed operation results; JSON is limited to the UI's staged draft property.
use super::RuntimeModels;
use crate::{
    controllers::collections::{collection_seed_matches, stage_collection_draft},
    protocol::{OperationRequest, OperationResult, Request},
    AppState, OrcaWindow,
};
use orca_services::{
    operation_types::{CollectionEdit, CollectionIdentity, LyricsStatus},
    types::Group,
};
use slint::ComponentHandle;

fn seed_matches(seed: &str, current: &str) -> bool {
    collection_seed_matches(&serde_json::Value::String(seed.into()), current)
}
fn matches_detail(identity: &CollectionIdentity, state: &AppState) -> bool {
    identity.kind.as_str() == state.get_view().as_str()
        && identity.key == state.get_detail_key().as_str()
        && identity.secondary == state.get_detail_secondary().as_str()
}
fn completes_current_edit(edit: &CollectionEdit, current: &str) -> bool {
    edit.draft_id.is_some()
        && matches!(serde_json::from_str::<OperationRequest>(current), Ok(OperationRequest::CollectionEdit(current)) if edit.identity == current.identity && edit.draft_id == current.draft_id)
}
pub(super) fn apply(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    request: OperationRequest,
    result: OperationResult,
) -> bool {
    let state = ui.global::<AppState>();
    let mut update_images = false;
    match (&request, result) {
        (OperationRequest::CollectionEdit(edit), OperationResult::CollectionEdited) => {
            state.set_collection_saving(false);
            if matches_detail(&edit.identity, &state) {
                if let Some(name) = &edit.name {
                    if matches!(edit.identity.kind.as_str(), "artists" | "albums" | "genres") {
                        state.set_detail_key(name.clone().into());
                        state.set_detail_title(name.clone().into());
                    }
                }
            }
            if state.get_collection_dialog()
                && completes_current_edit(edit, &state.get_collection_request())
            {
                state.set_collection_saving(false);
                state.set_collection_dialog(false);
            }
            let _ = models.service.tx.send(Request::Reconcile);
            state.invoke_browse();
        }
        (OperationRequest::CollectionPreview(preview), OperationResult::Detail(detail))
            if state.get_collection_dialog()
                && seed_matches(&preview.collection_seed, &state.get_collection_request()) =>
        {
            if let Ok(mut draft) =
                serde_json::from_str::<serde_json::Value>(&state.get_collection_request())
            {
                draft["preview"] = serde_json::json!(detail.artwork.unwrap_or_default());
                state.set_collection_request(draft.to_string().into());
                update_images = true;
            }
        }
        (OperationRequest::FetchCover(fetch), OperationResult::Artwork(artwork)) => {
            if fetch.collection_draft
                && state.get_collection_dialog()
                && seed_matches(&fetch.collection_seed, &state.get_collection_request())
            {
                crate::controllers::collections::finish_collection_fetch(
                    ui,
                    fetch,
                    "Artwork ready. Save to apply it.",
                );
                if let Ok(mut draft) =
                    serde_json::from_str::<serde_json::Value>(&state.get_collection_request())
                {
                    draft["cover"] = serde_json::json!(artwork.artwork);
                    update_images |= stage_collection_draft(ui, &draft);
                }
            } else if fetch.editor_path.is_some() {
                if state.get_metadata_open()
                    && crate::metadata::artwork_target_matches(
                        fetch,
                        &state.get_draft(),
                        state.get_editor_generation(),
                    )
                {
                    state.set_editor_busy(false);
                    if !crate::metadata::artwork_tags_match(fetch, &state.get_draft()) {
                        state.set_editor_status(
                            "Tags changed during search. Fetch artwork again.".into(),
                        );
                        return false;
                    }
                    let mut draft = state.get_draft();
                    draft.cover_to_embed = artwork.artwork.into();
                    draft.remove_cover = false;
                    state.set_draft(draft);
                    state.set_editor_status("Artwork ready — Save metadata to embed it".into());
                    update_images = true;
                }
            } else if !fetch.collection_draft {
                state.invoke_browse();
            }
        }
        (OperationRequest::FetchLyrics(fetch), OperationResult::Lyrics(lyrics))
            if state.get_metadata_open()
                && fetch.editor_path.as_deref() == Some(state.get_draft().path.as_str()) =>
        {
            if fetch
                .editor_generation
                .is_some_and(|at| at != state.get_editor_generation())
            {
                return false;
            }
            let current = state.get_draft();
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
                state.set_editor_busy(false);
                state.set_editor_status("Tags changed during search. Fetch lyrics again.".into());
                return false;
            }
            let mut draft = state.get_draft();
            if let Some(text) = lyrics.lyrics {
                draft.lyrics = text.into();
            }
            state.set_editor_status(
                match lyrics.status {
                    LyricsStatus::Ok | LyricsStatus::Cached => {
                        "Lyrics ready - Save metadata to embed them"
                    }
                    LyricsStatus::NotFound => "No lyrics found for this song",
                    LyricsStatus::Offline => "Could not connect. Please try again :(",
                }
                .into(),
            );
            state.set_draft(draft);
            state.set_editor_busy(false);
            update_images = true;
        }
        (OperationRequest::RemoveSource { .. }, OperationResult::SourceRemoved(_)) => {
            let _ = models.service.tx.send(Request::SourceChanged);
            state.invoke_browse();
        }
        (OperationRequest::ImportPlaylist { .. }, OperationResult::PlaylistImported(_)) => {
            state.invoke_browse()
        }
        (
            OperationRequest::ArtistDetail { .. } | OperationRequest::GroupDetail(_),
            OperationResult::Detail(detail),
        ) if matches_detail(&detail.identity, &state) => {
            state.set_detail_title(detail.title.into());
            state.set_detail_summary(
                format!(
                    "{}{} songs · {} mins",
                    if state.get_view() == "artists" {
                        format!("{} albums · ", detail.album_count)
                    } else {
                        String::new()
                    },
                    detail.count,
                    detail.duration / 60000
                )
                .into(),
            );
            *models.detail_art.borrow_mut() = (
                detail.artwork.unwrap_or_default(),
                detail.backdrop.unwrap_or_default(),
            );
            *models.related.groups.borrow_mut() = detail
                .albums
                .into_iter()
                .map(|album| Group {
                    key: album.navigation_title,
                    secondary: album.artist,
                    title: album.title,
                    subtitle: String::new(),
                    artwork: album.artwork.unwrap_or_default(),
                    artwork_tiles: vec![],
                    count: album.song_count,
                })
                .collect();
            models.related.reset();
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
