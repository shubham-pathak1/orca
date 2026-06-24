<script lang="ts">
  import { formatDuration } from '../format';
  import type { LocalSong } from '../types';
  import LazyArtwork from './LazyArtwork.svelte';

  export let open = false;
  export let songs: LocalSong[] = [];
  export let currentPath: string | null = null;
  export let shuffleEnabled = false;
  export let repeatMode: 'off' | 'all' | 'one' = 'off';
  export let onClose: () => void = () => {};
  export let onChooseSong: (song: LocalSong) => Promise<void> | void = () => {};
  export let onReorder: (sourcePath: string, targetPath: string) => void = () => {};
  export let onClear: () => void = () => {};
  export let onRemoveSong: (path: string) => void = () => {};

  const maxVisibleSongs = 20;
  let draggedPath: string | null = null;
  let dragOverPath: string | null = null;
  let pointerMovedQueueItem = false;

  $: visibleSongs = songs.slice(0, maxVisibleSongs);
  $: hiddenSongCount = Math.max(0, songs.length - visibleSongs.length);
  $: currentIndex = songs.findIndex((song) => song.path === currentPath);
  $: upcomingCount = currentIndex >= 0 ? Math.max(0, songs.length - currentIndex - 1) : songs.length;

  function playQueuedSong(song: LocalSong) {
    void onChooseSong(song);
  }

  function startQueuePointer(event: PointerEvent, song: LocalSong) {
    if (!event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) {
      return;
    }

    pointerMovedQueueItem = false;
    draggedPath = song.path === currentPath ? null : song.path;
    dragOverPath = null;
  }

  function enterQueueItem(song: LocalSong) {
    if (!draggedPath || draggedPath === song.path) {
      return;
    }

    pointerMovedQueueItem = true;
    const sourcePath = draggedPath;
    dragOverPath = song.path;
    onReorder(sourcePath, song.path);
  }

  function finishQueuePointer(song: LocalSong) {
    const shouldPlay = !pointerMovedQueueItem;

    draggedPath = null;
    dragOverPath = null;
    pointerMovedQueueItem = false;

    if (shouldPlay) {
      playQueuedSong(song);
    }
  }

  function cancelQueuePointer() {
    draggedPath = null;
    dragOverPath = null;
    pointerMovedQueueItem = false;
  }

</script>

