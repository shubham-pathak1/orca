# Contributing to Orca

Orca is a local music player built with Rust and Slint. Windows is currently the
supported platform. Linux support is planned once the app is stable enough,
with macOS a possibility later.

Orca is still in alpha. Bug reports, fixes, feature ideas and documentation
improvements are all welcome. Thanks for helping!

## Setup

Install stable Rust through rustup and Visual Studio Build Tools with the
**Desktop development with C++** workload and a Windows SDK.

From the repository root:

```powershell
cargo run --locked --manifest-path app/Cargo.toml --target-dir target/slint
```

The first build can take several minutes. Software rendering is the default.
To try GPU rendering, add `--features gpu` before `--`, and `--gpu` after it.

For an isolated test profile:

```powershell
cargo run --locked --manifest-path app/Cargo.toml --target-dir target/slint -- --data-dir "C:\path\to\test-profile"
```

Use disposable audio files when testing metadata or embedded-cover edits,
since these operations write to the files.

## Checks

Run these before opening a pull request:

```powershell
foreach ($project in @('crates/orca-core', 'crates/orca-services', 'app')) {
    cargo fmt --manifest-path "$project/Cargo.toml" --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
    cargo clippy --manifest-path "$project/Cargo.toml" --target-dir target/slint --locked --all-targets --no-deps -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --manifest-path "$project/Cargo.toml" --target-dir target/slint --locked -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
}
```

Add tests for observable behavior where useful, and manually check affected UI
and playback flows. Some integration tests require audio hardware or external
fixtures and are ignored by default.

## Branches, commits and pull requests

Branch from `main` and keep changes focused. Use concise Conventional Commit
messages, such as `fix(lyrics): refresh lyrics after editing`.

Explain the problem, what changed and how you checked it. Link relevant issues
and include screenshots for UI changes. Keep each commit buildable.

For bug reports, include your Orca version, Windows version and steps to
reproduce in an [issue](https://github.com/shubham-pathak1/orca/issues).
Remove private information from logs and screenshots before sharing them.

## Code guidelines

- Keep UI and controllers in `app`, application workflows in `crates/orca-services`, and reusable music functionality in `crates/orca-core`.
- Keep scanning, file access and network requests off the UI thread. Handle cancellation and shutdown when adding background work.
- Reuse existing UI components and theme values. Check narrow layouts as well as desktop layouts.
- Follow surrounding Rust and Slint styles. Keep comments concise and use them to explain decisions that the code cannot make clear.
- Keep builds, local databases, caches and secrets out of commits.

See [Architecture](ARCHITECTURE.md) for the module structure and data flow.

## Windows packages

Install NSIS 3, then run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-native-windows.ps1
```

Packages and checksums are written to `release/`. Use `-PortableOnly` to skip
the installer. To rebuild just the single-file portable EXE, run
`scripts/build-portable-windows.ps1` with PowerShell.

## License

Orca uses the [MIT License](../LICENSE). Contributions are covered by the same license.
