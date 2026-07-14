<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatQuality } from '../format';
  import type { LocalSong, PlaybackState } from '../types';
  import PlaybackControls from './PlaybackControls.svelte';
  import SeekControl from './SeekControl.svelte';
  import LazyArtwork from './LazyArtwork.svelte';

  export let nowPlaying: LocalSong | null = null;
  export let playback: PlaybackState;
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
  export let showQualityInfo = true;
  export let shuffleEnabled = false;
  export let repeatMode: 'off' | 'all' | 'one' = 'off';
  export let onToggle: () => void = () => {};
  export let onPrevious: () => void = () => {};
  export let onNext: () => void = () => {};
  export let onToggleShuffle: () => void = () => {};
  export let onCycleRepeat: () => void = () => {};
  export let onSeek: (event: Event) => void = () => {};
  export let onVolume: (event: Event) => void = () => {};
  export let onToggleMute: () => void = () => {};
  export let onAdjustVolume: (amount: number) => void = () => {};
  export let onOpenFullPlayer: () => void = () => {};
  export let queueOpen = false;
  export let onToggleQueue: () => void = () => {};
  export let alwaysVisible = false;

  let volumeOpen = false;
  let volumeGroup: HTMLDivElement;
  let volumeHideTimer: ReturnType<typeof setTimeout> | undefined;

  function handleVolumeWheel(event: WheelEvent) {
    const change = event.deltaY < 0 ? 0.05 : -0.05;
    onAdjustVolume(change);
  }

  function showVolume() {
    if (volumeHideTimer) {
      clearTimeout(volumeHideTimer);
      volumeHideTimer = undefined;
    }
    volumeOpen = true;
  }

  function hideVolume() {
    volumeHideTimer = setTimeout(() => {
      volumeOpen = false;
      volumeHideTimer = undefined;
    }, 800);
  }

  function handleVolumeClick() {
    onToggleMute();
    showVolume();
  }

  function handleClickOutside(event: MouseEvent) {
    if (volumeGroup && !volumeGroup.contains(event.target as Node)) {
      volumeOpen = false;
      if (volumeHideTimer) {
        clearTimeout(volumeHideTimer);
        volumeHideTimer = undefined;
      }
    }
  }

  onMount(() => {
    window.addEventListener('click', handleClickOutside);
  });

  onDestroy(() => {
    window.removeEventListener('click', handleClickOutside);
    if (volumeHideTimer) {
      clearTimeout(volumeHideTimer);
    }
  });

  const PLACEHOLDER_DURATION_MS = 180_000; // 3 minutes
  const PLACEHOLDER_PATH = '__placeholder__';

  const placeholderSong: LocalSong = {
    id: null,
    title: 'No track playing',
    artist: 'Ready',
    album_artist: '',
    album: '',
    year: null,
    track_number: null,
    disc_number: null,
    genre: null,
    duration: PLACEHOLDER_DURATION_MS,
    artwork: null,
    artwork_preview: null,
    artwork_thumb: null,
    lyrics: null,
    sample_rate: null,
    bitrate: null,
    bit_depth: null,
    format: null,
    modified_at: null,
    file_size: null,
    path: PLACEHOLDER_PATH,
  };

  $: displaySong = nowPlaying ?? placeholderSong;
  $: displayPlayback = nowPlaying ? playback : { ...playback, duration_ms: PLACEHOLDER_DURATION_MS, position_ms: 0 };
  $: displayVariant = nowPlaying ? seekbarStyle : 'waveform';
</script>

