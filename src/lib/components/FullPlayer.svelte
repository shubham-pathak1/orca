<script lang="ts">
  import { artworkUrl } from '../tauri';
  import { formatQuality } from '../format';
  import type { LocalSong, PlaybackState } from '../types';
  import LyricsPanel from './LyricsPanel.svelte';
  import PlaybackControls from './PlaybackControls.svelte';
  import SeekControl from './SeekControl.svelte';
  import VolumePopover from './VolumePopover.svelte';

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
  export let blurredBackground = true;
  export let lyricsOpen = false;
  export let queueOpen = false;
  export let onToggleQueue: () => void = () => {};
  export let onVolume: (event: Event) => void = () => {};
  export let onToggleMute: () => void = () => {};
  export let onAdjustVolume: (amount: number) => void = () => {};
  export let onEditSong: () => void = () => {};

  let fetchedLyrics = '';
  let fetchedLyricsSongPath: string | null = null;
  let lyricsStatus: 'idle' | 'loading' | 'not-found' | 'offline' | 'error' = 'idle';
  let swipeStartX = 0;
  let swipeStartY = 0;
  let swipeStartTime = 0;
  let swipePointerId: number | null = null;
  let wheelSwipeAccum = 0;
  let wheelSwipeLastAt = 0;
  let wheelSwipeConsumed = false;
  let swipeActionLastAt = 0;




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
    {#if blurredBackground}
      <div class="full-player-artwork-glow absolute inset-0 bg-cover bg-center blur-3xl [background-image:var(--cover-art)]"></div>
    {/if}
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
        <div class="flex items-center gap-2">
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
          <button
            class:queue-toggle-button-active={queueOpen}
            class="queue-toggle-button"
            title={queueOpen ? 'Hide queue' : 'Show queue'}
            aria-pressed={queueOpen}
            on:click={onToggleQueue}
          >
            <svg class="queue-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M8 6h12" />
              <path d="M8 12h12" />
              <path d="M8 18h12" />
              <path d="M4 6h.01" />
              <path d="M4 12h.01" />
              <path d="M4 18h.01" />
            </svg>
          </button>
        </div>
      </header>

      {#if lyricsOpen}
        <div class="lyrics-player-layout grid min-h-0 flex-1 grid-cols-[minmax(280px,390px)_minmax(0,900px)] items-center justify-center gap-10 pt-2 max-lg:grid-cols-1 max-lg:items-stretch">
          <div class="lyrics-side-player flex min-h-0 flex-col items-center justify-center text-center max-lg:hidden">
            <button class={`full-player-cover aspect-square w-full max-w-[min(340px,38vh)] shrink-0 overflow-hidden rounded-lg ${song?.artwork ? 'bg-white/10' : ''}`} title="Show player" on:click={() => (lyricsOpen = false)} on:contextmenu|preventDefault={onEditSong}>
              {#if song && artworkUrl(song.artwork)}
                <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
              {:else}
                <img src="/cover.png" class="h-full w-full object-cover" alt="" />
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

            <div class="player-controls-row mt-5 flex shrink-0 items-center justify-center gap-2">
              <div class="w-10 shrink-0"></div>
              <PlaybackControls {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
              <div class="w-10 shrink-0"></div>
            </div>

            
          </div>

          <LyricsPanel
            {open}
            {song}
            {playback}
            {onSeekTo}
            bind:fetchedLyrics
            bind:fetchedLyricsSongPath
            bind:lyricsStatus
          />

          <div class="lyrics-controls mx-auto hidden w-full max-w-2xl pb-0 max-lg:block">
            <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={46} onSeek={onSeek} />
            <div class="player-controls-row mt-5 flex items-center justify-center gap-6">
              <div class="w-10 shrink-0"></div>
              <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
              <div class="w-10 shrink-0"></div>
            </div>

            
          </div>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col items-center justify-center pb-10 text-center">
          <div class={`full-player-cover aspect-square w-full max-w-[min(420px,48vh)] shrink-0 overflow-hidden rounded-lg ${song?.artwork ? 'bg-white/10' : ''}`} on:contextmenu|preventDefault={onEditSong}>
            {#if song && artworkUrl(song.artwork)}
              <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
            {:else}
              <img src="/cover.png" class="h-full w-full object-cover" alt="" />
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
            <div class="w-10 shrink-0"></div>
            <PlaybackControls large {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
            <div class="w-10 shrink-0"></div>
          </div>
      <!-- inline volume control removed; using fixed bottom-right control instead -->
        </div>
      {/if}
      <VolumePopover
        volume={playback.volume}
        {onVolume}
        {onToggleMute}
        {onAdjustVolume}
      />
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
    pointer-events: none;
  }

  .full-player-wash {
    background:
      radial-gradient(circle at 79% 48%, rgba(255, 255, 255, 0.08) 0%, transparent 16rem),
      radial-gradient(circle at 20% 48%, rgba(0, 0, 0, 0.18) 0%, transparent 24rem),
      linear-gradient(90deg, rgba(0, 0, 0, 0.94) 0%, rgba(0, 0, 0, 0.74) 36%, rgba(0, 0, 0, 0.46) 68%, rgba(0, 0, 0, 0.72) 100%),
      linear-gradient(180deg, rgba(0, 0, 0, 0.74) 0%, transparent 26%, transparent 68%, rgba(0, 0, 0, 0.82) 100%);
      pointer-events: none;
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



  .queue-toggle-button {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    place-items: center;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.035);
    color: rgba(255, 255, 255, 0.62);
    transition:
      background 160ms ease,
      border-color 160ms ease,
      color 160ms ease;
  }

  .queue-toggle-button:hover,
  .queue-toggle-button-active {
    border-color: rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .queue-toggle-icon {
    width: 1.15rem;
    height: 1.15rem;
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
    z-index: 40;
    pointer-events: auto;
  }

  .lyrics-toggle-button::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 0;
    border-radius: inherit;
    padding: 0;
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
    pointer-events: none;
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

  .lyrics-player-layout {
    pointer-events: none;
  }

  .lyrics-side-player,
  .player-controls-row,
  .lyrics-controls {
    pointer-events: auto;
  }

  .lyrics-player-layout {
    column-gap: clamp(2.25rem, 4vw, 4.5rem);
    row-gap: clamp(2.6rem, 5.5vh, 4.4rem);
    transform: translateY(clamp(-2.6rem, -4vh, -1.4rem));
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
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
  }

  .full-player-cover img {
    -webkit-user-drag: none;
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
    pointer-events: none;
  }

  @media (max-width: 1024px) {
    .lyrics-player-layout {
      row-gap: clamp(1.4rem, 3.6vh, 2.4rem);
      transform: none;
    }
  }
</style>
