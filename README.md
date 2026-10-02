# Orca

Orca is a local music player for Windows built using Svelte 5, Tauri 2, and Rust.

> [!IMPORTANT]
> **Alpha release:** Orca is in active development. Performance and stability on libraries larger than **5,000 tracks** have not been broadly tested yet. Please report bugs or regressions through GitHub Issues.

## Download

Get the Windows x64 installer or portable executable from [GitHub Releases](https://github.com/shubham-pathak1/orca/releases). The portable executable runs without an installer but still uses the normal user data locations.

---

## Key Features

- **Local library**: Scan local folders and keep them updated as files change. Supports `MP3`, `FLAC`, `M4A`, `WAV`, `OGG`, `OPUS`, and `AIFF` / `AIF`.
- **Playback**: Rodio-based audio playback with gapless playback, queue controls, shuffle, repeat, and waveform or standard seeking.
- **Waveforms**: Decode and cache waveform seekbars from the track audio.
- **Lyrics**: Prefer matching local `.lrc` files, then read embedded lyrics or fetch and cache timed or plain lyrics from LRCLIB. Enhanced LRC files with inline word timestamps display a white word-by-word sweep. Click a lyric line to seek, or import a local `.lrc` file through the metadata editor.
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
git clone https://github.com/shubham-pathak1/orca.git
cd orca
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

## Building a Release

Orca uses **NSIS** to bundle a Windows executable installer. MSI installers are disabled to keep packaging simple.

To build and collect the Windows x64 installer and portable executable:
```bash
bun run release:windows
```
The two release assets are placed in `release/v<version>/`. The script checks that
the package, Tauri, and Rust crate versions match before building. The portable
executable runs without an installer, but still uses the normal user data locations.

---

## Contributing & Support

Thank you for checking out Orca! If you would like to help improve the player:
* Feel free to report bugs or suggest features by opening a GitHub Issue.
* Pull requests are always welcome!

## License

MIT License. See [LICENSE](LICENSE) for more details.
