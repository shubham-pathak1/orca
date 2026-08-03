<script lang="ts">
  import { artworkUrl } from '../tauri';
  import { formatDuration, formatTotalDuration } from '../format';
  import type { LocalSong, GenreEntry } from '../types';
  import LazyArtwork from './LazyArtwork.svelte';

  export let genres: GenreEntry[] = [];
  export let songs: LocalSong[] = [];
  export let query = '';
  export let currentPath: string | null = null;
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};

  // Exported so LibraryView can toggle header / height
  export let isInDetail = false;

  let selectedGenreName: string | null = null;
  let detailQuery = '';
  let genreListEl: HTMLDivElement;
  let genreScrollTop = 0;
  let genreViewportHeight = 0;
  let genreViewportWidth = 0;

  const GRID_MIN_COLUMN_WIDTH = 140;
  const GRID_GAP = 16;
  const OVERSCAN_ROWS = 3;

  $: isInDetail = Boolean(selectedGenreName);

  $: genreEntries = genres.filter((g) =>
    !query || g.name.toLowerCase().includes(query.trim().toLowerCase())
  );
  $: selectedGenre = selectedGenreName
    ? genres.find((g) => g.name === selectedGenreName) ?? null
    : null;
  $: selectedGenreSongs = selectedGenreName
    ? songs
        .filter((s) => s.genre === selectedGenreName)
        .sort((a, b) => a.title.localeCompare(b.title))
    : [];
  $: selectedGenreVisibleSongs = filterDetailSongs(selectedGenreSongs, detailQuery);
  $: genreColumnCount = Math.max(1, Math.floor((genreViewportWidth + GRID_GAP) / (GRID_MIN_COLUMN_WIDTH + GRID_GAP)));
  $: genreItemWidth = Math.max(
    GRID_MIN_COLUMN_WIDTH,
    (genreViewportWidth - GRID_GAP * (genreColumnCount - 1)) / genreColumnCount
  );
  $: genreRowHeight = genreItemWidth + GRID_GAP;
  $: genreRowCount = Math.ceil(genreEntries.length / genreColumnCount);
  $: genreVisibleRowStart = Math.max(0, Math.floor(genreScrollTop / genreRowHeight) - OVERSCAN_ROWS);
  $: genreVisibleRowEnd = Math.min(
    genreRowCount,
    Math.ceil((genreScrollTop + genreViewportHeight) / genreRowHeight) + OVERSCAN_ROWS
  );
  $: genreVisibleStart = genreVisibleRowStart * genreColumnCount;
  $: genreVisibleEnd = Math.min(genreEntries.length, genreVisibleRowEnd * genreColumnCount);
  $: visibleGenres = genreEntries.slice(genreVisibleStart, genreVisibleEnd);
  $: {
    genreEntries;
    genreScrollTop = 0;
    if (genreListEl) genreListEl.scrollTop = 0;
  }

  function filterDetailSongs(sourceSongs: LocalSong[], searchQuery: string) {
    const needle = searchQuery.trim().toLowerCase();
    if (!needle) return sourceSongs;
    return sourceSongs.filter((s) =>
      [s.title, s.artist, s.album].some((v) => v.toLowerCase().includes(needle))
    );
  }

  function rowArtwork(song: LocalSong): string | null {
    return song.artwork_thumb ?? song.artwork_preview ?? null;
  }

  function openGenre(name: string) {
    selectedGenreName = name;
    detailQuery = '';
  }

  function closeGenre() {
    selectedGenreName = null;
    detailQuery = '';
  }

  function playFirstSong(sourceSongs: LocalSong[]) {
    const first = sourceSongs[0];
    if (first) onChooseSong(first, sourceSongs);
  }

  function updateGenreScroll(event: Event) {
    genreScrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }
</script>

