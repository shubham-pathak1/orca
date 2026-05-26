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

  async function centerActiveLyric() {
    await tick();

    if (!lyricsViewport) {
      return;
    }

    const activeLine = lyricsViewport.querySelector<HTMLElement>('[data-active="true"]');
    if (!activeLine) {
      return;
    }

    const targetTop = activeLine.offsetTop - lyricsViewport.clientHeight * 0.26 + activeLine.clientHeight / 2;
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
  <section class="absolute inset-0 z-30 overflow-hidden bg-black text-white">
    <div class="absolute inset-0 bg-cover bg-center opacity-55 blur-3xl [background-image:var(--cover-art)]"></div>
    <div class="absolute inset-0 bg-[linear-gradient(90deg,rgba(0,0,0,0.9)_0%,rgba(0,0,0,0.62)_48%,rgba(0,0,0,0.28)_100%)]"></div>

    <div class="relative flex h-full min-h-0 flex-col px-14 py-10 max-lg:px-6">
      <header class="flex h-16 shrink-0 items-center justify-between gap-4">
        <div class="flex min-w-0 items-center gap-3">
          <button class="back-button" title="Back" on:click={onClose}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path d="m15 6-6 6 6 6" />
            </svg>
          </button>

          {#if lyricsOpen}
            <button class="now-playing-pill" title="Show player" on:click={() => (lyricsOpen = false)}>
              <span class="h-12 w-12 shrink-0 overflow-hidden rounded-full bg-white/10">
                {#if song && artworkUrl(song.artwork)}
                  <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
                {/if}
              </span>
              <span class="min-w-0 text-left">
                <span class="block truncate text-base font-black text-[var(--accent)]">{song?.title ?? 'Select a song'}</span>
                <span class="block truncate text-sm text-white/58">{song?.artist ?? 'No track playing'}</span>
              </span>
              <span class="visualizer" aria-hidden="true">
                <span></span>
                <span></span>
                <span></span>
                <span></span>
              </span>
            </button>
          {/if}
        </div>

        <button
          class={`inline-flex h-10 items-center gap-2 rounded-full border px-4 text-sm font-black transition ${lyricsOpen ? 'border-white bg-white text-black' : 'border-white/12 bg-white/[0.06] text-white hover:bg-white/[0.1]'}`}
          title={lyricsOpen ? 'Hide lyrics' : 'Show lyrics'}
          on:click={() => (lyricsOpen = !lyricsOpen)}
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
            <path d="M8 5h11" />
            <path d="M8 12h11" />
            <path d="M8 19h7" />
            <path d="M4 5h.01" />
            <path d="M4 12h.01" />
            <path d="M4 19h.01" />
          </svg>
          Lyrics
        </button>
      </header>

      {#if lyricsOpen}
        <div class="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)_auto] gap-5 pt-2">
          <div class="min-h-0">
            {#if lyricLines.length}
              <div bind:this={lyricsViewport} class="lyrics-stack lyrics-open">
                {#each lyricLines as line}
                  <div
                    data-active={line.index === activeLyricIndex ? 'true' : undefined}
                    class:lyric-active={line.index === activeLyricIndex}
                    class:lyric-muted={line.index !== activeLyricIndex}
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

          <div class="mx-auto w-full max-w-2xl pb-0">
            <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
            <div class="mt-5">
              <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
            </div>
          </div>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col items-center justify-center pb-10 text-center">
          <div class="aspect-square w-full max-w-[min(420px,48vh)] shrink-0 overflow-hidden rounded-lg bg-white/10 shadow-[0_28px_100px_rgba(0,0,0,0.42)]">
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

          <div class="mt-8 w-full max-w-[min(560px,62vw)]">
            <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
          </div>

          <div class="mt-5 shrink-0">
            <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
          </div>
        </div>
      {/if}
    </div>
  </section>
{/if}

<style>
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

  .now-playing-pill {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    width: fit-content;
    min-width: min(250px, 44vw);
    max-width: min(350px, calc(100vw - 12rem));
    gap: 0.65rem;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.58);
    padding: 0.38rem 0.55rem 0.38rem 0.38rem;
    color: white;
    box-shadow:
      0 18px 60px rgba(0, 0, 0, 0.34),
      inset 0 0 0 1px rgba(255, 255, 255, 0.02);
  }

  .visualizer {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    width: 1.9rem;
    height: 1.9rem;
    flex-shrink: 0;
    border-radius: 999px;
    color: var(--accent);
  }

  .visualizer span {
    width: 3px;
    height: 0.7rem;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.95;
    animation: visualizer-pulse 900ms ease-in-out infinite;
  }

  .visualizer span:nth-child(2) {
    animation-delay: 110ms;
  }

  .visualizer span:nth-child(3) {
    animation-delay: 220ms;
  }

  .visualizer span:nth-child(4) {
    animation-delay: 330ms;
  }

  @keyframes visualizer-pulse {
    0%,
    100% {
      transform: scaleY(0.45);
    }

    50% {
      transform: scaleY(1.45);
    }
  }

  .lyrics-stack {
    box-sizing: border-box;
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-block: clamp(7rem, 18vh, 10rem);
    scroll-behavior: smooth;
    scrollbar-width: none;
  }

  .lyrics-stack::-webkit-scrollbar {
    display: none;
  }

  .lyrics-open {
    margin-inline: auto;
    max-width: min(900px, 62vw);
    transform: translateX(clamp(4rem, 7vw, 8rem));
  }

  .no-lyrics-state {
    display: flex;
    height: 100%;
    flex-direction: column;
    justify-content: center;
    max-width: min(900px, 62vw);
    margin-inline: auto;
    transform: translate(clamp(4rem, 7vw, 8rem), -7vh);
  }

  .lyric-line {
    margin-bottom: 1rem;
    max-width: min(1040px, 100%);
    cursor: pointer;
    color: rgba(255, 255, 255, 0.2);
    font-size: clamp(2rem, 3vw, 3.5rem);
    font-weight: 900;
    line-height: 1.08;
    letter-spacing: 0;
    outline: none;
    user-select: text;
    -webkit-text-fill-color: currentColor;
    background: none;
    background-clip: border-box;
    transition:
      color 220ms ease,
      filter 220ms ease,
      opacity 220ms ease;
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
    text-shadow: none;
  }

  .lyric-muted {
    color: rgba(255, 255, 255, 0.16);
    opacity: 1;
  }

  .lyric-muted:first-child,
  .lyric-muted:last-child {
    opacity: 1;
    filter: none;
  }

  @media (max-width: 1024px) {
    .now-playing-pill {
      min-width: 0;
      max-width: calc(100vw - 8rem);
    }

    .lyrics-open {
      max-width: 100%;
      transform: none;
    }

    .no-lyrics-state {
      max-width: 100%;
      transform: translateY(-5vh);
    }

    .lyric-line {
      font-size: clamp(1.75rem, 7vw, 3rem);
    }
  }
</style>
