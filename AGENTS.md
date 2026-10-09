# Repository Guidelines

Orca is Slint/Rust. UI and controllers live in `app`, application workflows
in `crates/orca-services`, and scanning, storage, audio, tags and providers in `crates/orca-core`.
Dependencies flow UI -> application -> core. Controllers must not access SQL or audio internals.

Use standard Rust formatting and match Slint conventions. Keep reusable behavior
in small components, use typed messages, and keep expensive work off the UI thread.
Service changes require cancellation, shutdown and data-boundary checks.
Test observable behavior with disposable profiles/files, never user music.

See `docs/CONTRIBUTING.md` for build, Cargo checks and packaging. Run formatting,
Clippy and tests before submission; manually verify affected UI/playback flows.

Use Conventional Commit subjects and branch from `main`. Describe behavior and
validation; include screenshots for visible changes. Keep builds, databases,
caches and secrets out of commits.
