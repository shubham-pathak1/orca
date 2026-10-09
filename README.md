# Orca

A free, open-source local music player.

Orca is in active development. The current native prerelease targets Windows x64.

## Features

- Offline local playback, search and alphabetical browsing
- Nested folder browsing, with Songs / Folders tabs in narrow windows
- Songs, artists, albums, genres, playlists and folders in list and grid views
- Gapless playback, shuffle, repeat, queue reordering and session restore
- Synced and word-timed lyrics, click-to-seek, LRC import and online lyric fetching
- Waveform and classic seekbars, compact and full-player views
- Song metadata and cover editing, bulk album/artist/genre tag updates
- Online album artwork and artist pictures, custom collection covers
- Playlist creation, editing and M3U import/export
- Light/dark themes, minimal mode, custom fonts, cover-based backgrounds and dynamic accents
- Windows media controls, tray integration, keyboard shortcuts and Phantom Mode

## Getting started

Check [GitHub Releases](https://github.com/shubham-pathak1/orca/releases) for available builds and their release notes.
Run the Windows setup, or extract the portable ZIP and open **Orca.exe**.
Add your music folders in Settings.

Online artwork fetching is experimental. Images may be unavailable or matched
incorrectly; you can choose or replace them manually.

Song edits write audio-file tags and covers. Album, artist and genre renames update
the corresponding song tags; collection covers and playlists stay in Orca.

Back up `%LOCALAPPDATA%\OrcaSlintTauri` with Orca closed before upgrading.
Close Orca before running a newer setup. For portable builds, extract the newer
ZIP into a separate directory. Your profile is reused.

## Contributing

Bug reports, suggestions and contributions are welcome through
[GitHub Issues](https://github.com/shubham-pathak1/orca/issues).
See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for development and checks.
See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the code structure.

## Credits

- [Slint](https://slint.dev/) — native interface.
- [LRCLIB](https://lrclib.net/) — lyrics.
- [iTunes](https://www.apple.com/itunes/), [Deezer](https://www.deezer.com/), [MusicBrainz](https://musicbrainz.org/) and [Cover Art Archive](https://coverartarchive.org/) — music lookup and artwork.
- [Wikimedia](https://commons.wikimedia.org/) — linked artist images.
- [Rodio](https://github.com/RustAudio/rodio) and [Lofty](https://github.com/Serial-ATA/lofty-rs) — audio playback and metadata.

Thanks to their maintainers and contributors.

## License

Orca's code is available under the [MIT License](LICENSE).
Third-party libraries, fonts and online content retain their respective licenses and terms.
