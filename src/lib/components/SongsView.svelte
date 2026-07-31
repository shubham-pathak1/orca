<script lang="ts">
  import { onMount } from 'svelte';
  import { formatDuration } from '../format';
  import { artworkUrl } from '../tauri';
  import type { LocalSong } from '../types';
  import AlphabetRail from './AlphabetRail.svelte';
  import LazyArtwork from './LazyArtwork.svelte';

  export let songs: LocalSong[] = [];
  export let filteredSongs: LocalSong[] = [];
  export let query = '';
  export let selectedPath: string | null = null;
  export let currentPath: string | null = null;
  export let status = 'Ready';
  export let sortKey: 'title' | 'artist' | 'album' = 'title';
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};

  let songListEl: HTMLDivElement;
  let songLayout: 'list' | 'grid' = 'list';
  let sortMenuOpen = false;
  let songScrollTop = 0;
  let songViewportHeight = 0;
  let songViewportWidth = 0;

  const LIST_ROW_HEIGHT = 40;
  const GRID_MIN_COLUMN_WIDTH = 132;
  const GRID_GAP = 16;
  const GRID_TEXT_HEIGHT = 50;
  const OVERSCAN_ROWS = 4;

  const sortOptions: { key: 'title' | 'artist' | 'album'; label: string }[] = [
    { key: 'title', label: 'Title' },
    { key: 'artist', label: 'Artist' },
    { key: 'album', label: 'Album' }
  ];

  onMount(() => {
    const savedLayout = window.localStorage.getItem('orca.librarySongLayout');
    if (savedLayout === 'grid' || savedLayout === 'list') {
      songLayout = savedLayout;
    }

    const savedSort = window.localStorage.getItem('orca.librarySortKey');
    if (savedSort === 'title' || savedSort === 'artist' || savedSort === 'album') {
      sortKey = savedSort;
    }
  });

  $: sortedSongs = [...filteredSongs].sort((a, b) => compareSongs(a, b, sortKey));
  // Reset the virtual-list scroll position whenever the visible song set changes
  // so the window always starts from the top after a search or sort change.
  $: {
    filteredSongs;
    sortKey;
    songScrollTop = 0;
    if (songListEl) songListEl.scrollTop = 0;
  }
  $: listVisibleStart = Math.max(0, Math.floor(songScrollTop / LIST_ROW_HEIGHT) - OVERSCAN_ROWS);
  $: listVisibleEnd = Math.min(
    sortedSongs.length,
    Math.ceil((songScrollTop + songViewportHeight) / LIST_ROW_HEIGHT) + OVERSCAN_ROWS
  );
  $: visibleListSongs = sortedSongs.slice(listVisibleStart, listVisibleEnd);
  $: gridColumnCount = Math.max(1, Math.floor((songViewportWidth + GRID_GAP) / (GRID_MIN_COLUMN_WIDTH + GRID_GAP)));
  $: gridItemWidth = gridColumnCount > 0
    ? Math.max(GRID_MIN_COLUMN_WIDTH, (songViewportWidth - GRID_GAP * (gridColumnCount - 1)) / gridColumnCount)
    : GRID_MIN_COLUMN_WIDTH;
  $: gridRowHeight = gridItemWidth + GRID_TEXT_HEIGHT + GRID_GAP;
  $: gridRowCount = Math.ceil(sortedSongs.length / gridColumnCount);
  $: gridVisibleRowStart = Math.max(0, Math.floor(songScrollTop / gridRowHeight) - OVERSCAN_ROWS);
  $: gridVisibleRowEnd = Math.min(
    gridRowCount,
    Math.ceil((songScrollTop + songViewportHeight) / gridRowHeight) + OVERSCAN_ROWS
  );
  $: gridVisibleStart = gridVisibleRowStart * gridColumnCount;
  $: gridVisibleEnd = Math.min(sortedSongs.length, gridVisibleRowEnd * gridColumnCount);
  $: visibleGridSongs = sortedSongs.slice(gridVisibleStart, gridVisibleEnd);
  $: currentSortLabel = sortOptions.find((option) => option.key === sortKey)?.label ?? 'Title';

  function compareSongs(a: LocalSong, b: LocalSong, key: 'title' | 'artist' | 'album') {
    const primary = a[key].localeCompare(b[key], undefined, { sensitivity: 'base' });
    if (primary !== 0) {
      return primary;
    }

    return a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });
  }

  function rowArtwork(song: LocalSong): string | null {
    return song.artwork_thumb ?? song.artwork_preview ?? null;
  }

  function previewArtwork(song: LocalSong): string | null {
    return song.artwork_preview ?? song.artwork_thumb ?? null;
  }

  function selectSort(key: 'title' | 'artist' | 'album') {
    sortKey = key;
    window.localStorage.setItem('orca.librarySortKey', key);
    sortMenuOpen = false;
  }

  function setSongLayout(layout: 'list' | 'grid') {
    songLayout = layout;
    window.localStorage.setItem('orca.librarySongLayout', layout);
  }

  function initialFromText(value: string): string {
    const first = value.trim().charAt(0).toUpperCase();
    return /^[A-Z]$/.test(first) ? first : '#';
  }

  function jumpToSongLetter(letter: string) {
    if (!songListEl) {
      return;
    }

    const letters = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('')];
    const startIndex = letters.indexOf(letter);
    const searchOrder = startIndex >= 0 ? letters.slice(startIndex) : [letter];
    const targetIndex = sortedSongs.findIndex((song) => searchOrder.includes(initialFromText(song.title)));

    if (targetIndex < 0) {
      return;
    }

    const top = songLayout === 'list'
      ? targetIndex * LIST_ROW_HEIGHT
      : Math.floor(targetIndex / gridColumnCount) * gridRowHeight;

    songListEl.scrollTo({ top, behavior: 'smooth' });
  }

  function updateSongScroll(event: Event) {
    songScrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  function closeSortMenu() {
    sortMenuOpen = false;
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeSortMenu();
    }
  }
