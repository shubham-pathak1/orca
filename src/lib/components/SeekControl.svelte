<script context="module" lang="ts">
  const WAVEFORM_CACHE_MAX = 96;
  const waveformCache = new Map<string, number[]>();
  const pendingDecodes = new Map<string, Promise<number[]>>();

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
</script>

<script lang="ts">
  import { tick, onDestroy } from 'svelte';
  import { formatDuration } from '../format';
  import { waveformPeaks } from '../tauri';
  import type { LocalSong, PlaybackState } from '../types';

  export let song: LocalSong | null = null;
  export let playback: PlaybackState;
  export let variant: 'standard' | 'waveform' = 'standard';
  export let waveformLayout: 'inline' | 'stacked' = 'inline';
  export let waveformHeight = 28;
  export let onSeek: (event: Event) => Promise<void> | void = () => {};

  let peaks: number[] = [];
  let loadedPath: string | null = null;
  let isLoadingWaveform = false;
  let canvas: HTMLCanvasElement | null = null;
  let drawFrame = 0;

  let isDragging = false;
  let dragPositionMs = 0;

  // Grace timer and optimistic position to prevent the seekbar from jumping back 
  // to the old position before the backend updates.
  let seekGraceTimer: any = null;
  let optimisticPosition: number | null = null;

  // Extrapolate position locally at 60fps when playing
  let smoothPosition = 0;
  let lastUpdateTime = 0;
  let animationFrame = 0;

  function updateSmoothPosition() {
    if (!playback.is_playing || isDragging || optimisticPosition !== null) {
      cancelAnimationFrame(animationFrame);
      return;
    }

    const now = performance.now();
    const elapsed = now - lastUpdateTime;
    smoothPosition = Math.min(playback.position_ms + elapsed, playback.duration_ms || 0);
    animationFrame = requestAnimationFrame(updateSmoothPosition);
  }

  $: if (playback) {
    if (!isDragging && optimisticPosition === null) {
      smoothPosition = playback.position_ms;
      lastUpdateTime = performance.now();
      if (playback.is_playing) {
        cancelAnimationFrame(animationFrame);
        animationFrame = requestAnimationFrame(updateSmoothPosition);
      } else {
        cancelAnimationFrame(animationFrame);
      }
    }
  }

  onDestroy(() => {
    cancelAnimationFrame(animationFrame);
    if (seekGraceTimer) {
      clearTimeout(seekGraceTimer);
    }
  });

  $: displayPosition = isDragging 
    ? dragPositionMs 
    : (optimisticPosition !== null ? optimisticPosition : smoothPosition);

  $: progress = playback.duration_ms > 0 ? Math.min(Math.max(displayPosition / playback.duration_ms, 0), 1) : 0;

  // Toggle between total duration and remaining time
  let showRemaining = localStorage.getItem('seekbar-show-remaining') === 'true';
  function toggleRemaining() {
    showRemaining = !showRemaining;
    localStorage.setItem('seekbar-show-remaining', String(showRemaining));
  }

  $: totalMs = playback.duration_ms || song?.duration || 0;
  $: remainingMs = Math.max(0, totalMs - displayPosition);
  $: rightLabel = showRemaining ? `-${formatDuration(remainingMs)}` : formatDuration(totalMs);

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

    if (path === '__placeholder__') {
      isLoadingWaveform = false;
      return;
    }

    isLoadingWaveform = true;

    const cached = readWaveformCache(path);
    if (cached) {
      peaks = cached;
      isLoadingWaveform = false;
      return;
    }

    let decodePromise = pendingDecodes.get(path);
    if (!decodePromise) {
      decodePromise = (async () => {
        try {
          const nextPeaks = await waveformPeaks(path, 720);
          if (nextPeaks.some((peak) => peak > 0)) {
            writeWaveformCache(path, nextPeaks);
            return nextPeaks;
          }
        } catch (error) {
          console.error('Waveform decode error for', path, error);
        } finally {
          pendingDecodes.delete(path);
        }
        return fallbackPeaks(path);
      })();
      pendingDecodes.set(path, decodePromise);
    }

    try {
      const nextPeaks = await decodePromise;
      if (loadedPath === path) {
        peaks = nextPeaks;
        scheduleDraw();
      }
    } catch {
      // Keep fallback
    } finally {
      if (loadedPath === path) {
        isLoadingWaveform = false;
      }
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

  function handleInput(event: Event) {
    isDragging = true;
    const target = event.currentTarget as HTMLInputElement;
    dragPositionMs = Number(target.value);
  }

  async function handleChange(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    const seekVal = Number(target.value);
    optimisticPosition = seekVal;

    if (seekGraceTimer) {
      clearTimeout(seekGraceTimer);
    }

    try {
      await onSeek(event);
      seekGraceTimer = setTimeout(() => {
        optimisticPosition = null;
        smoothPosition = playback.position_ms;
        lastUpdateTime = performance.now();
      }, 800);
    } catch {
      optimisticPosition = null;
    } finally {
      isDragging = false;
    }
  }

</script>

<div class="w-full">
  {#if variant === 'waveform'}
    <label class={`group grid min-w-0 ${waveformLayout === 'stacked' ? 'gap-2' : 'grid-cols-[42px_minmax(0,1fr)_42px] items-center gap-3'}`}>
      {#if waveformLayout === 'inline'}
        <span class="text-right text-xs font-medium text-white/52">{formatDuration(displayPosition)}</span>
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
          value={displayPosition}
          on:input={handleInput}
          on:change={handleChange}
        />
      </span>
      {#if waveformLayout === 'inline'}
        <button
          class="w-[42px] text-left text-xs font-medium text-white/52 transition hover:text-white/80 cursor-pointer"
          title={showRemaining ? 'Show total duration' : 'Show remaining time'}
          on:click={toggleRemaining}
        >{rightLabel}</button>
      {:else}
        <span class="flex justify-between text-xs font-medium text-white/60">
          <span>{formatDuration(displayPosition)}</span>
          <button
            class="transition hover:text-white/80 cursor-pointer"
            title={showRemaining ? 'Show total duration' : 'Show remaining time'}
            on:click={toggleRemaining}
          >{rightLabel}</button>
        </span>
      {/if}
    </label>
  {:else}
    <label class="group grid min-w-0 grid-cols-[42px_minmax(0,1fr)_42px] items-center gap-3">
      <span class="text-right text-xs font-medium text-white/60">{formatDuration(displayPosition)}</span>
      <span class="standard-seek-track">
        <span class="standard-seek-progress" style={`width: ${progress * 100}%;`}></span>
        <span class="standard-seek-thumb" style={`left: ${progress * 100}%;`}></span>
        <input
          class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
          type="range"
          min="0"
          max={playback.duration_ms || 0}
          value={displayPosition}
          on:input={handleInput}
          on:change={handleChange}
        />
      </span>
      <button
        class="w-[42px] text-right text-xs font-medium text-white/60 transition hover:text-white/80 cursor-pointer"
        title={showRemaining ? 'Show total duration' : 'Show remaining time'}
        on:click={toggleRemaining}
      >{rightLabel}</button>
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
