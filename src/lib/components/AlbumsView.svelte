<script lang="ts">
  import { tick } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatDuration, formatQuality, formatTotalDuration } from '../format';
  import type { LocalSong, AlbumEntry } from '../types';
  import AlphabetRail from './AlphabetRail.svelte';
  import LazyArtwork from './LazyArtwork.svelte';

  export let albums: AlbumEntry[] = [];
  export let songs: LocalSong[] = [];
  export let query = '';
  export let currentPath: string | null = null;
  export let showQualityInfo = true;
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onChooseAlbumCover: (albumKey: string) => Promise<void> | void = () => {};
  export let onRemoveAlbumCover: (albumKey: string) => Promise<void> | void = () => {};
  export let onFetchAlbumArtworkManual: (albumKey: string, artist: string, album: string) => Promise<void> | void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};
  /** Optional: pre-open a specific album (e.g. navigating from artist detail) */
  export let initialAlbumKey: string | null = null;

  // Exported so LibraryView can toggle header visibility / height
  export let isInDetail = false;

  let albumListEl: HTMLDivElement;
  let selectedAlbumKey: string | null = null;
  let detailQuery = '';
  let savedScrollTop = 0;
  let albumContextMenu: { x: number; y: number; album: AlbumEntry } | null = null;
  let albumScrollTop = 0;
  let albumViewportHeight = 0;
  let albumViewportWidth = 0;

  const GRID_MIN_COLUMN_WIDTH = 132;
  const GRID_GAP = 12;
  const GRID_TEXT_HEIGHT = 46;
  const OVERSCAN_ROWS = 3;

  $: isInDetail = Boolean(selectedAlbumKey);

  // Open initial album when prop is set from parent
  $: if (initialAlbumKey && initialAlbumKey !== selectedAlbumKey) {
    openAlbum(initialAlbumKey);
  }

  $: albumEntries = albums.filter((album) =>
    !query ||
    album.title.toLowerCase().includes(query.trim().toLowerCase()) ||
    album.artist.toLowerCase().includes(query.trim().toLowerCase())
  );
  $: selectedAlbum = selectedAlbumKey
    ? albumEntries.find((a) => a.key === selectedAlbumKey) ?? null
    : null;
  $: selectedAlbumSongs = selectedAlbum
    ? songs
        .filter((song) => `${song.album_artist}:${song.album}` === selectedAlbum.key)
        .sort((a, b) => (a.track_number ?? 999) - (b.track_number ?? 999) || a.title.localeCompare(b.title))
    : [];
  $: selectedAlbumVisibleSongs = filterDetailSongs(selectedAlbumSongs, detailQuery);
  $: albumColumnCount = Math.max(1, Math.floor((albumViewportWidth + GRID_GAP) / (GRID_MIN_COLUMN_WIDTH + GRID_GAP)));
  $: albumItemWidth = Math.max(
    GRID_MIN_COLUMN_WIDTH,
    (albumViewportWidth - GRID_GAP * (albumColumnCount - 1)) / albumColumnCount
  );
  $: albumRowHeight = albumItemWidth + GRID_TEXT_HEIGHT + GRID_GAP;
  $: albumRowCount = Math.ceil(albumEntries.length / albumColumnCount);
  $: albumVisibleRowStart = Math.max(0, Math.floor(albumScrollTop / albumRowHeight) - OVERSCAN_ROWS);
  $: albumVisibleRowEnd = Math.min(
    albumRowCount,
    Math.ceil((albumScrollTop + albumViewportHeight) / albumRowHeight) + OVERSCAN_ROWS
  );
  $: albumVisibleStart = albumVisibleRowStart * albumColumnCount;
  $: albumVisibleEnd = Math.min(albumEntries.length, albumVisibleRowEnd * albumColumnCount);
  $: visibleAlbums = albumEntries.slice(albumVisibleStart, albumVisibleEnd);
  $: {
    albumEntries;
    albumScrollTop = 0;
    if (albumListEl) albumListEl.scrollTop = 0;
  }

  function filterDetailSongs(sourceSongs: LocalSong[], searchQuery: string) {
    const needle = searchQuery.trim().toLowerCase();
    if (!needle) return sourceSongs;
    return sourceSongs.filter((song) =>
      [song.title, song.artist, song.album].some((v) => v.toLowerCase().includes(needle))
    );
  }

  function rowArtwork(song: LocalSong): string | null {
    return song.artwork_thumb ?? song.artwork_preview ?? null;
  }

  function initialFromText(value: string): string {
    const first = value.trim().charAt(0).toUpperCase();
    return /^[A-Z]$/.test(first) ? first : '#';
  }

  function jumpToLetter(letter: string) {
    if (!albumListEl) return;
    const letters = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('')];
    const startIndex = letters.indexOf(letter);
    const searchOrder = startIndex >= 0 ? letters.slice(startIndex) : [letter];
    const targetIndex = searchOrder
      .map((candidate) => albumEntries.findIndex((album) => initialFromText(album.title) === candidate))
      .find((index) => index >= 0);
    if (targetIndex === undefined) return;
    albumListEl.scrollTo({
      top: Math.floor(targetIndex / albumColumnCount) * albumRowHeight,
      behavior: 'smooth'
    });
  }

  function updateAlbumScroll(event: Event) {
    albumScrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  export function openAlbum(key: string) {
    if (albumListEl) savedScrollTop = albumListEl.scrollTop;
    selectedAlbumKey = key;
    detailQuery = '';
  }

  export function closeAlbum() {
    selectedAlbumKey = null;
    detailQuery = '';
    void tick().then(() => { if (albumListEl) albumListEl.scrollTop = savedScrollTop; });
  }

  async function chooseSelectedAlbumCover() {
    if (selectedAlbum) await onChooseAlbumCover(selectedAlbum.key);
  }

  async function removeSelectedAlbumCover() {
    if (selectedAlbum) await onRemoveAlbumCover(selectedAlbum.key);
  }

  function playFirstSong(sourceSongs: LocalSong[]) {
    const first = sourceSongs[0];
    if (first) onChooseSong(first, sourceSongs);
  }

  function openAlbumMenu(event: MouseEvent, album: AlbumEntry) {
    event.preventDefault();
    event.stopPropagation();
    albumContextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 200),
      y: Math.min(event.clientY, window.innerHeight - 130),
      album
    };
  }

  function closeAlbumContextMenu() { albumContextMenu = null; }

  async function fetchContextAlbumArt() {
    const album = albumContextMenu?.album;
    closeAlbumContextMenu();
    if (album) await onFetchAlbumArtworkManual(album.key, album.artist, album.title);
  }