</script>

<svelte:window on:click={closeSortMenu} on:keydown={handleGlobalKeydown} />

<div class="mb-4 grid grid-cols-[minmax(200px,1fr)_minmax(220px,380px)_84px_140px] items-center gap-4 max-lg:grid-cols-1">
  <slot />
  <div class="grid h-10 grid-cols-2 overflow-hidden rounded-md border border-white/10 bg-white/[0.035] p-1">
    <button
      class={`grid place-items-center rounded-sm transition ${songLayout === 'list' ? 'bg-white text-black' : 'text-white/58 hover:bg-white/[0.08] hover:text-white'}`}
      type="button"
      title="List view"
      aria-label="List view"
      on:click={() => setSongLayout('list')}
    >
      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M8 6h13M8 12h13M8 18h13" />
        <path d="M3 6h.01M3 12h.01M3 18h.01" />
      </svg>
    </button>
    <button
      class={`grid place-items-center rounded-sm transition ${songLayout === 'grid' ? 'bg-white text-black' : 'text-white/58 hover:bg-white/[0.08] hover:text-white'}`}
      type="button"
      title="Grid view"
      aria-label="Grid view"
      on:click={() => setSongLayout('grid')}
    >
      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="3" width="7" height="7" rx="1" />
        <rect x="14" y="3" width="7" height="7" rx="1" />
        <rect x="3" y="14" width="7" height="7" rx="1" />
        <rect x="14" y="14" width="7" height="7" rx="1" />
      </svg>
    </button>
  </div>
  <div class="relative" on:click|stopPropagation role="presentation">
    <button
      class="flex h-10 w-full items-center justify-between rounded-md border border-white/10 bg-white/[0.04] px-3 text-xs font-semibold text-white/72 outline-none transition hover:border-white/20 hover:bg-white/[0.06]"
      type="button"
      on:click={() => (sortMenuOpen = !sortMenuOpen)}
    >
      <span>Sort: {currentSortLabel}</span>
      <svg class={`h-4 w-4 transition ${sortMenuOpen ? 'rotate-180' : ''}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="m6 9 6 6 6-6" />
      </svg>
    </button>
    {#if sortMenuOpen}
      <div class="absolute right-0 top-11 z-20 w-full overflow-hidden rounded-md border border-white/10 bg-[#171719] p-1 shadow-[0_18px_60px_rgba(0,0,0,0.36)]" role="menu">
        {#each sortOptions as option}
          <button
            class={`flex h-9 w-full items-center rounded-sm px-3 text-left text-xs font-semibold transition ${sortKey === option.key ? 'bg-white/12 text-white' : 'text-white/54 hover:bg-white/[0.07] hover:text-white'}`}
            type="button"
            role="menuitem"
            on:click={() => selectSort(option.key)}
          >
            Sort: {option.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<div class="h-[calc(100%-72px)] min-h-0 overflow-hidden">
  {#if songLayout === 'list'}
    <div class="grid grid-cols-[minmax(0,1fr)_24px]">
      <div class="grid h-8 grid-cols-[minmax(240px,1.35fr)_minmax(130px,0.7fr)_minmax(130px,0.8fr)_72px] items-center gap-3 border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36 max-lg:grid-cols-[minmax(220px,1fr)_90px]">
        <span>Title</span>
        <span class="max-lg:hidden">Artist</span>
        <span class="max-lg:hidden">Album</span>
        <span class="text-right">Duration</span>
      </div>
      <div></div>
    </div>
    <div class="grid h-[calc(100%-32px)] grid-cols-[minmax(0,1fr)_24px]">
      <div
        class="scrollbar-none overflow-auto"
        bind:this={songListEl}
        bind:clientHeight={songViewportHeight}
        bind:clientWidth={songViewportWidth}
        on:scroll={updateSongScroll}
      >
        {#if sortedSongs.length}
          <div class="relative" style={`height: ${sortedSongs.length * LIST_ROW_HEIGHT}px;`}>
          {#each visibleListSongs as song, index (song.path)}
          <button
            data-letter={initialFromText(song.title)}
            class={`absolute left-0 grid min-h-10 w-full grid-cols-[minmax(240px,1.35fr)_minmax(130px,0.7fr)_minmax(130px,0.8fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition max-lg:grid-cols-[minmax(220px,1fr)_90px] ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : selectedPath === song.path ? 'bg-white/[0.055]' : 'hover:bg-white/[0.045]'}`}
            style={`height: ${LIST_ROW_HEIGHT}px; transform: translateY(${(listVisibleStart + index) * LIST_ROW_HEIGHT}px);`}
            on:click={() => onChooseSong(song, sortedSongs)}
            on:contextmenu={(event) => onOpenSongMenu(event, song)}
          >
            <span class="flex min-w-0 items-center gap-2">
              {#if artworkUrl(song.artwork)}
                <LazyArtwork rootClass="h-7 w-7 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
              {:else}
                <img src="/cover.png" class="h-7 w-7 shrink-0 rounded-sm object-cover" alt="" />
              {/if}
              <span class="min-w-0">
                <span class="block truncate text-sm font-semibold text-white">{song.title}</span>
              </span>
            </span>
            <span class="truncate text-xs text-white/52 max-lg:hidden">{song.artist}</span>
            <span class="truncate text-xs text-white/42 max-lg:hidden">{song.album}</span>
            <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
          </button>
          {/each}
          </div>
        {:else}
          {#if songs.length === 0 && !query.trim()}
            <div class="mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center px-2">
              <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
              <h2 class="mt-3 text-4xl font-black tracking-normal">{status}</h2>
            </div>
          {:else}
            <div class="mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center px-2">
              <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
              <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such song found :(</h2>
              <p class="mt-3 text-sm leading-6 text-white/48">Try another title, artist, album, or format.</p>
            </div>
          {/if}
        {/if}
      </div>
      <AlphabetRail onJump={jumpToSongLetter} />
    </div>
  {:else}
    <div class="grid h-full grid-cols-[minmax(0,1fr)_24px]">
      <div
        class="scrollbar-none max-h-full overflow-auto pr-3"
        bind:this={songListEl}
        bind:clientHeight={songViewportHeight}
        bind:clientWidth={songViewportWidth}
        on:scroll={updateSongScroll}
      >
        {#if sortedSongs.length}
          <div class="relative" style={`height: ${gridRowCount * gridRowHeight}px;`}>
          {#each visibleGridSongs as song, index (song.path)}
          {@const absoluteIndex = gridVisibleStart + index}
          {@const column = absoluteIndex % gridColumnCount}
          {@const row = Math.floor(absoluteIndex / gridColumnCount)}
          <button
            data-letter={initialFromText(song.title)}
            class={`absolute min-w-0 text-left transition ${song.path === currentPath ? 'opacity-100' : selectedPath === song.path ? 'opacity-90' : 'opacity-76 hover:opacity-100'}`}
            style={`width: ${gridItemWidth}px; transform: translate(${column * (gridItemWidth + GRID_GAP)}px, ${row * gridRowHeight}px);`}
            on:click={() => onChooseSong(song, sortedSongs)}
            on:contextmenu={(event) => onOpenSongMenu(event, song)}
          >
            <span class={`relative block aspect-square overflow-hidden rounded-md ${artworkUrl(song.artwork) ? 'bg-white/[0.07]' : ''} ${song.path === currentPath ? 'ring-2 ring-[var(--accent)]' : ''}`}>
              {#if artworkUrl(song.artwork)}
                <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={previewArtwork(song)} alt="" />
              {:else}
                <img src="/cover.png" class="h-full w-full object-cover" alt="" />
              {/if}
            </span>
            <span class="mt-2 block truncate text-sm font-bold text-white">{song.title}</span>
            <span class="block truncate text-xs text-white/46">{song.artist}</span>
          </button>
          {/each}
          </div>
        {:else}
          {#if songs.length === 0 && !query.trim()}
            <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
              <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
              <h2 class="mt-3 text-4xl font-black tracking-normal">{status}</h2>
            </div>
          {:else}
            <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
              <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
              <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such song found :(</h2>
              <p class="mt-3 text-sm leading-6 text-white/48">Try another title, artist, album, or format.</p>
            </div>
          {/if}
        {/if}
      </div>
      <AlphabetRail onJump={jumpToSongLetter} />
    </div>
  {/if}
</div>
