# Architecture

Orca separates presentation, application workflows and reusable music processing.
Dependencies flow `app -> orca-services -> orca-core`. The services and core crates
do not depend on Slint. UI controllers submit requests rather than accessing SQL
or the audio engine directly.

The Rust host in `app/` includes desktop-specific orchestration: worker dispatch,
queue navigation, UI persistence and platform integration. `orca-services` provides
the catalog and editing workflows those workers call. This is an in-process Rust
application; the thread boundary uses channels, not a network service or FFI.

## Native source layout

`app/` contains the runnable Slint application. `crates/orca-services/` coordinates
catalog, metadata, playlists and background operations; `crates/orca-core/` owns
scanning, SQLite, audio, file tags and provider clients. `scripts/` packages the
Windows release; `.github/` contains validation CI.

Within `app/src/`:

| Folder | Responsibility |
| --- | --- |
| `controllers` | User actions, editor drafts and preferences |
| `media` | Artwork, lyrics and metadata drafts |
| `playback` | Navigation, timing and playback flow |
| `platform` | Media sessions, tray, hotkeys and renderer selection |
| `service` | Jobs, dispatch, persistence, errors and file watching |
| `views` | Visible models and presentation helpers |
| `protocol` | Typed requests, results and actions |
| `worker` | Service thread and background processing |
| `runtime` | Result application, scheduling and layout |
| `tests` | Offscreen UI workflows |

`main.rs` selects the renderer/profile, creates services and visible models,
installs callbacks and timers, then owns shutdown. Folder `mod.rs` files define
module boundaries; root imports preserve the internal paths used by controllers.

### Service and core modules

| Location | Responsibility |
| --- | --- |
| `orca-services/src/bootstrap.rs` | Profile preparation, backend creation and legacy library import |
| `orca-services/src/catalog.rs`, `folders.rs` | Queries, paging, collection identity and folder boundaries |
| `orca-services/src/operation_types.rs`, `types/` | Owned request/result contracts and validation |
| `orca-services/src/operations/` | Route collection edits, artwork, media and playlist operations |
| `orca-services/src/media/` | Metadata recovery/writes, lyrics, waveform and playlist workflows |
| `orca-services/src/artwork.rs` | Build lookup identity from tags and recording evidence |
| `orca-services/src/scanning.rs`, `playback.rs` | Scan/audio lifecycle and playback commands/snapshots |
| `orca-core/src/db/` | SQLite schema, migrations and domain queries |
| `orca-core/src/scanner.rs`, `library/` | Discover files, read tags and apply verified tag edits |
| `orca-core/src/atomic_file.rs` | Staged replacement and file-version checks |
| `orca-core/src/audio_engine/`, `audio_output/` | Decoding, audio processing, playback and output devices |
| `orca-core/src/artwork_cache.rs` | Persist content-based artwork originals and smaller variants |
| `orca-core/src/online_artwork/`, `lyrics.rs` | Provider requests, matching, caching and cancellation |

Paths in this table are relative to `crates/`.

## Requests and state ownership

A typical browse request follows this path:

1. A Slint callback reaches `controllers/browse.rs`, which constructs a query.
2. `protocol::Request` carries that query and its generation to the app worker.
3. `worker/catalog.rs` calls the service catalog API, which reads through core DB helpers.
4. `protocol::Event` returns owned tracks/groups to the UI runtime.
5. The runtime accepts results for the current generation and updates visible models.

Metadata and collection editors use the same direction, with typed metadata or
operation requests and explicit success/failure events. The runtime applies
completion policy; services validate and perform the underlying operation.
Compatibility JSON entry points still exist in the service API, but native
controllers use typed requests. Strings remain where they represent paths,
search text and collection keys.

