# Contributing to Orca

Thanks for wanting to improve Orca.

## Development

1. Fork the repository and create a branch from `main`.
2. Install dependencies with `bun install`.
3. Run the app with `bun run tauri:dev`.
4. Before opening a pull request, run:

```bash
bun run check
cargo check --manifest-path src-tauri/Cargo.toml
```

## Notes

- Keep UI changes consistent with the existing Svelte component structure.
- Put shared player/library behavior in small components or `src/lib` helpers when it keeps the app easier to reason about.
- Prefer Rust for expensive local work such as scanning, metadata parsing, persistence, and audio processing.
- Include screenshots for visible UI changes.
