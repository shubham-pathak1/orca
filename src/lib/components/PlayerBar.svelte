<script lang="ts">
  import { artworkUrl } from '../tauri';
  import { formatQuality } from '../format';
  import type { LocalSong, PlaybackState } from '../types';
  import PlaybackControls from './PlaybackControls.svelte';
  import SeekControl from './SeekControl.svelte';

  export let nowPlaying: LocalSong | null = null;
  export let playback: PlaybackState;
  export let status = 'Ready';
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
  export let onOpenFullPlayer: () => void = () => {};
  export let alwaysVisible = false;
</script>

<footer class={`${alwaysVisible ? 'col-span-2' : 'col-span-3 max-xl:col-span-2'} grid grid-cols-[minmax(190px,280px)_1fr_72px] items-center gap-4 border-t border-white/8 bg-[#111315]/96 px-5 py-2 max-md:col-span-1 max-md:grid-cols-1 max-md:gap-2`}>
  <div class="flex min-w-0 items-center gap-3">
    <button
      class="h-11 w-11 shrink-0 overflow-hidden rounded-sm bg-white/10 text-left outline-none transition hover:scale-[1.03] focus-visible:ring-2 focus-visible:ring-white/30"
      title="Open full player"
      on:click={onOpenFullPlayer}
    >
      {#if nowPlaying && artworkUrl(nowPlaying.artwork)}
        <img class="h-full w-full object-cover" src={artworkUrl(nowPlaying.artwork) ?? ''} alt="" />
      {/if}
    </button>
    <div class="min-w-0">
      <p class="truncate text-sm font-bold">{nowPlaying?.title ?? 'No track playing'}</p>
      <p class="truncate text-xs text-white/42">{nowPlaying?.artist ?? status}</p>
      {#if nowPlaying && showQualityInfo}
        <p class="mt-0.5 truncate text-[10px] font-bold uppercase text-white/32">{formatQuality(nowPlaying.format, nowPlaying.sample_rate, nowPlaying.bitrate)}</p>
      {/if}
    </div>
  </div>

  {#if seekbarStyle === 'waveform'}
    <div class="grid min-w-0 grid-rows-[32px_40px] items-center justify-items-center gap-1">
      <div class="w-full max-w-[840px] self-end">
        <SeekControl song={nowPlaying} {playback} variant={seekbarStyle} onSeek={onSeek} />
      </div>
      <div class="self-start">
        <PlaybackControls compact {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
      </div>
    </div>
  {:else}
    <div class="grid min-w-0 grid-cols-[150px_minmax(0,1fr)] items-center gap-3">
      <PlaybackControls compact {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />
      <SeekControl song={nowPlaying} {playback} variant={seekbarStyle} onSeek={onSeek} />
    </div>
  {/if}

  <div class="group relative flex justify-end">
    <button class="grid h-10 w-10 place-items-center rounded-md border border-white/10 text-white/64 transition hover:bg-white/[0.08] hover:text-white focus:bg-white/[0.08] focus:text-white" type="button" aria-label="Volume">
      <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
        <path d="M11 5 6 9H3v6h3l5 4V5Z" />
        <path d="M15.5 8.5a5 5 0 0 1 0 7" />
        <path d="M18.4 5.6a9 9 0 0 1 0 12.8" />
      </svg>
    </button>
    <div class="pointer-events-none absolute bottom-full right-0 mb-2 grid h-36 w-12 translate-y-1 place-items-center rounded-md border border-white/10 bg-[#171719] py-3 opacity-0 shadow-[0_18px_60px_rgba(0,0,0,0.36)] transition group-hover:pointer-events-auto group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:translate-y-0 group-focus-within:opacity-100">
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
      />
    </div>
  </div>
</footer>
