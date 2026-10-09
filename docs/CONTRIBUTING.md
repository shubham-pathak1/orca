# Contributing

Create a branch from `main`. Complete formatting, Clippy and tests before
submitting changes.

Keep UI behavior in `app`, application workflows in `crates/orca-services`,
and reusable music functionality in `crates/orca-core`. Use typed requests,
keep expensive work off the UI thread, and handle cancellation and shutdown.

Use standard Rust formatting and match existing Slint conventions. Test observable
behavior, failures and recovery with disposable profiles and files. Include
screenshots for visible changes and describe relevant manual checks.

Use Conventional Commit subjects. Pull requests should explain the problem,
resulting behavior and validation. Keep builds, databases, caches and secrets
out of commits.

## Build and run

Install stable Rust and Windows MSVC build tools. Run from the repository root:

```powershell
cargo run --manifest-path app/Cargo.toml --target-dir target/slint
```

Use `--data-dir "C:\path\to\test-profile"` for an isolated profile and `--no-audio`
when testing without an audio device. Software rendering is the default;
`--features gpu` compiles optional support selected with the app's `--gpu` flag.

## Checks

```powershell
foreach ($project in @('crates/orca-core', 'crates/orca-services', 'app')) {
    cargo fmt --manifest-path "$project/Cargo.toml" --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
    cargo clippy --manifest-path "$project/Cargo.toml" --target-dir target/slint --jobs 2 --locked --all-targets --no-deps -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --manifest-path "$project/Cargo.toml" --target-dir target/slint --jobs 2 --locked -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
}
```

Add `--offline` to Cargo commands when dependencies are cached. Ordinary tests
use disposable profiles and files. Set `ORCA_TEST_SCREENSHOTS=1` only when captures
are needed; images go under `target/slint/test-artifacts/ui/`.

Core integration tests are opt-in:

- `metadata_integrity.rs`: set `ORCA_METADATA_FIXTURES` to downloaded Lofty minimal audio fixtures, then run `cargo test --manifest-path crates/orca-core/Cargo.toml --test metadata_integrity -- --ignored`.
- `wasapi_output.rs`: set `ORCA_TEST_AUDIO` to a disposable WAV, then run `cargo test --manifest-path crates/orca-core/Cargo.toml --test wasapi_output -- --ignored`. Requires Windows audio hardware.

These tests are excluded from ordinary runs because they need external fixtures
or hardware. No test should edit a user's original music.

## Package

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-native-windows.ps1
```

Install NSIS 3 to build the installer. The script checks, builds and packages a
Windows x64 portable ZIP and NSIS setup executable under `release/`, with SHA-256
checksums. Use `-PortableOnly` when only the ZIP is needed.
Packaging includes the x64 Visual C++ runtime from the installed MSVC
redistributables; use `-RuntimeDirectory` to specify its CRT directory explicitly.
Use `-SkipValidation` only immediately after passing checks on the same source.
Verify fresh/upgrade startup, actual media controls, audio-device interruptions
and accessibility before publishing. Write release notes with the GitHub release.

Setup installs for the current user, adds a Start-menu shortcut and registers an
uninstaller. Close Orca, including its tray mode, before upgrading or removing it.
Uninstall preserves music, profile data and unrelated files in the app directory.
Verify install, upgrade, locked-executable refusal and uninstall on a disposable
Windows account or clean machine before publishing; compilation alone is insufficient.