<footer class={`${alwaysVisible ? 'col-start-2 col-span-1' : 'col-span-3 max-xl:col-start-2 max-xl:col-span-1'} max-md:col-start-1 max-md:col-span-1 grid grid-cols-[minmax(190px,280px)_1fr_116px] items-center gap-4 border-t border-white/10 bg-[#111315]/96 px-5 py-2 max-md:grid-cols-1 max-md:gap-2`}>
  <div class="flex min-w-0 items-center gap-3">
    <button
      class="h-11 w-11 shrink-0 overflow-hidden rounded-sm border border-transparent bg-white/10 text-left outline-none transition hover:border-[color:var(--accent)] focus-visible:border-[color:var(--accent)]"
      title="Open full player"
      on:click={onOpenFullPlayer}
    >
      {#if nowPlaying && artworkUrl(nowPlaying.artwork_thumb ?? nowPlaying.artwork_preview ?? nowPlaying.artwork)}
        <LazyArtwork
          rootClass="h-full w-full"
          imageClass="h-full w-full object-cover"
          path={nowPlaying.artwork_thumb ?? nowPlaying.artwork_preview ?? nowPlaying.artwork}
          alt=""
        />
      {/if}
    </button>
    <div class="min-w-0">
      <p class="truncate text-sm font-bold">{nowPlaying?.title ?? 'No track playing'}</p>
      <p class="truncate text-xs text-white/42">{nowPlaying?.artist ?? 'Ready'}</p>
      {#if nowPlaying && showQualityInfo}
        <p class="mt-0.5 truncate text-[10px] font-bold uppercase text-white/32">{formatQuality(nowPlaying.format, nowPlaying.sample_rate, nowPlaying.bitrate)}</p>
      {/if}
    </div>
  </div>

  <div class="grid min-w-0 grid-rows-[34px_36px] items-center justify-items-center gap-1">
    <div class="w-full max-w-[840px] self-end">
      <SeekControl song={displaySong} playback={displayPlayback} variant={displayVariant} onSeek={onSeek} waveformLayout={displayVariant === 'waveform' ? 'inline' : 'inline'} />
    </div>
    <div class="self-start">
      <PlaybackControls compact {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
    </div>
  </div>

  <div class="flex items-center justify-end gap-2">
    <button
      class={`grid h-10 w-10 place-items-center rounded-md border border-white/10 transition hover:bg-white/[0.08] hover:text-white focus:bg-white/[0.08] focus:text-white ${queueOpen ? 'bg-white/[0.08] text-white' : 'text-white/64'}`}
      type="button"
      aria-label={queueOpen ? 'Hide queue' : 'Show queue'}
      aria-pressed={queueOpen}
      on:click={onToggleQueue}
    >
      <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 6h12" />
        <path d="M8 12h12" />
        <path d="M8 18h12" />
        <path d="M4 6h.01" />
        <path d="M4 12h.01" />
        <path d="M4 18h.01" />
      </svg>
    </button>

    <div class="group relative" bind:this={volumeGroup} on:wheel|preventDefault|stopPropagation={handleVolumeWheel} on:mouseenter={showVolume} on:mouseleave={hideVolume}>
      <button class="grid h-8 w-8 place-items-center rounded-md text-white/64 transition hover:text-white" type="button" aria-label="Volume" on:click={handleVolumeClick}>
        {#if playback.volume === 0}
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M11 5 6 9H3v6h3l5 4V5Z" />
            <line x1="22" y1="9" x2="16" y2="15" />
            <line x1="16" y1="9" x2="22" y2="15" />
          </svg>
        {:else if playback.volume < 0.5}
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M11 5 6 9H3v6h3l5 4V5Z" />
            <path d="M15.5 8.5a5 5 0 0 1 0 7" />
          </svg>
        {:else}
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M11 5 6 9H3v6h3l5 4V5Z" />
            <path d="M15.5 8.5a5 5 0 0 1 0 7" />
            <path d="M18.4 5.6a9 9 0 0 1 0 12.8" />
          </svg>
        {/if}
      </button>
      {#if volumeOpen}
      <div class="pointer-events-auto absolute bottom-full right-0 mb-2 z-20 animate-in fade-in-0 zoom-in-95 duration-150">
        <div class="bg-[#171719]/90 backdrop-blur-md border border-white/8 rounded-xl shadow-[0_8px_30px_rgba(0,0,0,0.45)] p-3">
          <input
            class="h-28 w-3 [direction:rtl] [writing-mode:vertical-lr]"
            style={`accent-color: var(--accent)`}
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={playback.volume}
            on:input={onVolume}
            aria-label="Volume level"
            on:mouseenter={showVolume}
            on:mouseleave={hideVolume}
          />
        </div>
      </div>
      {/if}
    </div>
  </div>
</footer>