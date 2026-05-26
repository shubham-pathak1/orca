# Orca

Orca is a local music player for Windows, built with Svelte, Tauri, and Rust.

It is designed around fast browsing, editable metadata, playlists, lyrics, and playback controls that stay out of the way. The app is still early, so some parts are changing quickly.

## Features

- Scan local music folders into a persistent library.
- Browse by library, artist, album, and playlist.
- Use list or grid views for the main library.
- Play local files with seek, volume, shuffle, repeat, and keyboard controls.
- Switch between a right-side player and a compact bottom player.
- Open a full player with cover ambience, waveform seeking, and optional lyrics.
- View synced LRC or plain lyrics, with click-to-seek on lyric lines.
- Edit title, artist, album, album artist, year, track/disc number, genre, cover art, and lyrics.
- Create and manage playlists.

## Tech Stack

- Svelte 5
- TypeScript
- Tailwind CSS
- Tauri 2
- Rust
- SQLite
- Rodio for playback
- Lofty for metadata and tag editing

## Repository Layout

```text
src/                 Svelte frontend
src/lib/components/  UI components
src-tauri/           Tauri app shell and commands
crates/orca-core/    Library scanning, metadata, database, lyrics, and audio logic
public/              Static assets
```

## Requirements

- Bun
- Rust stable
- Tauri 2 system dependencies for Windows

## Development

```bash
bun install
bun run tauri:dev
```

For a dev build with optimized Rust code:

```bash
bun run tauri dev --release
```

For a release bundle:

```bash
bun run tauri:build
```

## Checks

```bash
bun run check
cargo check --manifest-path src-tauri/Cargo.toml
```

## License

MIT. See [LICENSE](LICENSE).
