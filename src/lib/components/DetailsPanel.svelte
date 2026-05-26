<script lang="ts">
  import { artworkUrl } from '../tauri';
  import { formatQuality } from '../format';
  import type { LocalSong, PlaybackState } from '../types';
  import PlaybackControls from './PlaybackControls.svelte';
  import SeekControl from './SeekControl.svelte';

  export let song: LocalSong | null = null;
  export let playback: PlaybackState;
  export let status = 'Ready';
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
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
</script>

<aside class="relative min-h-0 overflow-hidden bg-black/45 px-5 py-5 max-xl:hidden">
  <div class="pointer-events-none absolute inset-0 bg-cover bg-center opacity-18 blur-2xl [background-image:var(--cover-art)]"></div>
  <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-black/45 via-black/70 to-black/88"></div>
  <div class="relative flex h-full flex-col">
    {#if song}
      <button
        class="group mx-auto aspect-square w-full max-w-[272px] overflow-hidden rounded-md bg-white/8 text-left outline-none transition hover:scale-[1.01] focus-visible:ring-2 focus-visible:ring-white/32"
        title="Open full player"
        on:click={onOpenFullPlayer}
      >
        {#if artworkUrl(song.artwork)}
          <img class="h-full w-full object-cover transition group-hover:brightness-110" src={artworkUrl(song.artwork) ?? ''} alt="" />
        {/if}
      </button>
      <div class="mt-5 text-center">
        <h2 class="truncate text-2xl font-bold">{song.title}</h2>
        <p class="mt-1 truncate text-base text-white/68">{song.artist}</p>
        <p class="mt-4 inline-flex rounded-sm bg-white/10 px-2.5 py-1.5 text-[10px] font-bold uppercase text-white/66">
          {formatQuality(song.format, song.sample_rate, song.bitrate) || 'Local audio'}
        </p>
      </div>

      <div class="mt-10 space-y-5">
        <div>
          <SeekControl {song} {playback} variant={seekbarStyle} waveformLayout="stacked" waveformHeight={42} onSeek={onSeek} />
        </div>

        <PlaybackControls compact {shuffleEnabled} {repeatMode} isPlaying={playback.is_playing} onToggle={onToggle} onPrevious={onPrevious} onNext={onNext} {onToggleShuffle} {onCycleRepeat} />

        <div class="grid grid-cols-[32px_1fr] items-center gap-3">
          <svg class="h-4 w-4 text-white/42" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M11 5 6 9H2v6h4l5 4V5Z" />
            <path d="M15.5 8.5a5 5 0 0 1 0 7" />
          </svg>
          <input class="w-full" style={`accent-color: var(--accent)`} type="range" min="0" max="1" step="0.01" value={playback.volume} on:input={onVolume} />
        </div>

      </div>
    {:else}
      <div class="grid h-full place-items-center text-center text-sm text-white/40">{status}</div>
    {/if}
  </div>
</aside>
