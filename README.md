# Orca

Orca is a local music player for Windows built using Svelte 5, Tauri 2, and Rust.

> [!IMPORTANT]
> **Alpha release:** Orca is in active development. Performance and stability on libraries larger than **5,000 tracks** have not been broadly tested yet. Please report bugs or regressions through GitHub Issues.

---

## Key Features

- **Local library**: Scan local folders and keep them updated as files change. Supports `MP3`, `FLAC`, `M4A`, `WAV`, `OGG`, `OPUS`, and `AIFF` / `AIF`.
- **Playback**: Rodio-based audio playback with gapless playback, queue controls, shuffle, repeat, and waveform or standard seeking.
- **Waveforms**: Decode and cache waveform seekbars from the track audio.
- **Lyrics**: Prefer matching local `.lrc` files, then read embedded lyrics or fetch and cache timed or plain lyrics from LRCLIB. Click a lyric line to seek, or import a local `.lrc` file through the metadata editor.
- **Metadata**: Edit track tags and cover art directly in the app.
- **Playlists**: Create playlists, set custom covers, and import or export standard M3U playlists.
- **Windows integration**: Taskbar controls, global media shortcuts, and Windows media controls.
- **Player views**: Library, artists, albums, genres, playlists, queue, and a full-player lyrics view.

---

## Keyboard Shortcuts

- `Space`: Play or pause
- `Alt + N` / `Alt + P`: Next or previous song
- `M`: Toggle mute
- `L`: Show or hide full-player lyrics
- `F11`: Toggle full screen
- `Ctrl + Shift + B`: Enter or restore Phantom Mode while Orca is running

---

## Screenshots

**Library**: ![Library View](docs/screenshots/library.png)
**Albums**: ![Albums View](docs/screenshots/albums.png)
**Artists**: ![Artists View](docs/screenshots/artist.png)
**Full Player**: ![Full Player](docs/screenshots/fullplayer.png)
**Synced Lyrics**: ![Lyrics View](docs/screenshots/lyrics.png)
**Metadata Editor**: ![Metadata Editor](docs/screenshots/metadata_editor.png)

---

## Tech Stack

* **Frontend**: Svelte 5 (Vite), TypeScript, Tailwind CSS, HTML5 Canvas
* **Backend**: Rust, Tauri 2, SQLite (`rusqlite`)
* **Audio Engine**: Rodio
* **Tagging Library**: Lofty

---

## Repository Structure

```text
src/                 Svelte frontend codebase
src/lib/components/  UI components (Player, Waveform, Metadata, Queue)
src-tauri/           Tauri application backend and command handlers
crates/orca-core/    Core database structure, scanning engine, and audio thread logic
crates/orca-gpui/    Experimental native GPUI prototype
```

---

## Getting Started

### Prerequisites

You will need the following tools installed on your Windows machine:
1. [Rust](https://www.rust-lang.org/tools/install)
2. [Bun](https://bun.sh/)
3. [Tauri Windows Setup Requirements](https://v2.tauri.app/start/prerequisites/)

### Development

Clone the repository and install the dependencies:
```bash
bun install
```

Start the development server with live reload:
```bash
bun run tauri:dev
```

To run the desktop app with Rust release optimizations:
```bash
bun run tauri:dev -- --release
```

---

## Native GPUI Prototype

The main app remains the Svelte/Tauri version on the default branch. A separate, experimental native UI prototype lives on the [gpui-prototype branch](../../tree/gpui-prototype). It is not release-ready and may change quickly, but it is a place to explore a lower-overhead native renderer for Orca.

To try it on Windows:

```bash
cargo run --release --manifest-path crates/orca-gpui/Cargo.toml
```

Contributors interested in GPUI, native Windows rendering, input/accessibility, profiling, or cross-platform packaging are especially welcome. Please open an issue before taking on a larger change so the work can be coordinated.

---

## Building a Release

Orca uses **NSIS** to bundle a Windows executable installer. MSI installers are disabled to keep packaging simple.

To build the NSIS installer:
```bash
bun run tauri:build
```
The output `.exe` installer will be located in `src-tauri/target/release/bundle/nsis/`.

---

## Contributing & Support

Thank you for checking out Orca! If you would like to help improve the player:
* Feel free to report bugs or suggest features by opening a GitHub Issue.
* Pull requests are always welcome!

## License

MIT License. See [LICENSE](LICENSE) for more details.
