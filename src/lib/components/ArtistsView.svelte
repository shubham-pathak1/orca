<script lang="ts">
  import { tick } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatDuration, formatTotalDuration } from '../format';
  import type { LocalSong, ArtistEntry, AlbumEntry } from '../types';
  import AlphabetRail from './AlphabetRail.svelte';
  import LazyArtwork from './LazyArtwork.svelte';

  export let artists: ArtistEntry[] = [];
  export let albums: AlbumEntry[] = [];
  export let songs: LocalSong[] = [];
  export let query = '';
  export let currentPath: string | null = null;
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onChooseArtistCover: (artistName: string) => Promise<void> | void = () => {};
  export let onRemoveArtistCover: (artistName: string) => Promise<void> | void = () => {};
  export let onFetchArtistArtworkManual: (artistName: string) => Promise<void> | void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};
  export let onOpenAlbum: (key: string) => void = () => {};

  // Exported so LibraryView can toggle header/height
  export let isInDetail = false;

  let artistListEl: HTMLDivElement;
  let selectedArtistName: string | null = null;
  let detailQuery = '';
  let savedScrollTop = 0;
  let artistContextMenu: { x: number; y: number; artist: ArtistEntry } | null = null;
  let artistScrollTop = 0;
  let artistViewportHeight = 0;
  let artistViewportWidth = 0;
  let windowWidth = 0;

  const ARTIST_GRID_GAP = 12;
  const ARTIST_ROW_HEIGHT = 68;
  const OVERSCAN_ROWS = 4;

  $: isInDetail = Boolean(selectedArtistName);

  $: artistEntries = artists.filter((artist) =>
    !query || artist.name.toLowerCase().includes(query.trim().toLowerCase())
  );
  $: selectedArtistSongs = selectedArtistName
    ? songs.filter((song) => song.artist === selectedArtistName).sort((a, b) => a.title.localeCompare(b.title))
    : [];
  $: selectedArtistVisibleSongs = filterDetailSongs(selectedArtistSongs, detailQuery);
  $: selectedArtist = selectedArtistName
    ? {
        name: selectedArtistName,
        songs: selectedArtistSongs,
        artwork: artists.find((a) => a.name === selectedArtistName)?.artwork ?? null,
        song_artwork: artists.find((a) => a.name === selectedArtistName)?.song_artwork ?? null,
        albums: albums.filter((album) => album.artist === selectedArtistName)
      }
    : null;
  $: artistColumnCount = windowWidth >= 1536
    ? 5
    : artistViewportWidth >= 1024
      ? 4
      : artistViewportWidth >= 768
        ? 3
        : 2;
  $: artistItemWidth = Math.max(
    0,
    (artistViewportWidth - ARTIST_GRID_GAP * (artistColumnCount - 1)) / artistColumnCount
  );
  $: artistRowCount = Math.ceil(artistEntries.length / artistColumnCount);
  $: artistVisibleRowStart = Math.max(0, Math.floor(artistScrollTop / ARTIST_ROW_HEIGHT) - OVERSCAN_ROWS);
  $: artistVisibleRowEnd = Math.min(
    artistRowCount,
    Math.ceil((artistScrollTop + artistViewportHeight) / ARTIST_ROW_HEIGHT) + OVERSCAN_ROWS
  );
  $: artistVisibleStart = artistVisibleRowStart * artistColumnCount;
  $: artistVisibleEnd = Math.min(artistEntries.length, artistVisibleRowEnd * artistColumnCount);
  $: visibleArtists = artistEntries.slice(artistVisibleStart, artistVisibleEnd);
  $: {
    artistEntries;
    artistScrollTop = 0;
    if (artistListEl) artistListEl.scrollTop = 0;
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
    if (!artistListEl) return;
    const letters = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('')];
    const startIndex = letters.indexOf(letter);
    const searchOrder = startIndex >= 0 ? letters.slice(startIndex) : [letter];
    const targetIndex = searchOrder
      .map((candidate) => artistEntries.findIndex((artist) => initialFromText(artist.name) === candidate))
      .find((index) => index >= 0);
    if (targetIndex === undefined) return;
    artistListEl.scrollTo({
      top: Math.floor(targetIndex / artistColumnCount) * ARTIST_ROW_HEIGHT,
      behavior: 'smooth'
    });
  }

  function updateArtistScroll(event: Event) {
    artistScrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  function openArtist(name: string) {
    if (artistListEl) savedScrollTop = artistListEl.scrollTop;
    selectedArtistName = name;
    detailQuery = '';
  }

  function closeArtist() {
    selectedArtistName = null;
    detailQuery = '';
    void tick().then(() => { if (artistListEl) artistListEl.scrollTop = savedScrollTop; });
  }

  async function chooseSelectedArtistCover() {
    if (selectedArtist) await onChooseArtistCover(selectedArtist.name);
  }

  async function removeSelectedArtistCover() {
    if (selectedArtist) await onRemoveArtistCover(selectedArtist.name);
  }

  function playFirstSong(sourceSongs: LocalSong[]) {
    const first = sourceSongs[0];
    if (first) onChooseSong(first, sourceSongs);
  }

  function openArtistMenu(event: MouseEvent, artist: ArtistEntry) {
    event.preventDefault();
    event.stopPropagation();
    artistContextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 200),
      y: Math.min(event.clientY, window.innerHeight - 130),
      artist
    };
  }

  function closeArtistContextMenu() { artistContextMenu = null; }

  async function fetchContextArtistImage() {
    const artist = artistContextMenu?.artist;
    closeArtistContextMenu();
    if (artist) await onFetchArtistArtworkManual(artist.name);
  }
