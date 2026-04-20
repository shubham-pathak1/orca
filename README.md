# Orca 🐋

Orca is a desktop music player for local files, built with Rust and the Slint UI framework. It is focused on high-quality audio playback and a clean layout.

![Orca Logo](public/public/orca_logo.png)

## Features

- **Library Management**: Scan and index local music folders.
- **Lyrics Support**: Displays synced (.lrc) and plain text lyrics.
- **High-Resolution Metadata**: View sample rates, bitrates, and bit depths for your audio files.
- **Search**: Navigate your library with a global search shortcut (Ctrl+K).
- **Interface**: A modern, simple UI with support for customization like monochrome mode.
- **Controls**: Basic playback controls alongside shuffle, repeat, and volume management.

## Keyboard Shortcuts

Orca provides several global and application-level shortcuts for ease of use:

- **Search**: `Ctrl + K` (Focus search bar)
- **Playback**: `Alt + N` (Next track), `Alt + P` (Previous track), `Space` (Play/Pause)
- **Visuals**: `F11` (Toggle Fullscreen), `Ctrl + Shift + M` (Toggle Monochrome mode)
- **System**: `Ctrl + Shift + B` (Show/Hide window globally)

## Supported Formats

Orca supports the following audio formats:

- **Lossless**: FLAC, WAV, ALAC (m4a)
- **Compressed**: MP3, AAC (m4a)

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/)
- C++ compiler (for dependencies)

### Usage

1. Clone the repository:
   ```bash
   git clone https://github.com/shubham-pathak1/orca.git
   cd orca
   ```

2. Run the app:
   ```bash
   cargo run --release
   ```

## Contributing

For information on contributing to this project, please see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
