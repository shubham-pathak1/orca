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
  const waveformCache = new Map<string, number[]>();

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

    if (waveformCache.has(path)) {
      peaks = waveformCache.get(path) ?? [];
      isLoadingWaveform = false;
      return;
    }

    waveformCache.set(path, peaks);

    try {
      const nextPeaks = await waveformPeaks(path, 720);
      if (nextPeaks.some((peak) => peak > 0)) {
        waveformCache.set(path, nextPeaks);
        peaks = nextPeaks;
        scheduleDraw();
      }
    } catch {
      waveformCache.set(path, peaks);
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
    const accent = getComputedStyle(canvas).getPropertyValue('--accent').trim() || 'rgba(255,255,255,0.96)';
    drawShape(ctx, width, height, waveformLayout === 'stacked' ? 'rgba(255,255,255,0.13)' : 'rgba(255,255,255,0.14)');
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
    <input
      class="h-1 w-full"
      style={`accent-color: var(--accent)`}
      type="range"
      min="0"
      max={playback.duration_ms || 0}
      value={playback.position_ms}
      on:input={onSeek}
    />
    <div class="mt-2 flex justify-between text-xs text-white/48">
      <span>{formatDuration(playback.position_ms)}</span>
      <span>{formatDuration(playback.duration_ms || song?.duration || 0)}</span>
    </div>
  {/if}
</div>