{#if open}
  <div class="queue-layer" role="presentation">
    <button class="queue-scrim" type="button" aria-label="Close queue" on:click={onClose}></button>

    <aside class="queue-panel" aria-label="Queue">
      <header class="queue-header">
        <div class="min-w-0">
          <h2>Queue</h2>
          <p>
            {#if shuffleEnabled}
              Shuffle is on - upcoming order may change
            {:else if repeatMode === 'one'}
              Repeating the current track
            {:else}
              {upcomingCount} up next
            {/if}
          </p>
        </div>

        <div class="queue-header-actions">
          <button class="queue-text-button" type="button" on:click={onClear} disabled={songs.length <= 1}>Clear</button>
          <button class="queue-close-button" type="button" aria-label="Close queue" on:click={onClose}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round">
              <path d="m6 6 12 12" />
              <path d="M18 6 6 18" />
            </svg>
          </button>
        </div>
      </header>

      {#if songs.length}
        <div class="queue-list" role="list" data-no-swipe>
          {#each visibleSongs as queuedSong (queuedSong.path)}
            <div
              class:queue-item-current={queuedSong.path === currentPath}
              class:queue-item-dragging={draggedPath === queuedSong.path}
              class:queue-item-drop-target={dragOverPath === queuedSong.path}
              class="queue-item"
              role="button"
              tabindex="0"
              title={`Play ${queuedSong.title}`}
              aria-current={queuedSong.path === currentPath ? 'true' : undefined}
              on:pointerdown={(event) => startQueuePointer(event, queuedSong)}
              on:pointerenter={() => enterQueueItem(queuedSong)}
              on:pointerup={() => finishQueuePointer(queuedSong)}
              on:pointercancel={cancelQueuePointer}
              on:contextmenu={(event) => {
                event.preventDefault();
                if (queuedSong.path !== currentPath) {
                  onRemoveSong(queuedSong.path);
                }
              }}
              on:keydown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  playQueuedSong(queuedSong);
                }
              }}
            >
              <span class="queue-artwork">
                <LazyArtwork
                  rootClass="h-full w-full"
                  imageClass="h-full w-full object-cover"
                  path={queuedSong.artwork_thumb ?? queuedSong.artwork_preview ?? queuedSong.artwork}
                  alt=""
                />
              </span>

              <span class="queue-copy">
                <span class="queue-title">{queuedSong.title}</span>
                <span class="queue-artist">{queuedSong.artist}</span>
              </span>

              <span class="queue-duration">{formatDuration(queuedSong.duration)}</span>
            </div>
          {/each}

          {#if hiddenSongCount > 0}
            <div class="queue-more-note">+{hiddenSongCount} more in queue</div>
          {/if}
        </div>
      {:else}
        <div class="queue-empty">
          <p>No queue yet</p>
          <span>Play something from your library to build an up-next list.</span>
        </div>
      {/if}
    </aside>
  </div>
{/if}

<style>
  .queue-layer {
    position: absolute;
    inset: 0;
    z-index: 60;
    pointer-events: none;
  }

  .queue-scrim {
    position: absolute;
    inset: 0;
    pointer-events: auto;
    border: 0;
    background: rgba(0, 0, 0, 0.24);
  }

  .queue-panel {
    position: absolute;
    right: clamp(1rem, 2.5vw, 1.75rem);
    bottom: clamp(1rem, 3vh, 1.5rem);
    display: flex;
    width: min(24rem, calc(100vw - 2rem));
    max-height: min(42rem, calc(100vh - 2rem));
    pointer-events: auto;
    overflow: hidden;
    flex-direction: column;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 1.35rem;
    background: rgba(0, 0, 0, 0.86);
    box-shadow:
      0 2.2rem 5rem rgba(0, 0, 0, 0.52),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(22px) saturate(1.05);
    user-select: none;
    -webkit-user-select: none;
  }

  .queue-header {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1.15rem 1.2rem 0.95rem;
  }

  .queue-header h2 {
    margin: 0;
    font-size: 1.22rem;
    font-weight: 900;
    letter-spacing: -0.035em;
  }

  .queue-header p {
    margin: 0.18rem 0 0;
    overflow: hidden;
    color: rgba(255, 255, 255, 0.45);
    font-size: 0.76rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .queue-header-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.5rem;
  }

  .queue-text-button {
    height: 2.25rem;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    padding: 0 0.85rem;
    color: rgba(255, 255, 255, 0.66);
    font-size: 0.78rem;
    font-weight: 850;
    transition:
      background 150ms ease,
      color 150ms ease,
      border-color 150ms ease;
  }

  .queue-text-button:hover:not(:disabled),
  .queue-text-button:focus-visible {
    border-color: rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.08);
    color: white;
    outline: none;
  }

  .queue-text-button:disabled {
    cursor: not-allowed;
    opacity: 0.34;
  }

  .queue-close-button {
    display: grid;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    place-items: center;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.58);
    transition:
      background 150ms ease,
      color 150ms ease,
      border-color 150ms ease;
  }

  .queue-close-button:hover {
    border-color: rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .queue-list {
    position: relative;
    z-index: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.8rem;
    scrollbar-width: none;
  }

  .queue-list::-webkit-scrollbar {
    display: none;
  }

  .queue-item {
    display: flex;
    width: 100%;
    min-height: 4.2rem;
    align-items: center;
    gap: 0.75rem;
    border: 1px solid transparent;
    border-radius: 0.9rem;
    background: transparent;
    padding: 0.56rem 0.68rem;
    color: white;
    text-align: left;
    user-select: none;
    -webkit-user-select: none;
    transition:
      background 150ms ease,
      border-color 150ms ease,
      transform 150ms ease;
  }

  .queue-item:hover,
  .queue-item:focus-within {
    border-color: rgba(255, 255, 255, 0.07);
    background: rgba(255, 255, 255, 0.055);
    outline: none;
  }

  .queue-item-current {
    border-color: rgba(255, 255, 255, 0.09);
    background:
      linear-gradient(90deg, var(--accent-soft), transparent 58%),
      rgba(255, 255, 255, 0.055);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .queue-item-dragging {
    opacity: 0.82;
    transform: scale(1.01);
    box-shadow:
      0 0.75rem 2rem rgba(0, 0, 0, 0.22),
      inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }

  .queue-item-drop-target:not(.queue-item-dragging) {
    border-color: var(--accent);
    background: rgba(255, 255, 255, 0.08);
  }

  .queue-artwork {
    display: block;
    width: 3rem;
    height: 3rem;
    flex: 0 0 3rem;
    overflow: hidden;
    border-radius: 0.48rem;
    background: rgba(255, 255, 255, 0.08);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.08);
  }

  .queue-copy {
    min-width: 0;
    flex: 1 1 auto;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    outline: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .queue-copy:focus-visible .queue-title {
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-underline-offset: 0.16em;
  }

  .queue-title,
  .queue-artist {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .queue-title {
    font-size: 0.92rem;
    font-weight: 850;
    letter-spacing: -0.025em;
  }

  .queue-artist {
    margin-top: 0.08rem;
    color: rgba(255, 255, 255, 0.48);
    font-size: 0.78rem;
    font-weight: 650;
  }

  .queue-duration {
    flex: 0 0 auto;
    margin-left: 0.5rem;
    color: rgba(255, 255, 255, 0.48);
    font-size: 0.84rem;
    font-weight: 700;
  }

  .queue-more-note {
    margin: 0.45rem 0.35rem 0.15rem;
    border-radius: 0.8rem;
    background: rgba(255, 255, 255, 0.045);
    padding: 0.7rem 0.85rem;
    color: rgba(255, 255, 255, 0.46);
    font-size: 0.78rem;
    font-weight: 800;
    text-align: center;
  }

  .queue-empty {
    display: grid;
    min-height: 16rem;
    place-items: center;
    padding: 2rem;
    text-align: center;
  }

  .queue-empty p {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 900;
  }

  .queue-empty span {
    display: block;
    margin-top: 0.45rem;
    max-width: 18rem;
    color: rgba(255, 255, 255, 0.48);
    font-size: 0.9rem;
  }

  @media (max-width: 720px) {
    .queue-panel {
      top: auto;
      right: 0.75rem;
      bottom: 0.75rem;
      left: 0.75rem;
      width: auto;
      max-height: min(75vh, 42rem);
      border-radius: 1.2rem;
    }

    .queue-item {
      min-height: 3.9rem;
      gap: 0.65rem;
    }

    .queue-artwork {
      width: 2.65rem;
      height: 2.65rem;
      flex-basis: 2.65rem;
    }
  }
</style>
