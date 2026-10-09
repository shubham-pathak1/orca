use crate::{
    operation_types::{LyricsResult, LyricsStatus, OperationRequest, OperationResult, ReadLyrics},
    Backend,
};
use orca_core::{db, library};
use std::path::Path;

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    run_with_lyrics_lookup(
        backend,
        operation,
        stop,
        orca_core::lyrics::fetch_lyrics_with_album_cancellable,
    )
}

fn run_with_lyrics_lookup(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
    lookup: impl FnOnce(
        &str,
        &str,
        &str,
        u64,
        Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<String, String>,
) -> Result<OperationResult, String> {
    let mut response;
    match operation {
        OperationRequest::FetchLyrics(fetch) => {
            let song = backend.track(&fetch.key)?;
            // Fetch is an explicit refresh. Reading saved lyrics and automatic
            // missing-lyrics requests have separate cache/removal policies.
            let title = fetch.title.as_deref().unwrap_or(&song.title);
            let artist = fetch.artist.as_deref().unwrap_or(&song.artist);
            let lyrics = match lookup(
                title,
                artist,
                fetch.album.as_deref().unwrap_or(&song.album),
                song.duration_ms,
                stop,
            ) {
                Ok(text) => text,
                Err(error) => {
                    if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
                        return Err("Lyrics search cancelled".into());
                    }
                    let status = if error.to_lowercase().contains("not found")
                        || error.to_lowercase().contains("no lyrics")
                    {
                        LyricsStatus::NotFound
                    } else {
                        LyricsStatus::Offline
                    };
                    return Ok(OperationResult::Lyrics(LyricsResult {
                        path: song.path,
                        lyrics: None,
                        status,
                    }));
                }
            };
            if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
                return Err("Lyrics search cancelled".into());
            }
            if fetch.cache && !lyrics.is_empty() {
                db::set_lyrics(&backend.conn, &song.path, &lyrics)?;
            }
            response = OperationResult::Lyrics(LyricsResult {
                status: if lyrics.is_empty() {
                    LyricsStatus::NotFound
                } else {
                    LyricsStatus::Ok
                },
                lyrics: Some(lyrics),
                path: song.path,
            });
        }
        OperationRequest::Cover { .. } | OperationRequest::RemoveCover { .. } => {
            response = OperationResult::CoverUpdated(None);
            let (remove, kind, key, image, id) = match operation {
                OperationRequest::Cover {
                    kind,
                    key,
                    file,
                    id,
                } => (false, kind.as_str(), key.as_str(), file.as_str(), *id),
                OperationRequest::RemoveCover { kind, key, id } => {
                    (true, kind.as_str(), key.as_str(), "", *id)
                }
                _ => unreachable!(),
            };
            if !remove && !Path::new(image).is_file() {
                return Err("Cover file not found".into());
            }
            match kind {
                "artists" => {
                    if remove {
                        db::remove_artist_artwork(&backend.conn, key)?;
                    } else {
                        db::update_artist_artwork(&backend.conn, key, Some(image), None)?;
                    }
                }
                "albums" => {
                    if remove {
                        db::remove_album_artwork(&backend.conn, key)?;
                    } else {
                        db::update_album_artwork(&backend.conn, key, Some(image), None)?;
                    }
                }
                "playlists" => db::update_playlist_cover(
                    &backend.conn,
                    id,
                    if remove { None } else { Some(image) },
                )?,
                "song" => {
                    if remove {
                        library::remove_song_cover(Path::new(key))?;
                    } else {
                        library::replace_song_cover(Path::new(key), Path::new(image))?;
                    }
                    let song = library::scan_music_file(
                        Path::new(key),
                        &backend.data_dir.join("artwork"),
                    )?;
                    db::save_songs_to_db(&backend.conn, &[song])?;
                    response = OperationResult::CoverUpdated(Some(backend.read_metadata(key)?));
                }
                _ => return Err("Unknown cover type".into()),
            }
        }
        OperationRequest::ReadLyrics { key, file } => {
            response = OperationResult::ReadLyrics(ReadLyrics {
                path: key.clone(),
                lyrics: crate::media::read_text(Path::new(file), 1024 * 1024)?
                    .trim_start_matches('\u{feff}')
                    .into(),
            });
        }
        _ => return Err("Invalid media operation routing".into()),
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_types::LyricsFetch;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            Self(std::env::temp_dir().join(format!(
                "orca-manual-lyrics-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn manual_fetch_refreshes_saved_or_removed_lyrics_and_failure_preserves_saved_state() {
        let fixture = Fixture::new();
        let backend = crate::new_backend(fixture.0.to_str().unwrap(), false).unwrap();
        backend.conn.execute("INSERT INTO songs(path,title,artist,album,duration) VALUES('song.flac','Song','Artist','Album',120000)", []).unwrap();
        let request = OperationRequest::FetchLyrics(LyricsFetch {
            key: "song.flac".into(),
            title: None,
            artist: None,
            album: None,
            cache: false,
            editor_path: Some("song.flac".into()),
            editor_generation: Some(1),
        });
        for saved in ["Old unsynced lyrics", ""] {
            db::set_lyrics(&backend.conn, "song.flac", saved).unwrap();
            let result = run_with_lyrics_lookup(
                &backend,
                &request,
                None,
                |title, artist, album, duration, _| {
                    assert_eq!(
                        (title, artist, album, duration),
                        ("Song", "Artist", "Album", 120000)
                    );
                    Ok("[00:01.00]Fresh synced lyrics".into())
                },
            )
            .unwrap();
            let OperationResult::Lyrics(result) = result else {
                panic!("expected lyrics");
            };
            assert!(matches!(result.status, LyricsStatus::Ok));
            assert_eq!(
                result.lyrics.as_deref(),
                Some("[00:01.00]Fresh synced lyrics")
            );
            assert_eq!(
                db::get_lyrics(&backend.conn, "song.flac").as_deref(),
                Some(saved)
            );
            let failed = run_with_lyrics_lookup(&backend, &request, None, |_, _, _, _, _| {
                Err("offline".into())
            })
            .unwrap();
            let OperationResult::Lyrics(failed) = failed else {
                panic!("expected lyrics");
            };
            assert!(matches!(failed.status, LyricsStatus::Offline));
            assert!(failed.lyrics.is_none());
            let stop = AtomicBool::new(false);
            assert!(
                run_with_lyrics_lookup(&backend, &request, Some(&stop), |_, _, _, _, _| {
                    stop.store(true, Ordering::Relaxed);
                    Err("offline".into())
                })
                .unwrap_err()
                .contains("cancelled")
            );
            assert_eq!(
                db::get_lyrics(&backend.conn, "song.flac").as_deref(),
                Some(saved)
            );
        }
    }
}
