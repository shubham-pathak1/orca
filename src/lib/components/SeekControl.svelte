<script lang="ts">
  import { tick } from 'svelte';
  import { formatDuration } from '../format';
  import { waveformPeaks } from '../tauri';
  import type { LocalSong, PlaybackState } from '../types';

  export let song: LocalSong | null = null;
  export let playback: PlaybackState;
  export let variant: 'standard' | 'waveform' = 'standard';
  export let waveformLayout: 'inline' | 'stacked' = 'inline';
  export let waveformHeight = 28;
  export let onSeek: (event: Event) => void = () => {};

  let peaks: number[] = [];
  let loadedPath: string | null = null;
  let isLoadingWaveform = false;
  let canvas: HTMLCanvasElement | null = null;
  let drawFrame = 0;
  const WAVEFORM_CACHE_MAX = 96;
  const waveformCache = new Map<string, number[]>();

  function readWaveformCache(path: string): number[] | null {
    const cached = waveformCache.get(path);
    if (!cached) {
      return null;
    }

    // LRU refresh.
    waveformCache.delete(path);
    waveformCache.set(path, cached);
    return cached;
  }

  function writeWaveformCache(path: string, values: number[]) {
    if (waveformCache.has(path)) {
      waveformCache.delete(path);
    }

    waveformCache.set(path, values);
    if (waveformCache.size <= WAVEFORM_CACHE_MAX) {
      return;
    }

    const oldest = waveformCache.keys().next().value;
    if (oldest) {
      waveformCache.delete(oldest);
    }
  }

  $: progress = playback.duration_ms > 0 ? Math.min(Math.max(playback.position_ms / playback.duration_ms, 0), 1) : 0;
  $: if (variant === 'waveform' && canvas) {
    progress;
    peaks;
    scheduleDraw();
  }
  $: if (variant === 'waveform' && song?.path && song.path !== loadedPath && !isLoadingWaveform) {
    void loadPeaks(song.path);
  }

  async function loadPeaks(path: string) {
    loadedPath = path;
    peaks = fallbackPeaks(path);
    isLoadingWaveform = true;

    const cached = readWaveformCache(path);
    if (cached) {
      peaks = cached;
      isLoadingWaveform = false;
      return;
    }

    writeWaveformCache(path, peaks);

    try {
      const nextPeaks = await waveformPeaks(path, 720);
      if (nextPeaks.some((peak) => peak > 0)) {
        writeWaveformCache(path, nextPeaks);
        peaks = nextPeaks;
        scheduleDraw();
      }
    } catch {
      writeWaveformCache(path, peaks);
    } finally {
      isLoadingWaveform = false;
    }
  }

  function scheduleDraw() {
    cancelAnimationFrame(drawFrame);
    drawFrame = requestAnimationFrame(() => void drawWaveform());
  }

  function fallbackPeaks(seed: string): number[] {
    let hash = 2166136261;
    for (let index = 0; index < seed.length; index += 1) {
      hash ^= seed.charCodeAt(index);
      hash = Math.imul(hash, 16777619);
    }

    let value = hash >>> 0;
    return Array.from({ length: 640 }, (_, index) => {
      value = (Math.imul(value, 1664525) + 1013904223) >>> 0;
      const noise = value / 0xffffffff;
      value = (Math.imul(value, 1664525) + 1013904223) >>> 0;
      const detail = value / 0xffffffff;
      const contour = 0.35 + 0.2 * Math.sin(index * 0.014 + (hash % 29)) + 0.18 * Math.sin(index * 0.049 + (hash % 17));
      const transient = detail > 0.86 ? 0.35 : 0;
      return Math.min(1, Math.max(0.06, contour + noise * 0.26 + transient));
    });
  }

  async function drawWaveform() {
    await tick();
    if (!canvas) {
      return;
    }

    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.floor(rect.width * ratio));
    const height = Math.max(1, Math.floor(rect.height * ratio));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const ctx = canvas.getContext('2d');
    if (!ctx) {
      return;
    }

    ctx.clearRect(0, 0, width, height);
    const styles = getComputedStyle(canvas);
    const accent = styles.getPropertyValue('--accent').trim() || 'rgba(255,255,255,0.96)';
    const rest = styles.getPropertyValue(waveformLayout === 'stacked' ? '--waveform-rest-stacked' : '--waveform-rest').trim();
    drawShape(ctx, width, height, rest || (waveformLayout === 'stacked' ? 'rgba(255,255,255,0.13)' : 'rgba(255,255,255,0.14)'));
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, width * progress, height);
    ctx.clip();
    drawShape(ctx, width, height, accent);
    ctx.restore();
  }

  function drawShape(ctx: CanvasRenderingContext2D, width: number, height: number, fillStyle: string) {
    const values = peaks;
    if (!values.length) {
      return;
    }

    const centerY = height / 2;
    const step = width / Math.max(1, values.length - 1);

    ctx.fillStyle = fillStyle;
    ctx.beginPath();
    ctx.moveTo(0, centerY);

    for (let index = 0; index < values.length; index += 1) {
      const peak = Math.pow(values[index], 0.55);
      const barHeight = Math.max(2.5, peak * height * 0.98);
      ctx.lineTo(index * step, centerY - barHeight / 2);
    }

    for (let index = values.length - 1; index >= 0; index -= 1) {
      const peak = Math.pow(values[index], 0.55);
      const barHeight = Math.max(2.5, peak * height * 0.98);
      ctx.lineTo(index * step, centerY + barHeight / 2);
    }

    ctx.closePath();
    ctx.fill();
  }

