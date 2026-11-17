# Architecture

Orca is a Windows local music player built with Rust and Slint. Dependencies flow
`app -> orca-services -> orca-core`. Controllers do not access SQL or audio internals.

The three packages share one root Cargo workspace, lockfile and build profiles.
The About version and executable resources derive from the package version.
Shared dependency versions and the FLAC decoder patch are defined in the root
`Cargo.toml`; `.cargo/config.toml` keeps build output in `target/slint`.

## Responsibilities

| Location | Responsibility |
| --- | --- |
| `app/ui` | Slint components and the properties/callbacks exposed to Rust |
| `app/src/controllers` | Bind user actions, edit drafts and validate UI input |
| `app/src/views/models` | Catalog models, paging and visible artwork references |
| `app/src/playback` | UI playback state, interpolation and navigation policies |
| `app/src/runtime` | Apply worker results and schedule visible resources |
| `app/src/worker` | Coordinate requests, background jobs and backend lifetime |
| `app/src/infrastructure` | App-side persistence, job channels, error presentation and file watching |
| `app/src/media` | Artwork decoding, lyric layout/rasterization and metadata drafts |
| `app/src/platform` | Windows media controls, tray, shortcuts and renderer selection |
| `crates/orca-services` | Catalog, editing, scanning and playback workflows |
| `crates/orca-core` | SQLite, audio, tags, file replacement and provider clients |

`main.rs` creates the profile, worker and models, binds controllers and starts the
`UiRuntime`. `infrastructure` contains app-local mechanisms such as persistence,
channels and watching; it is separate from the workflows in `orca-services`.
`worker` owns background execution, while `runtime` applies results on the UI
thread. Domain callbacks belong in their controllers.

## State ownership

Slint bindings are split by domain: `AppState` holds shell/focus/window state;
`LibraryState`, `PlayerState`, `LyricsState`, `PreferencesState` and `DialogState`
hold their displayed values and callbacks. Typed window accessors make the owning
domain explicit in controllers. These globals own no database or audio stream.

- `CatalogModels` owns song/group models and detail artwork. The browse
  controller owns debounce and return-scroll state.
- `PlayerModels` owns the current track, loaded lyrics, seek preview, display
  clock, lyric renderer, queue and neighboring tracks. Browsing does not replace it.
- `AppWorker` owns background threads and request/result channels. Its catalog
  state is separate from playback navigation, preload plans and playback errors.
- The service backend owns database access, scan state and audio lifecycle.
- Core audio workers own decoder/output state. The UI clock only interpolates
  displayed lyric progress between backend snapshots.

Preferences and playback sessions are persisted through app-side helpers. The
Windows default profile is `%LOCALAPPDATA%\Orca`; bootstrap migrates legacy
profiles before starting the backend.

## Requests and results

A user action follows `Slint -> controller -> typed Request -> AppWorker ->
service backend -> core`. Owned results return as typed `Event` values to the
UI runtime. Slint handles stay on the UI thread. The event router checks query
and operation identity; typed catalog, playback and lyric presenters update their
own models. Shared lyric installation preserves editor drafts and refresh fallbacks.

Catalog results carry query generations. Artwork, lyrics and editor operations
also check current identity/revision before applying results. Late work must not
replace the current song, query or draft. Compatibility JSON service entry points
remain available; app controllers use typed requests. Shared Rust data is imported
from `orca_services::types`; it is not an FFI boundary. Lyric-render completions
use named fields for pixel geometry, timing generation and owned image buffers.

The UI runtime has explicit entry points: `event_dispatch::drain_events`,
`operation_results::apply_operation_result`, `artwork_updates::refresh_artwork`
and `artwork_scheduler::schedule_artwork`. Each owns one direction of the data
flow rather than a general-purpose update function.

## Scheduling and resource use

- UI result processing handles at most 64 events per tick and stops between
  events after 4 ms. Remaining events stay queued in order.
