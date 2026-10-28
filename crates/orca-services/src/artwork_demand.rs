//! Resolve bounded UI demand into collection lookups; never enumerate the library.
use crate::{
    operation_types::{
        ArtworkDemand, CollectionIdentity, CollectionKind, CoverFetch, OperationRequest,
    },
    Backend,
};
use orca_core::{artist_credits, db};
use std::collections::HashSet;

impl Backend {
    pub fn artwork_demand_jobs(
        &self,
        demand: &ArtworkDemand,
    ) -> Result<Vec<OperationRequest>, String> {
        self.artwork_demand_jobs_excluding(demand, &HashSet::new())
    }
    pub fn artwork_demand_jobs_excluding(
        &self,
        demand: &ArtworkDemand,
        attempted: &HashSet<CollectionIdentity>,
    ) -> Result<Vec<OperationRequest>, String> {
        let mut candidates = Vec::new();
        let ids: Vec<_> = demand
            .playing
            .into_iter()
            .chain(demand.tracks.iter().copied())
            .filter(|id| *id > 0)
            .take(129)
            .collect();
        for track in self.tracks_by_ids(&ids)? {
            if track.artwork.is_empty() && track.artwork_thumb.is_empty() {
                candidates.push(CollectionIdentity {
                    kind: CollectionKind::Albums,
                    key: track.album,
                    secondary: track.album_artist,
                });
            }
            for name in artist_credits::primary_artists(&track.artist, &track.title)
                .into_iter()
                .take(8)
            {
                candidates.push(CollectionIdentity {
                    kind: CollectionKind::Artists,
                    key: name,
                    secondary: String::new(),
                });
            }
        }
        candidates.extend(demand.collections.iter().take(64).cloned());
        self.artwork_jobs_for_collections(
            candidates
                .into_iter()
                .filter(|candidate| !attempted.contains(candidate)),
        )
    }

    pub(crate) fn artwork_jobs_for_collections(
        &self,
        candidates: impl IntoIterator<Item = CollectionIdentity>,
    ) -> Result<Vec<OperationRequest>, String> {
        db::refresh_artist_index(&self.conn)?;
        let overrides = self.collection_overrides()?;
        let mut seen = HashSet::new();
        let mut jobs = Vec::new();
        for candidate in candidates {
            let kind = candidate.kind.as_str();
            let key = candidate.key.trim();
            let secondary = candidate.secondary.trim();
            if !matches!(
                candidate.kind,
                CollectionKind::Artists | CollectionKind::Albums
            ) || key.is_empty()
                || key.len() > 512
                || secondary.len() > 512
                || candidate.kind == CollectionKind::Artists && key == "Unknown Artist"
                || candidate.kind == CollectionKind::Albums
                    && (key == "Unknown Album"
                        || secondary.is_empty()
                        || secondary == "Unknown Artist")
                || !seen.insert((
                    kind.to_string(),
                    key.to_lowercase(),
                    secondary.to_lowercase(),
                ))
            {
                continue;
            }
            let identity = serde_json::json!([kind, key, secondary]).to_string();
            if overrides
                .get(&identity)
                .and_then(|value| value.get("cover"))
                .is_some()
            {
                continue;
            }
            let album = candidate.kind == CollectionKind::Albums;
            if !db::collection_needs_artwork(
                &self.conn,
                if album { secondary } else { key },
                album.then_some(key),
            )? {
                continue;
            }
            jobs.push(OperationRequest::FetchCoverAuto(CoverFetch {
                kind: candidate.kind.clone(),
                key: if album {
                    format!("{secondary}:{key}")
                } else {
                    key.into()
                },
                artist: secondary.into(),
                album: if album { key.into() } else { String::new() },
                title: None,
                track_artist: None,
                duration_ms: None,
                editor_generation: None,
                editor_path: None,
                collection_draft: false,
                collection_seed: String::new(),
            }));
            if jobs.len() == 64 {
                break;
            }
        }
        Ok(jobs)
    }
}