{#if selectedGenre}
  <!-- Genre detail -->
  <div class="scrollbar-none h-full overflow-auto">
    <div class="relative mb-8 overflow-hidden rounded-md px-5 pb-6 pt-5">
      <div class="pointer-events-none absolute inset-0 transform-gpu bg-cover bg-center opacity-20 blur-3xl"
        style={`background-image: ${artworkUrl(selectedGenre.song_artwork) ? `url("${artworkUrl(selectedGenre.song_artwork)}")` : 'none'}`}></div>
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.05] via-transparent to-black/30"></div>
      <div class="relative mb-5 flex items-center justify-between gap-4">
        <button class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
          type="button" title="Back" aria-label="Back" on:click={closeGenre}>
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
        <label class="w-full max-w-xl">
          <span class="sr-only">Search songs in genre</span>
          <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
            bind:value={detailQuery} placeholder="Search {selectedGenre.name}..." />
        </label>
      </div>
      <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
        <div class="relative aspect-square w-[148px] shrink-0 overflow-hidden rounded-md bg-white/8 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
          {#if artworkUrl(selectedGenre.song_artwork)}
            <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedGenre.song_artwork} alt="" />
          {:else}
            <img src="/cover.png" class="h-full w-full object-cover" alt="" />
          {/if}
        </div>
        <div class="min-w-0">
          <h2 class="truncate text-6xl font-black leading-normal max-xl:text-5xl">{selectedGenre.name}</h2>
          <p class="mt-3 flex items-center gap-1.5 text-sm text-white/62">
            <span>{selectedGenre.song_count} {selectedGenre.song_count === 1 ? 'song' : 'songs'}</span>
            <span class="text-[6px] opacity-40">&#9679;</span>
            <span>{formatTotalDuration(selectedGenreSongs.reduce((acc, song) => acc + (song.duration || 0), 0))}</span>
          </p>
          <div class="mt-5 flex items-center gap-2">
            <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105"
              title="Play genre" on:click={() => playFirstSong(selectedGenreVisibleSongs)}>
              <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="grid h-8 grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36 max-lg:grid-cols-[40px_minmax(180px,1fr)_72px]">
      <span>#</span><span>Title</span><span class="max-lg:hidden">Artist</span><span class="text-right">Time</span>
    </div>
    {#each selectedGenreVisibleSongs as song, index}
      <button class={`grid min-h-11 w-full grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition max-lg:grid-cols-[40px_minmax(180px,1fr)_72px] ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.045]'}`}
        on:click={() => onChooseSong(song, selectedGenreVisibleSongs)}
        on:contextmenu={(e) => onOpenSongMenu(e, song)}>
        <span class="text-sm text-white/36">{index + 1}</span>
        <span class="flex min-w-0 items-center gap-2">
          {#if artworkUrl(song.artwork)}
            <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
          {:else}
            <img src="/cover.png" class="h-8 w-8 shrink-0 rounded-sm object-cover" alt="" />
          {/if}
          <span class="min-w-0">
            <span class="block truncate text-sm font-semibold text-white">{song.title}</span>
            <span class="block truncate text-xs text-white/36">{song.album}</span>
          </span>
        </span>
        <span class="truncate text-xs text-white/42 max-lg:hidden">{song.artist}</span>
        <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
      </button>
    {/each}
    {#if !selectedGenreVisibleSongs.length}
      <div class="mx-auto flex min-h-[220px] max-w-xl flex-col items-center justify-center px-2 text-center">
        <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
        <h2 class="mt-3 text-3xl font-black tracking-normal">Oops, no songs in {selectedGenre.name} match :(</h2>
        <p class="mt-3 text-sm leading-6 text-white/48">Try a different search inside this genre.</p>
      </div>
    {/if}
  </div>

{:else}
  <!-- Genre grid -->
  <div class="scrollbar-none max-h-full overflow-auto pr-2"
    bind:this={genreListEl}
    bind:clientHeight={genreViewportHeight}
    bind:clientWidth={genreViewportWidth}
    on:scroll={updateGenreScroll}>
    {#if genreEntries.length}
      <div class="relative" style={`height: ${genreRowCount * genreRowHeight}px;`}>
      {#each visibleGenres as genre, index (genre.name)}
        <button class="group absolute text-left transition"
          style={`width: ${genreItemWidth}px; transform: translate(${((genreVisibleStart + index) % genreColumnCount) * (genreItemWidth + GRID_GAP)}px, ${Math.floor((genreVisibleStart + index) / genreColumnCount) * genreRowHeight}px);`}
          on:click={() => openGenre(genre.name)}>
          <div class="relative flex aspect-square flex-col items-center justify-center overflow-hidden rounded-lg bg-black/80 shadow-[0_4px_20px_rgba(0,0,0,0.3)] transition group-hover:shadow-[0_8px_32px_rgba(0,0,0,0.4)]">
            {#if artworkUrl(genre.song_artwork)}
              <LazyArtwork rootClass="absolute inset-0" imageClass="h-full w-full object-cover opacity-40 transition duration-300 group-hover:opacity-20" path={genre.song_artwork} alt="" />
            {:else}
              <img src="/cover.png" class="absolute inset-0 h-full w-full object-cover opacity-40 transition duration-300 group-hover:opacity-20" alt="" />
            {/if}
            <!-- Centered genre text overlay -->
            <div class="relative z-10 flex flex-col items-center justify-center p-3 text-center">
              <span class="block w-full break-words text-lg font-black leading-tight text-white drop-shadow-lg">{genre.name}</span>
              <span class="mt-1.5 block text-[10px] font-bold uppercase tracking-widest text-white/60 drop-shadow-md">{genre.song_count} {genre.song_count === 1 ? 'song' : 'songs'}</span>
            </div>
          </div>
        </button>
      {/each}
      </div>
    {:else}
      <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
        <p class="text-sm font-bold uppercase text-white/34">No genres</p>
        <h2 class="mt-3 text-4xl font-black tracking-normal">No genre tags found.</h2>
        <p class="mt-3 text-sm leading-6 text-white/48">Add genre metadata to your music files and rescan your library.</p>
      </div>
    {/if}
  </div>
{/if}