</script>

<svelte:window on:click={closeAlbumContextMenu} />

{#if selectedAlbum}
  <div class="scrollbar-none h-full overflow-auto">
    <!-- Detail header -->
    <div class="relative mb-8 overflow-hidden rounded-md px-5 pb-6 pt-5">
      <div class="pointer-events-none absolute inset-0 transform-gpu bg-cover bg-center opacity-20 blur-3xl"
        style={`background-image: ${artworkUrl(selectedAlbum.artwork) ? `url("${artworkUrl(selectedAlbum.artwork)}")` : 'none'}`}></div>
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.05] via-transparent to-black/30"></div>
      <div class="relative mb-5 flex items-center justify-between gap-4">
        <button class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
          type="button" title="Back" aria-label="Back" on:click={closeAlbum}>
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
        <label class="w-full max-w-xl">
          <span class="sr-only">Search songs in album</span>
          <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
            bind:value={detailQuery} placeholder="Search songs in {selectedAlbum.title}..." />
        </label>
      </div>
      <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
        <div class="group relative aspect-square w-[148px] shrink-0 overflow-hidden rounded-md bg-white/8 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
          {#if artworkUrl(selectedAlbum.artwork)}
            <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedAlbum.artwork} alt="" />
          {/if}
          <div class="absolute inset-x-2 bottom-2 flex justify-end gap-2 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100">
            <button class="grid h-8 w-8 place-items-center rounded-full bg-white text-black shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md"
              type="button" title="Change cover" aria-label="Change cover" on:click={chooseSelectedAlbumCover}>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
              </svg>
            </button>
            <button class="grid h-8 w-8 place-items-center rounded-full bg-black text-white shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md"
              type="button" title="Remove cover" aria-label="Remove cover" on:click={removeSelectedAlbumCover}>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" /><path d="M10 11v5M14 11v5" />
              </svg>
            </button>
          </div>
        </div>
        <div class="min-w-0">
          <h2 class="truncate text-6xl font-black max-xl:text-5xl">{selectedAlbum.title}</h2>
          <p class="mt-3 text-sm text-white/62">By {selectedAlbum.artist}</p>
          <p class="mt-1 flex items-center gap-1.5 text-xs text-white/42">
            <span>{selectedAlbum.song_count} {selectedAlbum.song_count === 1 ? 'track' : 'tracks'}</span>
            <span class="text-[6px] opacity-40">&#9679;</span>
            <span>{formatTotalDuration(selectedAlbum.duration)}</span>
          </p>
          <div class="mt-5 flex items-center gap-2">
            <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105"
              title="Play album" on:click={() => playFirstSong(selectedAlbumVisibleSongs)}>
              <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Track list + more albums -->
    <div class="grid grid-cols-[minmax(0,1fr)_340px] gap-8 max-2xl:grid-cols-1">
      <div>
        <div class="grid h-8 grid-cols-[48px_minmax(240px,1fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36">
          <span>#</span><span>Title</span><span class="text-right">Duration</span>
        </div>
        {#each selectedAlbumVisibleSongs as song, index}
          <button class={`grid min-h-11 w-full grid-cols-[48px_minmax(240px,1fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.045]'}`}
            on:click={() => onChooseSong(song, selectedAlbumVisibleSongs)}
            on:contextmenu={(e) => onOpenSongMenu(e, song)}>
            <span class="text-sm text-white/36">{song.track_number ?? index + 1}</span>
            <span class="flex min-w-0 items-center gap-2">
              {#if artworkUrl(song.artwork)}
                <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
              {:else}
                <img src="/cover.png" class="h-8 w-8 shrink-0 rounded-sm object-cover" alt="" />
              {/if}
              <span class="min-w-0">
                <span class="block truncate text-sm font-semibold text-white">{song.title}</span>
                {#if showQualityInfo}
                  <span class="block truncate text-xs text-white/38">{formatQuality(song.format, song.sample_rate, song.bitrate)}</span>
                {/if}
              </span>
            </span>
            <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
          </button>
        {/each}
        {#if !selectedAlbumVisibleSongs.length}
          <div class="mx-auto flex min-h-[220px] max-w-xl flex-col items-center justify-center px-2 text-center">
            <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
            <h2 class="mt-3 text-3xl font-black tracking-normal">Oops, no songs in this album match :(</h2>
            <p class="mt-3 text-sm leading-6 text-white/48">Try a different search inside this album.</p>
          </div>
        {/if}
      </div>
      <div>
        <h3 class="mb-3 text-sm font-black">More albums from {selectedAlbum.artist}</h3>
        <div class="grid grid-cols-2 gap-3">
          {#each albumEntries.filter((a) => a.artist === selectedAlbum.artist && a.key !== selectedAlbum.key).slice(0, 6) as album}
            <button class="min-w-0 rounded-md bg-white/[0.035] p-2 text-left transition hover:bg-white/[0.07]"
              on:click={() => openAlbum(album.key)} on:contextmenu={(e) => openAlbumMenu(e, album)}>
              <div class="aspect-square overflow-hidden rounded bg-white/8">
                {#if artworkUrl(album.artwork)}
                  <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
                {:else}
                  <img src="/cover.png" class="h-full w-full object-cover" alt="" />
                {/if}
              </div>
              <p class="mt-2 truncate text-xs font-bold">{album.title}</p>
              <p class="truncate text-[11px] text-white/40">{album.song_count} tracks</p>
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>

{:else}
  <!-- Album grid -->
  <div class="grid h-full grid-cols-[minmax(0,1fr)_24px]">
    <div class="scrollbar-none max-h-full overflow-auto pr-2"
      bind:this={albumListEl}
      bind:clientHeight={albumViewportHeight}
      bind:clientWidth={albumViewportWidth}
      on:scroll={updateAlbumScroll}>
      {#if albumEntries.length}
        <div class="relative" style={`height: ${albumRowCount * albumRowHeight}px;`}>
        {#each visibleAlbums as album, index (album.key)}
          <button
            class="absolute text-left opacity-82 transition hover:opacity-100"
            style={`width: ${albumItemWidth}px; transform: translate(${((albumVisibleStart + index) % albumColumnCount) * (albumItemWidth + GRID_GAP)}px, ${Math.floor((albumVisibleStart + index) / albumColumnCount) * albumRowHeight}px);`}
            on:click={() => openAlbum(album.key)} on:contextmenu={(e) => openAlbumMenu(e, album)}>
            <div class={`relative aspect-square overflow-hidden rounded-md ${artworkUrl(album.artwork) ? 'bg-white/[0.07]' : ''}`}>
              {#if artworkUrl(album.artwork)}
                <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
              {:else}
                <img src="/cover.png" class="h-full w-full object-cover" alt="" />
              {/if}
            </div>
            <p class="mt-2 truncate text-sm font-semibold">{album.title}</p>
            <p class="truncate text-xs text-white/40">{album.artist}</p>
          </button>
        {/each}
        </div>
      {:else}
        <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
          <p class="text-sm font-bold uppercase text-white/34">No albums found</p>
          <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such album found :(</h2>
          <p class="mt-3 text-sm leading-6 text-white/48">Try another album or artist name.</p>
        </div>
      {/if}
    </div>
    <AlphabetRail onJump={jumpToLetter} />
  </div>
{/if}

{#if albumContextMenu}
  <div role="menu" tabindex="-1"
    class="fixed z-50 w-52 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
    style={`left: ${albumContextMenu.x}px; top: ${albumContextMenu.y}px;`}
    on:click|stopPropagation on:keydown|stopPropagation>
    <div class="border-b border-white/[0.06] px-3 py-2">
      <p class="truncate text-xs font-bold text-white">{albumContextMenu.album.title}</p>
      <p class="truncate text-[11px] text-white/42">{albumContextMenu.album.artist}</p>
    </div>
    <button role="menuitem"
      class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white"
      on:click={fetchContextAlbumArt}>
      Fetch Artwork Online
    </button>
  </div>
{/if}
