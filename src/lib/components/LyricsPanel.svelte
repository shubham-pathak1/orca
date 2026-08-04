<script lang="ts">
  import { tick } from 'svelte';
  import { estimateActiveLyricIndex, findActiveLyricIndex, lyricSeekPosition, parseLyrics, type LyricLine } from '../lyrics';
  import type { LocalSong, PlaybackState } from '../types';

  export let open = false;
  export let song: LocalSong | null = null;
  export let playback: PlaybackState;
  export let onSeekTo: (positionMs: number) => Promise<void> | void = () => {};
  export let fetchedLyrics = '';
  export let fetchedLyricsSongPath: string | null = null;
  export let lyricsStatus: 'idle' | 'loading' | 'not-found' | 'offline' | 'error' = 'idle';

  let lyricsViewport: HTMLDivElement | null = null;
  let centeredLyricIndex = -1;
  let centeredSongPath: string | null = null;
  let lastOpenSongPath: string | null = null;

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
    centeredLyricIndex = -1;
    centeredSongPath = null;
  }
  $: if (open && lyricsViewport && activeLyricIndex >= 0 && (activeLyricIndex !== centeredLyricIndex || song?.path !== centeredSongPath)) {
    centeredLyricIndex = activeLyricIndex;
    centeredSongPath = song?.path ?? null;
    void centerActiveLyric();
  }
  $: if (open && song && !song.lyrics && song.path !== fetchedLyricsSongPath && lyricsStatus !== 'loading') {
    void fetchLyrics(song);
  }

  function seekToLyric(line: LyricLine) {
    const positionMs = lyricSeekPosition(line, lyricLines.length, playback.duration_ms);
    if (positionMs !== null) {
      void onSeekTo(positionMs);
    }
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

<style>
  .lyrics-viewport-shell,
  .lyrics-stack,
  .lyrics-open {
    pointer-events: none;
  }

  .lyrics-viewport-shell {
    align-self: stretch;
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
    pointer-events: auto;
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

  @media (max-width: 1024px) {
    .lyrics-open {
      width: calc(100vw - 3rem);
      margin-inline: auto;
      transform: translateY(clamp(-12px, -1.6vh, -4px));
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