| State | Owner |
| --- | --- |
| Screen, focus, selection, editor drafts and displayed progress | Slint state and Rust controllers/runtime |
| Visible song/group models and decoded image buffers | `views/models/` and `media/artwork/` on the UI side |
| Current catalog query and playback navigation/history | App worker state and `playback/navigation.rs` |
| Library records, playlist membership and saved collection artwork | SQLite, accessed through services/core |
| Active audio stream and playback snapshot | Core audio worker, exposed by the service backend |
| Preferences, saved queue/session and position checkpoints | App persistence helpers |

The UI playback clock interpolates displayed position between snapshots. It does
not own the audio stream. Catalog refreshes must preserve queue plans and the
current playback context.

## Threads, scheduling and shutdown

Slint handles and models stay on the UI thread. Requests/results carry owned
Rust data. App workers create their backend connections rather than sharing a
UI-owned SQLite connection.

The main worker handles library/catalog/playback coordination. Separate workers
perform operations and waveform/lyrics work; scan and audio threads belong to
the backend lifecycle. Waveform/lyrics scheduling uses a replaceable pending
slot so rapid track changes retain the newest job. The operation queue is
bounded and reports busy/unavailable failures instead of silently losing requests.
The general request/event channels are ordinary channels, not bounded job queues.

The UI runtime drains events and media completions on a timer. Generation checks
reject old browse, analysis and editor results. Cancellation also prevents
obsolete jobs from committing changes where the workflow supports it; ignoring
an old UI result alone would not protect files or saved state.

On exit, UI timers are stopped and dropped before worker ownership is released.
Dropping `worker::Service` signals shutdown and joins its workers. Backend
shutdown cancels scanning, sends the audio shutdown command and joins scan/audio
threads. Artwork and lyric renderers also stop and join their own workers.

## State and safety

- Requests carry owned data. UI handles stay on the UI thread.
- Editor generations and collection identities reject stale completions.
- Background jobs and decoded caches are bounded; cancellation stops obsolete work.
- Shutdown signals and joins service, scan and audio workers.
- Failed automatic artwork searches do not refresh the library. Successful updates are batched; unchanged images stay cached.
- Metadata updates for the same playing file preserve loaded lyrics and playback state.
- Song edits write file tags. Collection names update matching tags across their songs; collection covers stay in Orca and preserve embedded song artwork.
- Collection writes preflight every file and use verified atomic replacement per file. Interrupted bulk edits report partial completion.
- Schema migrations are transactional. Profile saves use recoverable backups.

Manual artwork/lyrics fetching stages editor results until Save. Manual lyrics
fetching requests fresh results; automatic loading respects saved lyrics and
intentional removal. Ambiguous online artwork is rejected rather than guessed.

### Metadata commits

Song edits write the original audio file through a staged copy, file-version
checks and tag read-back verification. The index is reconciled with the file
that actually committed. Recovery records cover an interruption between the
file replacement and database update.

Album/artist/genre renames snapshot collection membership and preflight every
source before writing. Each file replacement is atomic; the entire collection
is not one filesystem transaction. A failure after earlier saves reports partial
completion. Matching album-artist credits follow an artist rename while unrelated
guest/compilation credits are preserved.

Collection artwork belongs to Orca's profile and preserves embedded song covers.
Playlist names are library data; folder names come from the filesystem. Header
and context-menu editors share controller/service paths so entry points obey
the same rules.

## Storage and library reconciliation

The selected profile contains `orca.db`, artwork assets and app state files.
Settings use `slint-settings.json`; playback context uses `slint-session.json`
with a separate position checkpoint. Recoverable backups and checkpoint identity
prevent a stale position from being applied to a different saved queue.

SQLite uses WAL, foreign keys and versioned transactional migrations. Unsupported
newer schemas are rejected. A failed migration rolls back rather than leaving a
partially upgraded library. Legacy import uses a SQLite snapshot, including
committed WAL data, and does not replace an existing destination library.
On startup, a retryable artwork migration copies referenced external originals,
thumbnails and previews into the native profile before committing new paths.
It includes playlist and collection covers, repairs earlier native imports,
and leaves the source database and images untouched. Missing files retain their
references for a later retry. Subsequent releases reuse the native profile.

