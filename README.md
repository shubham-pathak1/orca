<p align="center">
  <img src="app/ui/assets/orca-logo.png" alt="Orca" width="88">
</p>

<h1 align="center">Orca</h1>

<p align="center">A minimal local music player. Built with Rust and Slint.</p>

<p align="center">Currently in alpha. Windows x64.</p>

<p align="center">
  <a href="https://github.com/shubham-pathak1/orca/releases">Download</a> &middot;
  <a href="#screenshots">Screenshots</a> &middot;
  <a href="docs/CONTRIBUTING.md">Contribute</a> &middot;
  <a href="https://github.com/shubham-pathak1/orca/issues">Report a bug</a>
</p>

![Orca library with album artwork, search and playback controls](https://ik.imagekit.io/shubhampathak/orca-web/library.png?v=20261026)

## Features

- **Library** - browse songs, albums, artists, genres and folders.
- **Playback** - gapless playback, shuffle, repeat, an editable queue and session restore.
- **Playlists** - create playlists, save liked songs and import or export M3U files.
- **Lyrics** - synced and word-timed lyrics, click-to-seek, LRC import and optional romanization.
- **Audio Output** - shared, exclusive and unprocessed WASAPI modes for supported devices and formats.
- **Metadata** - edit song tags, embedded artwork and lyrics; rename albums, artists and genres across their songs.
- **Themes** - light and dark themes, artwork accents, custom fonts, classic or waveform seekbars, and a full player.
- **Windows Integration** - media controls, tray controls and keyboard shortcuts.

Playback is local. Optional lyrics, romanization and artwork lookup use online services.

## Get started

1. Download the Windows x64 installer or portable ZIP from [Releases](https://github.com/shubham-pathak1/orca/releases).
2. Install Orca, or extract the ZIP and open **Orca.exe**.
3. Add your music folders in **Settings → Library**. Subfolders are included automatically.

Tag and embedded-artwork edits write to your audio files. Playlists and collection
covers are stored in Orca's profile.

### Updating

Close Orca, then run the new installer or replace the portable app files with the
new build.

## Screenshots

### Lyrics

![Full player with album artwork and synced lyrics](https://ik.imagekit.io/shubhampathak/orca-web/fullplayer_lyrics.png?v=20261026)

### Artists

![Artist page with songs and albums](https://ik.imagekit.io/shubhampathak/orca-web/artist.png?v=20261026)

### Albums

![Album page with track details](https://ik.imagekit.io/shubhampathak/orca-web/album.png?v=20261026)

### Metadata editor

![Song metadata editor with artwork and lyrics controls](https://ik.imagekit.io/shubhampathak/orca-web/metadata_editor.png?v=20261026)


## Build

Install [Rust](https://rustup.rs/) and Visual Studio Build Tools with the
**Desktop development with C++** workload, then run from the repository root:

```powershell
cargo run --release --locked
```

See [Contributing](docs/CONTRIBUTING.md) for development setup, checks and packaging,
and [Architecture](docs/ARCHITECTURE.md) for the code structure.

## Contributing

Bug reports, documentation fixes and focused pull requests are welcome.
Discuss larger changes in an [issue](https://github.com/shubham-pathak1/orca/issues)
first to keep the scope focused on local music playback.

For bug reports, include your Orca version, steps to reproduce, and relevant
screenshots or logs. Please see the [contributor guide](docs/CONTRIBUTING.md) before submitting code.

Thank you for using Orca and helping make it better! :)

## Credits

- [Slint](https://slint.dev/) - native desktop interface.
- [Rodio](https://github.com/RustAudio/rodio), [Symphonia](https://github.com/pdeljanov/Symphonia) and [Lofty](https://github.com/Serial-ATA/lofty-rs) - audio playback, decoding and metadata.
- [LRC.RED](https://lrc.red/) and [LRCLIB](https://lrclib.net/) - lyrics; [Google](https://translate.google.com/) - romanization.
- [iTunes](https://www.apple.com/itunes/), [Deezer](https://www.deezer.com/), [MusicBrainz](https://musicbrainz.org/) and [Cover Art Archive](https://coverartarchive.org/) - artwork lookup.

## License

[MIT](LICENSE)
