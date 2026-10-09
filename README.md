<p align="center">
  <img src="app/ui/assets/orca-logo.png" alt="Orca logo" width="88">
</p>

<h1 align="center">Orca</h1>

A local open-source music player.

[Download](https://github.com/shubham-pathak1/orca/releases) · [Report a bug](https://github.com/shubham-pathak1/orca/issues) · [Contribute](docs/CONTRIBUTING.md)

Orca is still in alpha and is currently available only for Windows x64.
Once it is stable enough, the plan is to bring it to Linux and possibly macOS too.

If you try Orca, please [report any bugs](https://github.com/shubham-pathak1/orca/issues)
or share your ideas. Your feedback and contributions help make it better. Thank you! :)

![Orca's main library showing album artwork, search and playback controls](https://ik.imagekit.io/shubhampathak/orca-web/library.png)

## Features

- Offline playback, search, alphabetical browsing and nested music folders
- Songs, artists, albums, genres and playlists, with list and grid views
- Gapless playback, shuffle, repeat, queue reordering and session restore
- Synced and word-timed lyrics, click-to-seek, LRC import and online fetching
- Waveform and classic seekbars, compact player and full-player lyrics
- Song tags and embedded cover editing, bulk album, artist and genre tag updates
- Online artwork lookup, custom collection covers and M3U playlist import/export
- Light and dark themes, custom fonts, minimal mode, dynamic accents and cover backgrounds
- Windows media controls, tray integration, keyboard shortcuts and Phantom Mode

## Screenshots

<details open>
<summary>Artists, albums, lyrics and metadata editing</summary>

### Artists

![Artist page with songs and albums from the local library](https://ik.imagekit.io/shubhampathak/orca-web/artist.png)

### Albums

![Album page with track details and playback controls](https://ik.imagekit.io/shubhampathak/orca-web/album.png)

### Lyrics

![Full player with album artwork and synced lyrics](https://ik.imagekit.io/shubhampathak/orca-web/full_player_lyrics.png)

### Metadata editing

![Song metadata editor with cover and lyrics controls](https://ik.imagekit.io/shubhampathak/orca-web/metadata_editor.png)

</details>

## Getting started

1. Download a build from [GitHub Releases](https://github.com/shubham-pathak1/orca/releases).
2. Run the Windows installer, or extract the portable ZIP and open **Orca.exe**.
3. Add your music folders in **Settings**. Orca includes music in their subfolders.

Online artwork fetching is experimental. Images may be unavailable or matched
incorrectly; you can choose or replace them manually.

Song edits write tags and covers to the audio file. Album, artist and genre
renames update the corresponding song tags. Collection covers and playlists
stay in Orca, preserving embedded song artwork.

### Updating

Close Orca before installing an update. For portable builds, extract the new ZIP
into a separate directory. Your existing library, playlists and settings are reused.
Back up `%LOCALAPPDATA%\OrcaSlintTauri` with Orca closed before upgrading.

## Development

Orca uses Rust and Slint. Install stable Rust and the Windows MSVC build tools,
then run from the repository root:

```powershell
cargo run --manifest-path app/Cargo.toml --target-dir target/slint
```

See [Contributing](docs/CONTRIBUTING.md) for setup, checks and packaging, and
[Architecture](docs/ARCHITECTURE.md) for the code structure.

## Contributing

Contributions of all sizes are welcome, from bug reports and documentation fixes
to code improvements. Since Orca is still in alpha, there may be rough edges;
please let us know what works well and what needs attention through
[GitHub Issues](https://github.com/shubham-pathak1/orca/issues).

When reporting a bug, please include your Orca version, steps to reproduce and
screenshots or logs when helpful. For code contributions, see the
[contributor guide](docs/CONTRIBUTING.md), keep your pull request focused on one
change and describe how you checked it.

Thank you for trying Orca and helping it grow!

## Credits

- [Slint](https://slint.dev/): native interface
- [Rodio](https://github.com/RustAudio/rodio) and [Lofty](https://github.com/Serial-ATA/lofty-rs): audio and metadata
- [LRCLIB](https://lrclib.net/): lyrics
- [iTunes](https://www.apple.com/itunes/), [Deezer](https://www.deezer.com/), [MusicBrainz](https://musicbrainz.org/) and [Cover Art Archive](https://coverartarchive.org/): artwork lookup

## License

Orca is available under the [MIT License](LICENSE).
