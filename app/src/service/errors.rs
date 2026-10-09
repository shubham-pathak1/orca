/// Keep messages short and actionable; raw diagnostics stay in the console.
pub(crate) fn operation(error: &orca_services::operation_types::OperationError) -> String {
    use orca_services::operation_types::OperationError;
    match error {
        OperationError::Busy => "One moment, Orca is busy. Please try again shortly.".into(),
        OperationError::Unavailable(_) => {
            "Oops, the library worker isn't available. Please restart Orca.".into()
        }
        OperationError::InvalidRequest(_) | OperationError::InvalidResult(_) => {
            "Oops, this library operation couldn't finish. Please try again :(".into()
        }
        OperationError::Failed(message) => friendly(message),
    }
}
pub fn friendly(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.starts_with("library folder is unavailable") {
        "This music folder is unavailable. Reconnect it, or remove it in Settings > Library.".into()
    } else if lower.starts_with("collection edit stopped")
        || lower.starts_with("a collection with this name")
        || lower.starts_with("folder names come")
    {
        error.to_string()
    } else if lower.starts_with("this audio file changed") {
        "This file changed since editing began. Reopen the editor before saving.".into()
    } else if lower.starts_with("metadata saved but") {
        "Tags were saved, but the library update failed. Restart Orca to finish updating the library.".into()
    } else if lower.contains("audio file no longer exists") {
        "Oops, this file is missing. Please check its folder :(".into()
    } else if lower.starts_with("playback:seek:") {
        "Oops, this song couldn't seek there. Please try another position.".into()
    } else if lower.starts_with("playback:queue:") {
        "Oops, the next song couldn't load. Please try another song :(".into()
    } else if lower.starts_with("playback:") {
        "Oops, this song couldn't load. It may be damaged or unsupported :(".into()
    } else if lower.starts_with("output:") || lower.contains("audio device unavailable") {
        "Oops, audio output isn't available. Please check your speakers or headphones.".into()
    } else if lower.contains("permission denied") || lower.contains("access is denied") {
        "Orca can't access this file. Please check its permissions.".into()
    } else if lower.contains("queue is busy") {
        "One moment, Orca is busy. Please try again shortly.".into()
    } else if lower.contains("cannot open") && lower.contains("database") {
        "Oops, the library couldn't open. Please restart Orca.".into()
    } else if lower.starts_with("artwork lookup timed out") {
        "Artwork search timed out. Please try again.".into()
    } else if lower.starts_with("artwork providers are unavailable") {
        "An artwork provider couldn't be reached. Try again later, or choose an image.".into()
    } else if lower.starts_with("artwork providers returned invalid") {
        "No usable artwork could be downloaded. Try choosing an image.".into()
    } else if lower.starts_with("several artists share this name") {
        "Couldn't confidently identify this artist from the available results. Try choosing an image.".into()
    } else if lower.starts_with("artwork not found online") {
        "No matching artwork found. Check the artist and album tags, or choose an image.".into()
    } else if lower.starts_with("artist portrait not found") {
        "Artist identified, but no image is available. Try choosing an image.".into()
    } else if lower.starts_with("enter ")
        || lower.starts_with("missing ")
        || lower.starts_with("invalid ")
    {
        error.to_string()
    } else {
        "Oops, that didn't work. Please try again :(".into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_protocol_failures_keep_diagnostics_out_of_user_messages() {
        use orca_services::operation_types::OperationError;
        assert!(operation(&OperationError::Busy).contains("shortly"));
        assert!(
            operation(&OperationError::Unavailable("SQLite detail".into())).contains("restart")
        );
        for error in [
            OperationError::InvalidRequest("private/file".into()),
            OperationError::InvalidResult("private/file".into()),
        ] {
            assert!(!operation(&error).contains("private"));
        }
    }
    #[test]
    fn failures_have_short_actions_without_exposing_paths_or_driver_details() {
        assert!(friendly("Artist portrait not found").starts_with("Artist identified"));
        assert!(friendly("Several artists share this name").contains("confidently identify"));
        assert!(friendly("This audio file changed since the editor opened").contains("editor"));
        assert!(
            friendly("Metadata saved but library update failed: private/path")
                .to_lowercase()
                .contains("restart")
        );
        assert!(friendly("Audio file no longer exists").contains("folder"));
        assert!(friendly("playback:play: decoder error C:/private/song.flac").contains("damaged"));
        assert!(friendly("playback:seek: unsupported").contains("position"));
        assert!(friendly("output: WASAPI driver 0x123").contains("speakers"));
        assert!(!friendly("output: WASAPI driver 0x123").contains("0x123"));
        assert!(!friendly("SQL error C:/private/orca.db").contains("private"));
        assert_eq!(friendly("Enter a playlist name"), "Enter a playlist name");
    }
}