- Page coalescing examines at most 64 requests at a time and retains the two
  newest pages. A following non-page command preempts those page reads.
- Visible runtime updates run every 33 ms; hidden-window maintenance runs at
  most once per 200 ms. Returning to a visible window resumes normal updates.
- Waveform, lyric fetching and romanization use replaceable pending jobs.
  Operation queues are bounded; busy/unavailable results are explicit.
- Artwork decoding and renderer-buffer preparation use one sleeping worker.
  The runtime batches completions rather than waking layout for every image.
  Cache retention, queued results and prefetch are bounded. Cached covers stay
  immediate. New browse decoding waits until scrolling has been still for 90 ms;
  playback artwork stays eligible during scrolling. Requests for departed rows
  are cancelled, and nearby prefetch waits behind visible work.
  Visible models retain image handles. Circle masks are prepared on the worker;
  opaque interiors are preserved without recalculating edge coverage.
- Grids, album shelves and manual lyrics scrolling use Slint's native Flickable.
  Its physics handle the offset and size adjustments made by virtualized views.
  The full player preserves the covered library's docked-panel width and song
  anchor. Returning restores that anchor after the column layout is ready.
  Lyrics pause auto-follow on manual movement and resume after five idle seconds
  or a line click. Partially visible glyph images fade at viewport edges using
  the existing cached pixels; scrolling does not rasterize the text again.
  Catalog rows use physical-pixel heights and a shared aligned paint origin.
  On Windows, fractional wheel events identify a precise input sequence. Those
  deltas are applied directly instead of adding wheel inertia; whole-number
  deltas in that continuous sequence keep the same behavior until it idles.
  This is an event-precision heuristic, not a reliable device identifier.
  No competing property animation is applied to scrolling offsets.
- Lyric shaping/rasterization runs on a worker. Sharp and blurred variants share
  an 8 MiB cache; visible rows retain their images until they leave view or their
  layout changes. Pronunciation sweep timing is projected once during layout.

Software frame pacing keeps a refresh-based deadline, rebasing after substantial
lateness rather than issuing short catch-up intervals. Browser artwork uses
normal partial repainting; custom cover-pixel reuse is not enabled in the grid.

Worker shutdown signals cancellation and joins owned threads. UI timers are
stopped and dropped before worker ownership is released.

## Audio and file safety

Normal playback uses processed floating-point audio. Windows output supports
shared and exclusive routing; exclusive mode alone does not imply unprocessed
output. The unprocessed integer path accepts supported integer WAV, AIFF and
FLAC, requires a compatible device format and bypasses volume, EQ and resampling.
Unsupported unprocessed combinations fail explicitly.

Decoding feeds bounded PCM queues outside the endpoint callback. Source changes
invalidate queued PCM. Paused producers park; output initialization and worker
handoff have bounded waits. Device preferences remain distinct from actual
output status, including fallback notices and errors.

Tag/cover edits use staged file replacement, file-version checks and read-back
verification. Recovery records cover interruption between file replacement and
index reconciliation. Collection edits preflight sources and report partial
completion; a collection is not one filesystem transaction.

## Validation

See [Contributing](CONTRIBUTING.md) for formatting, Clippy, tests and packaging.
Tests use disposable profiles and generated files. Keep checks focused on
observable behavior, audio correctness, data safety and regressions. Hardware
and provider checks are opt-in; automated tests do not establish physical DAC
behavior or long-term reliability on every device.

### Full-player gradient

The optional Gradient background combines four cover-derived colors: RGB555
median-cut palette extraction, HSL color separation and tuning, radial
smootherstep falloff, and a vertical readability scrim. The artwork worker bakes
the settled composition into one opaque bitmap, bounded to 512 pixels on its
long edge and charged to the existing artwork cache. It uses the small cover
source. No per-frame blur or drifting animation runs on the UI thread.

Gradient is a full-player setting. Library grids remain plain; collection-detail
headers retain their blurred cover background when browse artwork is enabled.
