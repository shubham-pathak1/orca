# Contributing to Orca

Orca is a local music player built with Rust and Slint for Windows.

Orca is still in alpha. Bug reports, fixes, feature ideas and documentation
improvements are all welcome. Thanks for helping!

## Setup

Install stable Rust through rustup and Visual Studio Build Tools with the
**Desktop development with C++** workload and a Windows SDK.

From the repository root:

```powershell
cargo run --locked
```

The root Cargo workspace contains the app and both backend libraries. It shares
one `Cargo.lock`, dependency versions and build profiles. Plain `cargo run` or
`cargo build` selects the app; use `--workspace` for checks covering all packages.
Use `-p orca-core` or `-p orca-services` to select a backend package. Build output
stays in `target/slint`, configured by `.cargo/config.toml`.

The first build can take several minutes. Software rendering is the default.
To try GPU rendering, add `--features gpu` before `--`, and `--gpu` after it.

For an isolated test profile:

```powershell
cargo run --locked -- --data-dir "C:\path\to\test-profile"
```

Use disposable audio files when testing metadata or embedded-cover edits,
since these operations write to the files.

## Checks

Run these before opening a pull request:

```powershell
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets --no-deps -- -D warnings
cargo test --workspace --locked -- --test-threads=1
```

Add tests for observable behavior where useful, and manually check affected UI
and playback flows. Some integration tests require audio hardware or external
fixtures and are ignored by default.

Audio signal checks use generated disposable WAVs and need no output device:

```powershell
cargo test --locked -p orca-core audio_engine::signal_tests -- --nocapture --test-threads=1
```

These exercise the production source, playback queue, mixer, PCM buffer and
endpoint conversion. Matching-rate 16/24-bit unity playback must preserve the
samples exactly. Resampling checks tone amplitude through 20 kHz, suppression
of a 30 kHz alias during 192-to-48 kHz conversion, short-file duration, seeking
and queued format changes. They also cover volume/mute, PCM clipping and stereo
alignment. The 32-bit integer test documents the bounded precision loss of the
f32 pipeline instead of asserting bit-perfect output. These checks do not measure
the physical DAC, Bluetooth transmission or speakers; the separate ignored
`wasapi_output` test checks actual Windows device lifecycle behaviour.

Unprocessed integer checks use generated WAVs containing full-scale values,
nonzero low bits and deterministic random samples:

```powershell
cargo test --locked -p orca-core integer -- --test-threads=1
```

They verify 16/24/32-bit decoding, frame-accurate seeking, queued source replacement,
bounded i32 handoff, endpoint byte packing and rejection of precision/rate/channel
changes. The ignored hardware test uses silent disposable files to check mode
switching, queue transitions, pause/seek/resume and explicit refusal when the
device cannot accept 32 valid bits. Close other players before running it.

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
- Reuse existing UI components and theme values. Check narrow layouts as well as desktop layouts, including fractional display scales.
- Follow surrounding Rust and Slint styles. Keep comments concise and use them to explain decisions that the code cannot make clear.
- Name modules after their responsibility. App-local persistence and channels belong in `app/src/infrastructure`; application workflows belong in `orca-services`.
- Use function names that expose side effects at boundaries, such as `drain_events` and `apply_operation_result`. Short names such as `new` and local `install` remain appropriate when their type or module supplies the context.
- Use named fields for messages carrying several values of the same type; keep query generations distinct from timestamps.
- Document ownership, cancellation, invariants and surprising decisions. Remove comments that repeat the next statement or describe a previous implementation.
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

## FLAC compatibility backport

The workspace patches Symphonia's FLAC decoder from `vendor/symphonia-bundle-flac`
so normal Rodio playback and unprocessed integer playback use the same fix.
Its source remains MPL-2.0; preserve the bundled license and attribution.
The patch notes identify the upstream draft and local adaptations.

Generated stereo FLAC tests cover 16/24/32-bit samples, full-scale correlated
channels, frame-header sample-size codes, shared decoding and accurate seeking:

```powershell
cargo test --locked -p orca-core --test flac_precision
cargo test --locked -p symphonia-bundle-flac
```

## Rendering and input changes

Keep pixel correctness separate from frame timing. Compare partial repainting
against an independent full-rendered window, including late artwork completion
and transparent image edges. Use generated artwork rather than a personal library.
For precise scrolling, verify accumulated distance, direction changes and long
sequences that mix fractional and whole-number deltas.

A faster isolated image operation does not establish lower whole-app CPU usage.
Report what was measured and keep native scrolling checks separate from offscreen
rendering tests. Diagnostic traces and temporary probes belong under ignored
`target/slint/profiling`.

The local software-renderer override is documented in
[its patch notes](../vendor/i-slint-renderer-software/ORCA_PATCH.md). Library grids
use ordinary partial repainting; the renderer's optional retained-scroll path is
not enabled by the app.
