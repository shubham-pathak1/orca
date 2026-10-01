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
  export let onAddSongsToQueue: (songs: LocalSong[]) => void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};

  // Exported so LibraryView can toggle header / height
  export let isInDetail = false;

  let selectedGenreName: string | null = null;
  let detailQuery = '';
  let genreListEl: HTMLDivElement;
  let genreScrollTop = 0;
  let genreViewportHeight = 0;
  let genreViewportWidth = 0;

  const GRID_MIN_COLUMN_WIDTH = 220;
  const GRID_GAP = 16;
  const OVERSCAN_ROWS = 3;
  const GENRE_TILE_RATIO = 0.75;

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
  $: genreItemHeight = genreItemWidth * GENRE_TILE_RATIO;
  $: genreRowHeight = genreItemHeight + GRID_GAP;
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

  function genreArtworkTiles(genre: GenreEntry): string[] {
    const paths = [
      ...new Set(
        songs
          .filter((song) => song.genre === genre.name)
          .map((song) => song.artwork_preview ?? song.artwork_thumb ?? song.artwork)
          .filter((path): path is string => Boolean(path))
      ),
    ];

    if (!paths.length && genre.song_artwork) paths.push(genre.song_artwork);
    return paths.slice(0, 4);
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
            <button class="grid h-11 w-11 place-items-center rounded-full border border-white/12 text-white/72 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
              type="button" title="Add genre to queue" aria-label="Add genre to queue" on:click={() => onAddSongsToQueue(selectedGenreSongs)}>
              <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M4 6h11" /><path d="M4 12h11" /><path d="M4 18h7" /><path d="m18 9 3 3-3 3" /><path d="M15 12h6" />
              </svg>
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
        <button class="group absolute overflow-hidden rounded-md bg-white/[0.06] text-left shadow-[0_4px_20px_rgba(0,0,0,0.24)] transition hover:-translate-y-0.5 hover:shadow-[0_12px_28px_rgba(0,0,0,0.4)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]"
          style={`width: ${genreItemWidth}px; height: ${genreItemHeight}px; transform: translate(${((genreVisibleStart + index) % genreColumnCount) * (genreItemWidth + GRID_GAP)}px, ${Math.floor((genreVisibleStart + index) / genreColumnCount) * genreRowHeight}px);`}
          on:click={() => openGenre(genre.name)}>
          <div class={`genre-collage genre-collage-${Math.min(genreArtworkTiles(genre).length, 4)} h-full w-full bg-black/30 transition duration-300 group-hover:scale-[1.025]`}>
            {#each genreArtworkTiles(genre) as artworkPath}
              <LazyArtwork rootClass="min-h-0 overflow-hidden bg-white/[0.05]" imageClass="h-full w-full object-cover" path={artworkPath} alt="" />
            {/each}
          </div>
          <div class="pointer-events-none absolute inset-x-0 bottom-0 h-3/5 bg-gradient-to-t from-black/90 via-black/45 to-transparent"></div>
          <div class="pointer-events-none absolute inset-x-0 bottom-0 p-3">
            <span class="block truncate text-base font-bold leading-5 text-white drop-shadow">{genre.name}</span>
            <span class="mt-1 block text-xs text-white/65">{genre.song_count} {genre.song_count === 1 ? 'song' : 'songs'}</span>
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

<style>
  .genre-collage {
    display: grid;
    gap: 1px;
  }

  .genre-collage-1 {
    grid-template-columns: 1fr;
  }

  .genre-collage-2 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .genre-collage-3,
  .genre-collage-4 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-template-rows: repeat(2, minmax(0, 1fr));
  }

  .genre-collage-3 > :first-child {
    grid-row: span 2;
  }

  .genre-collage-0 {
    background: linear-gradient(135deg, rgb(255 255 255 / 0.1), rgb(255 255 255 / 0.025));
  }
</style>