Configured roots are distinct from indexed tracks. Scanning and watcher-driven
reconciliation account for nested/overlapping roots and unavailable sources.
An unavailable configured drive must remain distinguishable from a deleted
track. Removing a source changes library configuration/indexing and preserves
the user's music files.

## Artwork and lyrics rendering

Core artwork persistence uses content identity to share originals, thumbnails
and previews when songs have identical covers. The app's decoded cache is
separate: its keys include display geometry and visual treatment. It requests
visible images, protects visible buffers under memory pressure and bounds
speculative work. Geometry/DPI changes may require new decoded variants;
ordinary playback ticks do not justify clearing the cache.

Artist portrait lookup uses recording evidence to distinguish names. iTunes
recording matches identify an Apple Music artist page; only structured artist
data with the expected ID and name supplies portraits. Deezer and MusicBrainz
provide independent fallbacks. Verified MusicBrainz relations can supply
Wikidata images or linked Apple Music and Deezer artist IDs. Identifying an
artist without finding a portrait is reported separately from an ambiguous
identity; album images are never substituted for portraits.

Explicit artwork fetching bypasses lookup caches, while automatic fetching
reuses cached successes and conclusive misses. Transient failures remain
retryable. Each provider has a bounded time budget, and downloaded images have
size and decoding limits. Collection editors display search progress, reject
duplicate requests and ignore completions belonging to a different draft.

`media/artwork/mod.rs` owns caching and requests; `decode.rs` owns cropping,
rounding, collages and accent extraction. Completion updates affected rows;
failed automatic lookups do not cause a library refresh, and successful batches
produce a combined refresh.

Lyrics parsing is separate from rendering. `media/lyric_render/mod.rs` owns row
scheduling and cached bitmaps, `layout.rs` shapes text and word rectangles, and
`raster.rs` draws glyphs. Layout changes follow font/geometry changes; playback
advances highlights over cached glyphs. Online failures must remain independent
of local browsing and listening.

## UI components

`app/ui/app.slint` composes the window; `state.slint` defines presentation data and
callbacks. `controls.slint` shares artwork, focus and button behavior.
`library.slint` and `folders.slint` provide browsing; `players.slint` owns player,
lyrics and queue views; `dialogs.slint` owns editors and menus; `settings.slint`
owns preferences. Fonts, branding and defaults live in `assets/` and `icons/`.

Folder detail pages show songs and subfolders side by side in wide windows.
Narrow windows use Songs / Folders tabs with counts; the selected tab persists
through nested navigation. Folders without children show their songs directly.
Song and folder lists remain virtualized, and artwork completions update both.

Artwork caching and decoding live in `app/src/media/artwork/`. Lyric scheduling,
layout and rasterization live in `app/src/media/lyric_render/`. Service media
workflows and regression tests are grouped by domain under `orca-services/src/`.

See [contributor instructions](CONTRIBUTING.md) for build, checks and packaging.

## Making changes

Put shared visuals in Slint controls, interaction/draft behavior in controllers,
workflow validation in services, and reusable file/audio/provider behavior in
core. Extend the typed request/result contracts when adding an operation. Keep
generation, cancellation and completion behavior together when changing a job.

Core tests cover storage, matching, file safety and audio behavior. Service tests
exercise workflows with disposable libraries. App tests cover queue state,
worker completions, caching and offscreen UI interactions. Hardware/live-provider
tests are opt-in; ordinary unit tests do not establish clean-machine installer,
screen-reader or real-device compatibility.

The release script produces a Windows x64 portable ZIP and a per-user NSIS setup
executable. Setup uses the current user's Programs directory and registers a
Start-menu shortcut and uninstaller. Uninstall deletes only packaged app files;
music, profile data and unrelated files survive. Locked executables prevent
upgrade/removal. Build/test/package commands live in the contributor guide.