</script>

<svelte:window bind:innerWidth={windowWidth} on:click={closeArtistContextMenu} />

{#if selectedArtist}
  <div class="scrollbar-none h-full overflow-auto">
    <!-- Detail header -->
    <div class="relative mb-8 overflow-hidden rounded-md px-5 pb-6 pt-5">
      <div class="pointer-events-none absolute inset-0 transform-gpu bg-cover bg-center opacity-20 blur-3xl"
        style={`background-image: ${artworkUrl(selectedArtist.artwork) ? `url("${artworkUrl(selectedArtist.artwork)}")` : 'none'}`}></div>
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.05] via-transparent to-black/30"></div>
      <div class="relative mb-5 flex items-center justify-between gap-4">
        <button class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
          type="button" title="Back" aria-label="Back" on:click={closeArtist}>
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
        <label class="w-full max-w-xl">
          <span class="sr-only">Search songs by artist</span>
          <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
            bind:value={detailQuery} placeholder="Search songs by {selectedArtist.name}..." />
        </label>
      </div>
      <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
        <div class="group relative aspect-square w-[148px] shrink-0 overflow-hidden rounded-full bg-white/8 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
          {#if artworkUrl(selectedArtist.artwork ?? selectedArtist.song_artwork)}
            <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedArtist.artwork ?? selectedArtist.song_artwork} alt="" />
          {:else}
            <img src="/cover.png" class="h-full w-full object-cover" alt="" />
          {/if}
          <div class="absolute inset-0 rounded-full bg-black/40 opacity-0 transition duration-200 group-hover:opacity-100 group-focus-within:opacity-100">
            <div class="absolute bottom-4 right-[12%] flex gap-2">
              <button class="grid h-8 w-8 place-items-center rounded-full bg-white text-black shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md transition hover:scale-110"
                type="button" title="Change cover" aria-label="Change cover" on:click={chooseSelectedArtistCover}>
                <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
                </svg>
              </button>
              <button class="grid h-8 w-8 place-items-center rounded-full bg-black text-white shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md transition hover:scale-110"
                type="button" title="Remove cover" aria-label="Remove cover" on:click={removeSelectedArtistCover}>
                <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" /><path d="M10 11v5M14 11v5" />
                </svg>
              </button>
            </div>
          </div>
        </div>
        <div class="min-w-0">
          <h2 class="truncate text-6xl font-black leading-normal max-xl:text-5xl">{selectedArtist.name}</h2>
          <p class="mt-3 flex items-center gap-1.5 text-sm text-white/62">
            <span>{selectedArtist.albums.length} {selectedArtist.albums.length === 1 ? 'album' : 'albums'}</span>
            <span class="text-[6px] opacity-40">&#9679;</span>
            <span>{selectedArtist.songs.length} {selectedArtist.songs.length === 1 ? 'song' : 'songs'}</span>
            <span class="text-[6px] opacity-40">&#9679;</span>
            <span>{formatTotalDuration(selectedArtist.songs.reduce((acc, s) => acc + (s.duration || 0), 0))}</span>
          </p>
          <div class="mt-5 flex items-center gap-2">
            <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105"
              title="Play artist" on:click={() => playFirstSong(selectedArtistVisibleSongs)}>
              <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Songs + Albums sidebar -->
    <div class="grid grid-cols-[minmax(0,1fr)_340px] gap-8 max-2xl:grid-cols-1">
      <div>
        <div class="grid h-8 grid-cols-[48px_minmax(240px,1fr)_minmax(140px,0.6fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36">
          <span>#</span><span>Title</span><span class="max-lg:hidden">Album</span><span class="text-right">Duration</span>
        </div>
        {#each selectedArtistVisibleSongs as song, index}
          <button class={`grid min-h-11 w-full grid-cols-[48px_minmax(240px,1fr)_minmax(140px,0.6fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.045]'}`}
            on:click={() => onChooseSong(song, selectedArtistVisibleSongs)} on:contextmenu={(e) => onOpenSongMenu(e, song)}>
            <span class="text-sm text-white/36">{index + 1}</span>
            <span class="flex min-w-0 items-center gap-2">
              {#if artworkUrl(song.artwork)}
                <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
              {/if}
              <span class="truncate text-sm font-semibold text-white">{song.title}</span>
            </span>
            <span class="truncate text-xs text-white/42 max-lg:hidden">{song.album}</span>
            <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
          </button>
        {/each}
        {#if !selectedArtistVisibleSongs.length}
          <div class="mx-auto flex min-h-[220px] max-w-xl flex-col items-center justify-center px-2 text-center">
            <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
            <h2 class="mt-3 text-3xl font-black tracking-normal">Oops, no songs by this artist match :(</h2>
            <p class="mt-3 text-sm leading-6 text-white/48">Try a different search inside this artist page.</p>
          </div>
        {/if}
      </div>
      <div>
        <h3 class="mb-3 text-base font-black">Albums</h3>
        <div class="grid grid-cols-2 gap-3">
          {#each selectedArtist.albums.slice(0, 6) as album}
            <button class="min-w-0 rounded-md bg-white/[0.035] p-2 text-left transition hover:bg-white/[0.07]"
              on:click={() => onOpenAlbum(album.key)}>
              <div class="aspect-square overflow-hidden rounded bg-white/8">
                {#if artworkUrl(album.artwork)}
                  <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
                {:else}
                  <img src="/cover.png" class="h-full w-full object-cover" alt="" />
                {/if}
              </div>
              <p class="mt-2 truncate text-xs font-bold">{album.title}</p>
              <p class="truncate text-[11px] text-white/40">{album.song_count} {album.song_count === 1 ? 'song' : 'songs'}</p>
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>

{:else}
  <!-- Artist grid -->
  <div class="grid h-full grid-cols-[minmax(0,1fr)_24px]">
    <div class="scrollbar-none max-h-full overflow-auto pr-2"
      bind:this={artistListEl}
      bind:clientHeight={artistViewportHeight}
      bind:clientWidth={artistViewportWidth}
      on:scroll={updateArtistScroll}>
      {#if artistEntries.length}
        <div class="relative" style={`height: ${artistRowCount * ARTIST_ROW_HEIGHT}px;`}>
        {#each visibleArtists as artist, index (artist.name)}
          <button
            class="absolute flex min-w-0 items-center gap-3 border-b border-white/[0.04] px-2 py-3 text-left transition hover:bg-white/[0.035]"
            style={`width: ${artistItemWidth}px; height: ${ARTIST_ROW_HEIGHT - 4}px; transform: translate(${((artistVisibleStart + index) % artistColumnCount) * (artistItemWidth + ARTIST_GRID_GAP)}px, ${Math.floor((artistVisibleStart + index) / artistColumnCount) * ARTIST_ROW_HEIGHT}px);`}
            on:click={() => openArtist(artist.name)} on:contextmenu={(e) => openArtistMenu(e, artist)}>
            {#if artworkUrl(artist.artwork ?? artist.song_artwork)}
              <LazyArtwork rootClass="h-10 w-10 shrink-0 rounded-full overflow-hidden opacity-90" imageClass="h-full w-full object-cover"
                path={artist.artwork ? (artist.artwork_thumb ?? artist.artwork) : (artist.song_artwork_thumb ?? artist.song_artwork)} alt="" />
            {:else}
              <img src="/cover.png" class="h-10 w-10 shrink-0 rounded-full object-cover opacity-90" alt="" />
            {/if}
            <span class="min-w-0">
              <span class="block truncate text-sm font-semibold">{artist.name}</span>
              <span class="text-xs text-white/36">{artist.song_count} {artist.song_count === 1 ? 'song' : 'songs'}</span>
            </span>
          </button>
        {/each}
        </div>
      {:else}
        <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
          <p class="text-sm font-bold uppercase text-white/34">No artists found</p>
          <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such artist found :(</h2>
          <p class="mt-3 text-sm leading-6 text-white/48">Try another artist name.</p>
        </div>
      {/if}
    </div>
    <AlphabetRail onJump={jumpToLetter} />
  </div>
{/if}

{#if artistContextMenu}
  <div role="menu" tabindex="-1"
    class="fixed z-50 w-52 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
    style={`left: ${artistContextMenu.x}px; top: ${artistContextMenu.y}px;`}
    on:click|stopPropagation on:keydown|stopPropagation>
    <div class="border-b border-white/[0.06] px-3 py-2">
      <p class="truncate text-xs font-bold text-white">{artistContextMenu.artist.name}</p>
    </div>
    <button role="menuitem" class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white"
      on:click={fetchContextArtistImage}>
      Fetch Artwork Online
    </button>
  </div>
{/if}