</script>

<div class="w-full">
  {#if variant === 'waveform'}
    <label class={`group grid min-w-0 ${waveformLayout === 'stacked' ? 'gap-2' : 'grid-cols-[42px_minmax(0,1fr)_42px] items-center gap-3'}`}>
      {#if waveformLayout === 'inline'}
        <span class="text-right text-xs font-medium text-white/52">{formatDuration(playback.position_ms)}</span>
      {/if}
      <span
        class={`relative block min-w-0 overflow-hidden ${isLoadingWaveform ? 'opacity-70' : ''}`}
        style={`height: ${waveformHeight}px;`}
      >
        <canvas bind:this={canvas} class="absolute inset-0 h-full w-full" aria-hidden="true"></canvas>
        <input
          class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
          type="range"
          min="0"
          max={playback.duration_ms || 0}
          value={playback.position_ms}
          on:input={onSeek}
        />
      </span>
      {#if waveformLayout === 'inline'}
        <span class="text-xs font-medium text-white/52">{formatDuration(playback.duration_ms || song?.duration || 0)}</span>
      {:else}
        <span class="flex justify-between text-xs font-medium text-white/60">
          <span>{formatDuration(playback.position_ms)}</span>
          <span>{formatDuration(playback.duration_ms || song?.duration || 0)}</span>
        </span>
      {/if}
    </label>
  {:else}
    <label class="group grid min-w-0 grid-cols-[42px_minmax(0,1fr)_42px] items-center gap-3">
      <span class="text-right text-xs font-medium text-white/60">{formatDuration(playback.position_ms)}</span>
      <span class="standard-seek-track">
        <span class="standard-seek-progress" style={`width: ${progress * 100}%;`}></span>
        <span class="standard-seek-thumb" style={`left: ${progress * 100}%;`}></span>
        <input
          class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
          type="range"
          min="0"
          max={playback.duration_ms || 0}
          value={playback.position_ms}
          on:input={onSeek}
        />
      </span>
      <span class="text-xs font-medium text-white/60">{formatDuration(playback.duration_ms || song?.duration || 0)}</span>
    </label>
  {/if}
</div>

<style>
  .standard-seek-track {
    position: relative;
    display: block;
    height: 18px;
    min-width: 0;
  }

  .standard-seek-track::before {
    position: absolute;
    inset: 8px 0;
    border-radius: 999px;
    background: var(--waveform-rest, rgba(255, 255, 255, 0.16));
    content: '';
  }

  .standard-seek-progress {
    position: absolute;
    inset-block: 8px;
    left: 0;
    border-radius: 999px;
    background: var(--accent);
  }

  .standard-seek-thumb {
    position: absolute;
    top: 50%;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    background: var(--accent);
    box-shadow: 0 0 0 3px rgba(0, 0, 0, 0.18);
    transform: translate(-50%, -50%) scale(0.86);
    transition:
      box-shadow 140ms ease,
      transform 140ms ease;
  }

  .standard-seek-track:hover .standard-seek-thumb,
  .standard-seek-track:focus-within .standard-seek-thumb {
    box-shadow: 0 0 0 5px rgba(0, 0, 0, 0.2);
    transform: translate(-50%, -50%) scale(1);
  }
</style>
