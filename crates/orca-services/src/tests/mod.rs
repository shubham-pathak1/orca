use super::*;
mod artwork;
mod catalog;
mod collection_names;
mod folders;
mod lyrics;
mod metadata;
mod playlists;
mod recovery;
mod scanning;

use orca_core::{db, library};
use rusqlite::params;
use std::{fs, path::Path, sync::atomic::Ordering, thread, time::Duration};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "orca-native-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn backend(&self) -> Box<Backend> {
        new_backend(self.0.to_str().unwrap(), false).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn seed(b: &Backend) {
    for (path, title, artist, album) in [
        ("一.wav", "100% Love", "A", "Shared"),
        ("two.wav", "Other", "B", "Shared"),
        ("three.wav", "Last", "A", "Solo"),
    ] {
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES(?1,?2,?3,?3,?4,1000)", params![path,title,artist,album]).unwrap();
    }
}
