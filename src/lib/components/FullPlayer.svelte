<script lang="ts">
  import { tick } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatQuality } from '../format';
  import type { LocalSong, PlaybackState } from '../types';
  import PlaybackControls from './PlaybackControls.svelte';
  import SeekControl from './SeekControl.svelte';

  export let open = false;
  export let song: LocalSong | null = null;
  export let playback: PlaybackState;
  export let onClose: () => void = () => {};
  export let onToggle: () => void = () => {};
  export let onPrevious: () => void = () => {};
  export let onNext: () => void = () => {};
  export let shuffleEnabled = false;
  export let repeatMode: 'off' | 'all' | 'one' = 'off';
  export let onToggleShuffle: () => void = () => {};
  export let onCycleRepeat: () => void = () => {};
  export let onSeek: (event: Event) => void = () => {};
  export let onSeekTo: (positionMs: number) => Promise<void> | void = () => {};
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
  export let showQualityInfo = true;
  export let lyricsOpen = false;

  let lyricsViewport: HTMLDivElement | null = null;
  let centeredLyricIndex = -1;
  let centeredSongPath: string | null = null;
  let fetchedLyrics = '';
  let fetchedLyricsSongPath: string | null = null;
  let lyricsStatus: 'idle' | 'loading' | 'not-found' | 'offline' | 'error' = 'idle';
  let lastOpenSongPath: string | null = null;
  let swipeStartX = 0;
  let swipeStartY = 0;
  let swipeStartTime = 0;
  let swipePointerId: number | null = null;
  let wheelSwipeAccum = 0;
  let wheelSwipeLastAt = 0;
  let wheelSwipeConsumed = false;
  let swipeActionLastAt = 0;

  type LyricLine = {
    index: number;
    timeMs: number | null;
    text: string;
  };

  $: rawLyrics = song?.lyrics || (song?.path === fetchedLyricsSongPath ? fetchedLyrics : '');
  $: lyricLines = parseLyrics(rawLyrics);
  $: hasSyncedLyrics = lyricLines.some((line) => line.timeMs !== null);
  $: activeLyricIndex = lyricLines.length
    ? hasSyncedLyrics
      ? findActiveLyricIndex(lyricLines, playback.position_ms)
      : estimateActiveLyricIndex(lyricLines, playback.position_ms, playback.duration_ms)
    : -1;
  $: if (open && song?.path !== lastOpenSongPath) {
    lastOpenSongPath = song?.path ?? null;
    lyricsOpen = false;
    centeredLyricIndex = -1;
    centeredSongPath = null;
  }
  $: if (!open) {
    lyricsOpen = false;
  }
  $: if (open && lyricsOpen && lyricsViewport && activeLyricIndex >= 0 && (activeLyricIndex !== centeredLyricIndex || song?.path !== centeredSongPath)) {
    centeredLyricIndex = activeLyricIndex;
    centeredSongPath = song?.path ?? null;
    void centerActiveLyric();
  }
  $: if (open && lyricsOpen && song && !song.lyrics && song.path !== fetchedLyricsSongPath && lyricsStatus !== 'loading') {
    void fetchLyrics(song);
  }

  function parseLyrics(rawLyrics: string): LyricLine[] {
    const rawLines = rawLyrics.split(/\r?\n/);
    const syncedLines: LyricLine[] = [];
    const plainLines: LyricLine[] = [];

    for (const rawLine of rawLines) {
      const timestamps = Array.from(rawLine.matchAll(/\[(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?\]/g));
      const text = rawLine.replace(/\[[^\]]+\]/g, '').trim();
      if (!text) {
        continue;
      }

      if (timestamps.length > 0) {
        for (const timestamp of timestamps) {
          const minutes = Number(timestamp[1]);
          const seconds = Number(timestamp[2]);
          const fraction = timestamp[3] ?? '0';
          const millis = Number(fraction.padEnd(3, '0').slice(0, 3));
          syncedLines.push({
            index: syncedLines.length,
            timeMs: minutes * 60_000 + seconds * 1_000 + millis,
            text
          });
        }
      } else {
        plainLines.push({
          index: plainLines.length,
          timeMs: null,
          text
        });
      }
    }

    const lines = syncedLines.length > 0 ? syncedLines.sort((a, b) => (a.timeMs ?? 0) - (b.timeMs ?? 0)) : plainLines;
    return lines.map((line, index) => ({ ...line, index }));
  }

  function findActiveLyricIndex(lines: LyricLine[], positionMs: number): number {
    let activeIndex = 0;
    for (const line of lines) {
      if (line.timeMs !== null && line.timeMs <= positionMs) {
        activeIndex = line.index;
      }
    }
    return activeIndex;
  }

  function estimateActiveLyricIndex(lines: LyricLine[], positionMs: number, durationMs: number): number {
    if (!durationMs || durationMs <= 0) {
      return 0;
    }

    const progress = Math.min(Math.max(positionMs / durationMs, 0), 0.999);
    return Math.min(lines.length - 1, Math.floor(progress * lines.length));
  }

  function lyricSeekPosition(line: LyricLine) {
    if (line.timeMs !== null) {
      return line.timeMs;
    }

    if (!playback.duration_ms || lyricLines.length === 0) {
      return null;
    }

    const progress = lyricLines.length === 1 ? 0 : line.index / Math.max(1, lyricLines.length - 1);
    return Math.round(progress * playback.duration_ms);
  }

  function seekToLyric(line: LyricLine) {
    const positionMs = lyricSeekPosition(line);
    if (positionMs === null) {
      return;
    }

    void onSeekTo(positionMs);
  }

  function handleLyricKeydown(event: KeyboardEvent, line: LyricLine) {
    if (event.key !== 'Enter' && event.key !== ' ') {
      return;
    }

    event.preventDefault();
    seekToLyric(line);
  }

  function shouldIgnoreSwipeTarget(target: EventTarget | null) {
    if (!(target instanceof Element)) {
      return false;
    }

    return Boolean(target.closest('button,input,select,textarea,a,[data-no-swipe]'));
  }

  function triggerSwipeNavigation(direction: 'next' | 'previous') {
    const now = Date.now();
    if (now - swipeActionLastAt < 260) {
      return;
    }

    swipeActionLastAt = now;
    if (direction === 'next') {
      onNext();
      return;
    }

    onPrevious();
  }

  function startSwipeTracking(clientX: number, clientY: number) {
    swipeStartX = clientX;
    swipeStartY = clientY;
    swipeStartTime = Date.now();
  }

  function finishSwipeTracking(clientX: number, clientY: number) {
    if (swipeStartTime === 0) {
      return;
    }

    const deltaX = clientX - swipeStartX;
    const deltaY = clientY - swipeStartY;
    const elapsed = Date.now() - swipeStartTime;
    const absX = Math.abs(deltaX);
    const absY = Math.abs(deltaY);

    swipeStartTime = 0;
    swipePointerId = null;

    if (elapsed > 650) {
      return;
    }

    if (absX < 72 || absX < absY * 1.25) {
      return;
    }

    if (deltaX < 0) {
      triggerSwipeNavigation('next');
      return;
    }

    triggerSwipeNavigation('previous');
  }

  function handleFullPlayerPointerDown(event: PointerEvent) {
    if (!event.isPrimary) {
      return;
    }

    if (event.pointerType === 'mouse' && event.button !== 0) {
      return;
    }

    if (shouldIgnoreSwipeTarget(event.target)) {
      swipePointerId = null;
      swipeStartTime = 0;
      return;
    }

    swipePointerId = event.pointerId;
    startSwipeTracking(event.clientX, event.clientY);
  }

  function handleFullPlayerPointerUp(event: PointerEvent) {
    if (!event.isPrimary || swipePointerId === null || event.pointerId !== swipePointerId) {
      return;
    }

    finishSwipeTracking(event.clientX, event.clientY);
  }

  function handleFullPlayerPointerCancel(event: PointerEvent) {
    if (swipePointerId !== null && event.pointerId === swipePointerId) {
      swipePointerId = null;
      swipeStartTime = 0;
    }
  }

  function handleFullPlayerWheel(event: WheelEvent) {
    if (shouldIgnoreSwipeTarget(event.target)) {
      return;
    }

    const absX = Math.abs(event.deltaX);
    const absY = Math.abs(event.deltaY);
    if (absX < 6 || absX < absY * 1.2) {
      return;
    }

    event.preventDefault();
    const now = Date.now();
    if (now - wheelSwipeLastAt > 220) {
      wheelSwipeAccum = 0;
      wheelSwipeConsumed = false;
    }
    wheelSwipeLastAt = now;

    if (wheelSwipeConsumed) {
      return;
    }

    wheelSwipeAccum += event.deltaX;

    if (Math.abs(wheelSwipeAccum) < 84) {
      return;
    }

    if (wheelSwipeAccum > 0) {
      triggerSwipeNavigation('next');
    } else {
      triggerSwipeNavigation('previous');
    }
    wheelSwipeConsumed = true;
    wheelSwipeAccum = 0;
  }

  async function centerActiveLyric() {
    await tick();

    if (!lyricsViewport) {
      return;
    }

    const activeLine = lyricsViewport.querySelector<HTMLElement>('[data-active="true"]');
    if (!activeLine) {
      return;
    }

    const anchor = activeLyricIndex <= 1 ? 0.26 : 0.42;
    const targetTop = activeLine.offsetTop - lyricsViewport.clientHeight * anchor + activeLine.clientHeight / 2;
    const distance = Math.abs(lyricsViewport.scrollTop - targetTop);
    lyricsViewport.scrollTo({
      top: Math.max(0, targetTop),
      behavior: distance > lyricsViewport.clientHeight ? 'auto' : 'smooth'
    });
  }

  async function fetchLyrics(targetSong: LocalSong) {
    fetchedLyricsSongPath = targetSong.path;
    fetchedLyrics = '';
    lyricsStatus = 'loading';

    if (!navigator.onLine) {
      lyricsStatus = 'offline';
      return;
    }

    const params = new URLSearchParams({
      track_name: targetSong.title,
      artist_name: targetSong.artist,
      album_name: targetSong.album
    });

    if (targetSong.duration > 0) {
      params.set('duration', String(Math.round(targetSong.duration / 1000)));
    }

    try {
      const response = await fetch(`https://lrclib.net/api/get?${params.toString()}`, {
        headers: { Accept: 'application/json' }
      });

      if (response.status === 404) {
        lyricsStatus = 'not-found';
        return;
      }

      if (!response.ok) {
        lyricsStatus = 'error';
        return;
      }

      const data = await response.json();
      fetchedLyrics = data.syncedLyrics || data.plainLyrics || '';
      lyricsStatus = fetchedLyrics ? 'idle' : 'not-found';
    } catch {
      lyricsStatus = navigator.onLine ? 'error' : 'offline';
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <section
    class="full-player-surface absolute inset-0 z-30 overflow-hidden bg-black text-white"
    on:pointerdown={handleFullPlayerPointerDown}
    on:pointerup={handleFullPlayerPointerUp}
    on:pointercancel={handleFullPlayerPointerCancel}
    on:wheel={handleFullPlayerWheel}
  >
    <div class="full-player-artwork-glow absolute inset-0 bg-cover bg-center blur-3xl [background-image:var(--cover-art)]"></div>
    <div class="full-player-wash absolute inset-0"></div>
    <div class="full-player-spotlight absolute inset-0"></div>

    <div class="relative flex h-full min-h-0 flex-col px-14 py-10 max-lg:px-6">
      <header class="flex h-16 shrink-0 items-center justify-between gap-4">
        <div class="flex min-w-0 items-center gap-3">
          <button class="back-button" title="Back" on:click={onClose}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path d="m15 6-6 6 6 6" />
            </svg>
          </button>

        </div>
      </header>

      {#if lyricsOpen}
        <div class="lyrics-player-layout grid min-h-0 flex-1 grid-cols-[minmax(280px,390px)_minmax(0,900px)] items-center justify-center gap-10 pt-2 max-lg:grid-cols-1 max-lg:items-stretch">
          <div class="lyrics-side-player flex min-h-0 flex-col items-center justify-center text-center max-lg:hidden">
            <button class="full-player-cover aspect-square w-full max-w-[min(340px,38vh)] shrink-0 overflow-hidden rounded-lg bg-white/10" title="Show player" on:click={() => (lyricsOpen = false)}>
              {#if song && artworkUrl(song.artwork)}
                <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
              {/if}
            </button>

            <div class="mt-5 w-full max-w-[min(340px,38vh)]">
              <h2 class="truncate text-3xl font-black">{song?.title ?? 'Select a song'}</h2>
              <p class="mt-2 truncate text-base text-white/68">{song?.artist ?? 'No track playing'}</p>
              {#if song && showQualityInfo}
                <p class="mt-4 inline-flex rounded-sm bg-white/14 px-2.5 py-1.5 text-[10px] font-bold uppercase text-white/72">
                  {formatQuality(song.format, song.sample_rate, song.bitrate) || 'Local audio'}
                </p>
              {/if}
            </div>

            <div class="mt-8 w-full max-w-[min(420px,42vw)]">
              <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
            </div>

            <div class="player-controls-row mt-5 flex shrink-0 items-center justify-center gap-6">
              <div class="h-10 w-10"></div>
              <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
              <button
                class:lyrics-toggle-button-active={lyricsOpen}
                class="lyrics-toggle-button"
                title={lyricsOpen ? 'Hide lyrics' : 'Show lyrics'}
                aria-pressed={lyricsOpen}
                on:click={() => (lyricsOpen = !lyricsOpen)}
              >
                <svg class="lyrics-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <path d="M6.75 5.75h10.5A2.25 2.25 0 0 1 19.5 8v6.25a2.25 2.25 0 0 1-2.25 2.25h-6.9L5.5 19.25V8a2.25 2.25 0 0 1 2.25-2.25Z" />
                  <path d="M8.75 9.25h6.5" />
                  <path d="M8.75 12h5.35" />
                  <path d="M8.75 14.75h3.8" />
                </svg>
              </button>
            </div>
          </div>

          <div class="lyrics-viewport-shell min-h-0">
            {#if lyricLines.length}
              <div bind:this={lyricsViewport} class="lyrics-stack lyrics-open">
                {#each lyricLines as line}
                  <div
                    data-active={line.index === activeLyricIndex ? 'true' : undefined}
                    class:lyric-active={line.index === activeLyricIndex}
                    class:lyric-adjacent={activeLyricIndex >= 0 && Math.abs(line.index - activeLyricIndex) === 1}
                    class:lyric-muted={line.index !== activeLyricIndex && Math.abs(line.index - activeLyricIndex) !== 1}
                    class="lyric-line"
                    role="button"
                    tabindex="0"
                    title="Seek to lyric"
                    on:click={() => seekToLyric(line)}
                    on:keydown={(event) => handleLyricKeydown(event, line)}
                  >
                    {line.text}
                  </div>
                {/each}
              </div>
            {:else}
              <div class="no-lyrics-state">
                {#if lyricsStatus === 'loading'}
                  <p class="text-5xl font-black leading-tight text-white">Fetching lyrics from LRCLIB...</p>
                  <p class="mt-4 text-lg text-white/52">One sec.</p>
                {:else if lyricsStatus === 'offline'}
                  <p class="text-5xl font-black leading-tight text-white">Oops, no lyrics found :(</p>
                  <p class="mt-4 text-lg text-white/56">Please turn on your internet to fetch lyrics :)</p>
                {:else}
                  <p class="text-5xl font-black leading-tight text-white">Oops, no lyrics found :(</p>
                  <p class="mt-4 text-lg text-white/56">LRCLIB does not have lyrics for this track yet.</p>
                {/if}
              </div>
            {/if}
          </div>

          <div class="lyrics-controls mx-auto hidden w-full max-w-2xl pb-0 max-lg:block">
            <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
            <div class="player-controls-row mt-5 flex items-center justify-center gap-6">
              <div class="h-10 w-10"></div>
              <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
              <button
                class:lyrics-toggle-button-active={lyricsOpen}
                class="lyrics-toggle-button"
                title={lyricsOpen ? 'Hide lyrics' : 'Show lyrics'}
                aria-pressed={lyricsOpen}
                on:click={() => (lyricsOpen = !lyricsOpen)}
              >
                <svg class="lyrics-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <path d="M6.75 5.75h10.5A2.25 2.25 0 0 1 19.5 8v6.25a2.25 2.25 0 0 1-2.25 2.25h-6.9L5.5 19.25V8a2.25 2.25 0 0 1 2.25-2.25Z" />
                  <path d="M8.75 9.25h6.5" />
                  <path d="M8.75 12h5.35" />
                  <path d="M8.75 14.75h3.8" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col items-center justify-center pb-10 text-center">
          <div class="full-player-cover aspect-square w-full max-w-[min(420px,48vh)] shrink-0 overflow-hidden rounded-lg bg-white/10">
            {#if song && artworkUrl(song.artwork)}
              <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
            {/if}
          </div>

          <div class="mt-6 w-full max-w-[min(420px,48vh)]">
            <h2 class="truncate text-4xl font-black">{song?.title ?? 'Select a song'}</h2>
            <p class="mt-2 truncate text-lg text-white/68">{song?.artist ?? 'No track playing'}</p>
            {#if song && showQualityInfo}
              <p class="mt-4 inline-flex rounded-sm bg-white/14 px-2.5 py-1.5 text-[10px] font-bold uppercase text-white/72">
                {formatQuality(song.format, song.sample_rate, song.bitrate) || 'Local audio'}
              </p>
            {/if}
          </div>

          <div class="mt-8 w-full max-w-[min(560px,66vw)]">
            <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
          </div>

          <div class="player-controls-row mt-5 flex shrink-0 items-center justify-center gap-6">
            <div class="h-10 w-10"></div>
            <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
            <button
              class:lyrics-toggle-button-active={lyricsOpen}
              class="lyrics-toggle-button"
              title={lyricsOpen ? 'Hide lyrics' : 'Show lyrics'}
              aria-pressed={lyricsOpen}
              on:click={() => (lyricsOpen = !lyricsOpen)}
            >
              <svg class="lyrics-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M6.75 5.75h10.5A2.25 2.25 0 0 1 19.5 8v6.25a2.25 2.25 0 0 1-2.25 2.25h-6.9L5.5 19.25V8a2.25 2.25 0 0 1 2.25-2.25Z" />
                <path d="M8.75 9.25h6.5" />
                <path d="M8.75 12h5.35" />
                <path d="M8.75 14.75h3.8" />
              </svg>
            </button>
          </div>
        </div>
      {/if}
    </div>
  </section>
{/if}

<style>
  .full-player-surface {
    touch-action: pan-y;
  }

  .full-player-artwork-glow {
    opacity: 0.68;
    transform: scale(1.08);
    filter: blur(3.25rem) saturate(1.18) contrast(1.04);
  }

  .full-player-wash {
    background:
      radial-gradient(circle at 79% 48%, rgba(255, 255, 255, 0.08) 0%, transparent 16rem),
      radial-gradient(circle at 20% 48%, rgba(0, 0, 0, 0.18) 0%, transparent 24rem),
      linear-gradient(90deg, rgba(0, 0, 0, 0.94) 0%, rgba(0, 0, 0, 0.74) 36%, rgba(0, 0, 0, 0.46) 68%, rgba(0, 0, 0, 0.72) 100%),
      linear-gradient(180deg, rgba(0, 0, 0, 0.74) 0%, transparent 26%, transparent 68%, rgba(0, 0, 0, 0.82) 100%);
  }

  .full-player-spotlight {
    background: radial-gradient(ellipse at 73% 50%, rgba(0, 0, 0, 0) 0%, rgba(0, 0, 0, 0) 18rem, rgba(0, 0, 0, 0.18) 36rem, rgba(0, 0, 0, 0.42) 100%);
    pointer-events: none;
  }

  .back-button {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    flex-shrink: 0;
    place-items: center;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.28);
    color: rgba(255, 255, 255, 0.74);
    transition:
      background 160ms ease,
      color 160ms ease,
      border-color 160ms ease;
  }

  .back-button:hover {
    border-color: rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .player-controls-row {
    transform: translateX(-0.55rem);
  }

  .lyrics-toggle-button {
    position: relative;
    isolation: isolate;
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    place-items: center;
    overflow: hidden;
    border: 0;
    border-radius: 999px;
    background: linear-gradient(135deg, rgba(32, 34, 34, 0.96), rgba(3, 4, 4, 0.96));
    color: rgba(255, 255, 255, 0.94);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.08),
      0 0.8rem 1.8rem rgba(0, 0, 0, 0.26);
    transition:
      box-shadow 160ms ease,
      color 160ms ease,
      filter 160ms ease,
      transform 160ms ease;
  }

  .lyrics-toggle-button::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 0;
    border-radius: inherit;
    padding: 2px;
    background-image:
      var(--cover-art),
      linear-gradient(135deg, var(--accent-mid), var(--accent-soft));
    background-position: center;
    background-size: cover;
    -webkit-mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    -webkit-mask-composite: xor;
    mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    mask-composite: exclude;
  }

  .lyrics-toggle-icon {
    position: relative;
    z-index: 1;
    width: 1.12rem;
    height: 1.12rem;
  }

  .lyrics-toggle-button-active {
    color: white;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      0 0.9rem 2rem rgba(0, 0, 0, 0.3),
      0 0 0 1px rgba(255, 255, 255, 0.08);
  }

  .lyrics-stack {
    box-sizing: border-box;
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-top: clamp(5.75rem, 12vh, 8rem);
    padding-bottom: clamp(9rem, 18vh, 12rem);
    scroll-behavior: smooth;
    scrollbar-width: none;
    mask-image: linear-gradient(to bottom, transparent 0, black 3.5rem, black calc(100% - 7rem), transparent 100%);
  }

  .lyrics-stack::-webkit-scrollbar {
    display: none;
  }

  .lyrics-open {
    width: min(880px, 100%);
    margin-inline: auto;
    transform: translateX(clamp(-1.8rem, -2.2vw, -0.75rem));
  }

  .lyrics-player-layout {
    column-gap: clamp(2.25rem, 4vw, 4.5rem);
    row-gap: clamp(2.6rem, 5.5vh, 4.4rem);
    transform: translateY(clamp(-2.6rem, -4vh, -1.4rem));
  }

  .lyrics-viewport-shell {
    align-self: stretch;
  }

  .lyrics-controls {
    position: relative;
    z-index: 1;
  }

  .lyrics-side-player {
    justify-self: center;
    width: 100%;
    transform: translate(clamp(0.75rem, 1.4vw, 1.35rem), -1.2vh);
  }

  .full-player-cover {
    border-radius: 1rem;
    box-shadow:
      0 2rem 5rem rgba(0, 0, 0, 0.52),
      0 0 0 1px rgba(255, 255, 255, 0.11);
  }

  .no-lyrics-state {
    display: flex;
    height: 100%;
    flex-direction: column;
    justify-content: center;
    width: min(980px, calc(100vw - 8rem));
    margin-inline: auto;
    text-align: center;
    transform: translateY(-4.8vh);
  }

  .lyric-line {
    margin-bottom: 0.86rem;
    max-width: 100%;
    cursor: pointer;
    color: rgba(255, 255, 255, 0.18);
    font-size: clamp(1.9rem, 2.65vw, 3.15rem);
    font-weight: 900;
    line-height: 1.06;
    letter-spacing: -0.035em;
    text-align: center;
    outline: none;
    user-select: text;
    -webkit-text-fill-color: currentColor;
    background: none;
    background-clip: border-box;
    transition:
      color 220ms ease,
      filter 220ms ease,
      opacity 220ms ease,
      transform 220ms ease;
  }

  .lyric-line:hover {
    color: rgba(255, 255, 255, 0.52);
    transform: scale(1.01);
  }

  .lyric-line:focus-visible {
    color: rgba(255, 255, 255, 0.86);
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-decoration-thickness: 0.08em;
    text-underline-offset: 0.16em;
  }

  .lyric-active {
    color: rgb(255, 255, 255);
    opacity: 1;
    text-shadow:
      0 0.08em 0.4em rgba(0, 0, 0, 0.42),
      0 0 1.3em rgba(255, 255, 255, 0.13);
  }

  .lyric-adjacent {
    color: rgba(255, 255, 255, 0.34);
  }

  .lyric-muted {
    color: rgba(255, 255, 255, 0.18);
    opacity: 1;
  }

  .lyric-muted:first-child,
  .lyric-muted:last-child {
    opacity: 1;
    filter: none;
  }

  @media (max-width: 1024px) {
    .lyrics-open {
      width: calc(100vw - 3rem);
      margin-inline: auto;
      transform: translateY(clamp(-12px, -1.6vh, -4px));
    }

    .lyrics-player-layout {
      row-gap: clamp(1.4rem, 3.6vh, 2.4rem);
      transform: none;
    }

    .no-lyrics-state {
      width: calc(100vw - 3rem);
      margin-inline: auto;
      transform: translateY(-3.4vh);
    }

    .lyric-line {
      font-size: clamp(1.75rem, 7vw, 3rem);
    }
  }
</style>
