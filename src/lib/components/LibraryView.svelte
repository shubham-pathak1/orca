<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatDuration, formatQuality } from '../format';
  import type { LocalSong, Playlist, ArtistEntry, AlbumEntry } from '../types';
  import type { ActiveView } from '../navigation';
  import AlphabetRail from './AlphabetRail.svelte';
  import LazyArtwork from './LazyArtwork.svelte';
  import SettingsView from './SettingsView.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  export let activeView: ActiveView = 'songs';
  export let songs: LocalSong[] = [];
  export let playlists: Playlist[] = [];
  export let filteredSongs: LocalSong[] = [];
  export let artists: ArtistEntry[] = [];
  export let albums: AlbumEntry[] = [];
  export let query = '';
  export let selectedPath: string | null = null;
  export let currentPath: string | null = null;
  export let artistCount = 0;
  export let albumCount = 0;
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onCreatePlaylist: (name: string) => Promise<void> | void = () => {};
  export let onAddSongToPlaylist: (playlistId: number, song: LocalSong) => Promise<void> | void = () => {};
  export let onLoadPlaylistSongIds: (playlistId: number) => Promise<number[]> = async () => [];
  export let onRenamePlaylist: (playlistId: number, name: string) => Promise<void> | void = () => {};
  export let onDeletePlaylist: (playlistId: number) => Promise<void> | void = () => {};
  export let onChoosePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onRemovePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onRemoveSongFromPlaylist: (playlistId: number, song: LocalSong) => Promise<void> | void = () => {};
  export let onEditSong: (song: LocalSong) => void = () => {};
  export let playerPlacement: 'right' | 'bottom' = 'right';
  export let onPlayerPlacementChange: (placement: 'right' | 'bottom') => void = () => {};
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
  export let onSeekbarStyleChange: (style: 'standard' | 'waveform') => void = () => {};
  export let scanRoots: string[] = [];
  export let isScanning = false;
  export let onRemoveScanRoot: (root: string) => Promise<void> | void = () => {};
  export let dynamicCoverAccent = true;
  export let onDynamicCoverAccentChange: (enabled: boolean) => void = () => {};
  export let blurredBackground = true;
  export let onBlurredBackgroundChange: (enabled: boolean) => void = () => {};
  export let fontFamily = 'Plus Jakarta Sans';
  export let onFontFamilyChange: (font: string) => void = () => {};
  export let fontSizePercent = 100;
  export let onFontSizePercentChange: (size: number) => void = () => {};
  export let showQualityInfo = true;
  export let onShowQualityInfoChange: (enabled: boolean) => void = () => {};
  export let gaplessPlayback = true;
  export let onGaplessPlaybackChange: (enabled: boolean) => void = () => {};
  export let theme: 'default' = 'default';
  export let onThemeChange: (theme: 'default') => void = () => {};
  export let status = 'Ready';

  let songListEl: HTMLDivElement;
  let artistListEl: HTMLDivElement;
  let albumListEl: HTMLDivElement;
  let settingsQuery = '';
  let playlistQuery = '';
  let detailQuery = '';
  let newPlaylistName = '';
  let isCreatingPlaylist = false;
  let contextMenu: { x: number; y: number; song: LocalSong } | null = null;
  let playlistContextMenu: { x: number; y: number; playlist: Playlist } | null = null;
  let selectedPlaylistId: number | null = null;
  let selectedPlaylistSongIds: number[] = [];
  let editingPlaylistName = '';
  let playlistNameInput: HTMLInputElement;
  let isEditingPlaylistName = false;
  let isLoadingPlaylist = false;
  let isRenamingPlaylist = false;
  let showDeletePlaylistConfirm = false;
  let playlistToDelete: Playlist | null = null;
  let selectedArtistName: string | null = null;
  let selectedAlbumKey: string | null = null;
  let songLayout: 'list' | 'grid' = 'list';
  let sortKey: 'title' | 'artist' | 'album' = 'title';
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

  $: artistEntries = artists.filter((artist) =>
    !query || artist.name.toLowerCase().includes(query.trim().toLowerCase())
  );
  $: albumEntries = albums.filter((album) =>
    !query ||
    album.title.toLowerCase().includes(query.trim().toLowerCase()) ||
    album.artist.toLowerCase().includes(query.trim().toLowerCase())
  );
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
  $: filteredPlaylists = playlists.filter((playlist) =>
    playlist.name.toLowerCase().includes(playlistQuery.trim().toLowerCase())
  );
  $: selectedPlaylist = selectedPlaylistId
    ? playlists.find((playlist) => playlist.id === selectedPlaylistId) ?? null
    : null;
  $: selectedPlaylistSongs = selectedPlaylistSongIds
    .map((songId) => songs.find((song) => song.id === songId))
    .filter((song): song is LocalSong => Boolean(song));
  $: selectedPlaylistVisibleSongs = filterDetailSongs(selectedPlaylistSongs, detailQuery);
  $: selectedPlaylistArtwork = selectedPlaylist?.cover_path ?? selectedPlaylistSongs.find((song) => previewArtwork(song))?.artwork_preview ?? null;
  $: pageTitle = activeView === 'settings'
    ? 'Settings'
    : activeView === 'playlists'
      ? 'Playlists'
      : activeView === 'artists'
        ? 'Artists'
        : activeView === 'albums'
          ? 'Albums'
          : 'Main Library';
  $: pageSubtitle = activeView === 'settings'
    ? 'Tune Orca for the way you listen'
    : activeView === 'playlists'
      ? `${playlists.length} ${playlists.length === 1 ? 'playlist' : 'playlists'}`
      : activeView === 'artists'
        ? `${artistCount} ${artistCount === 1 ? 'artist' : 'artists'}`
        : activeView === 'albums'
          ? `${albumCount} ${albumCount === 1 ? 'album' : 'albums'}`
          : `${songs.length} songs / ${artistCount} artists / ${albumCount} albums`;
  $: selectedArtistSongs = selectedArtistName
    ? songs.filter((song) => song.artist === selectedArtistName).sort((a, b) => a.title.localeCompare(b.title))
    : [];
  $: selectedArtistVisibleSongs = filterDetailSongs(selectedArtistSongs, detailQuery);
  $: selectedArtist = selectedArtistName
    ? {
        name: selectedArtistName,
        songs: selectedArtistSongs,
        artwork: selectedArtistSongs.find((song) => previewArtwork(song))?.artwork_preview ?? null,
        albums: albums.filter((album) => album.artist === selectedArtistName)
      }
    : null;
  $: selectedAlbum = selectedAlbumKey ? albumEntries.find((album) => album.key === selectedAlbumKey) ?? null : null;
  $: selectedAlbumSongs = selectedAlbum
    ? songs
        .filter((song) => `${song.album_artist}:${song.album}` === selectedAlbum.key)
        .sort((a, b) => (a.track_number ?? 999) - (b.track_number ?? 999) || a.title.localeCompare(b.title))
    : [];
  $: selectedAlbumVisibleSongs = filterDetailSongs(selectedAlbumSongs, detailQuery);
  $: detailMode = Boolean(selectedPlaylist || selectedArtist || selectedAlbum);
  $: detailSearchPlaceholder = selectedPlaylist
    ? 'Search songs in this playlist...'
    : selectedArtist
      ? `Search songs by ${selectedArtist.name}...`
      : selectedAlbum
        ? `Search songs in ${selectedAlbum.title}...`
        : '';

  $: if (!detailMode && detailQuery) {
    detailQuery = '';
  }

  $: if (activeView !== 'artists') {
    selectedArtistName = null;
  }

  $: if (activeView !== 'albums') {
    selectedAlbumKey = null;
  }

  $: if (activeView !== 'playlists') {
    selectedPlaylistId = null;
    selectedPlaylistSongIds = [];
  }




  function compareSongs(a: LocalSong, b: LocalSong, key: 'title' | 'artist' | 'album') {
    const primary = a[key].localeCompare(b[key], undefined, { sensitivity: 'base' });
    if (primary !== 0) {
      return primary;
    }

    return a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });
  }

  function filterDetailSongs(sourceSongs: LocalSong[], searchQuery: string) {
    const needle = searchQuery.trim().toLowerCase();
    if (!needle) {
      return sourceSongs;
    }

    return sourceSongs.filter((song) =>
      [song.title, song.artist, song.album].some((value) => value.toLowerCase().includes(needle))
    );
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

  function closeFloatingUi() {
    contextMenu = null;
    playlistContextMenu = null;
    sortMenuOpen = false;
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeFloatingUi();
    }
  }

  let savedArtistScrollTop = 0;
  let savedAlbumScrollTop = 0;

  function openArtist(name: string) {
    if (artistListEl) {
      savedArtistScrollTop = artistListEl.scrollTop;
    }
    selectedArtistName = name;
    query = '';
    detailQuery = '';
  }

  function openAlbum(key: string) {
    if (albumListEl) {
      savedAlbumScrollTop = albumListEl.scrollTop;
    }
    selectedAlbumKey = key;
    query = '';
    detailQuery = '';
  }

  function playFirstSong(sourceSongs: LocalSong[]) {
    const firstSong = sourceSongs[0];
    if (firstSong) {
      onChooseSong(firstSong, sourceSongs);
    }
  }

  async function createPlaylistFromInput() {
    const name = newPlaylistName.trim();
    if (!name || isCreatingPlaylist) {
      return;
    }

    isCreatingPlaylist = true;
    try {
      await onCreatePlaylist(name);
      newPlaylistName = '';
    } finally {
      isCreatingPlaylist = false;
    }
  }

  async function openPlaylist(playlist: Playlist) {
    selectedPlaylistId = playlist.id;
    editingPlaylistName = playlist.name;
    detailQuery = '';
    isLoadingPlaylist = true;
    try {
      selectedPlaylistSongIds = await onLoadPlaylistSongIds(playlist.id);
    } finally {
      isLoadingPlaylist = false;
    }
  }

  function closePlaylist() {
    selectedPlaylistId = null;
    selectedPlaylistSongIds = [];
    editingPlaylistName = '';
    detailQuery = '';
  }

  function closeArtist() {
    selectedArtistName = null;
    detailQuery = '';
    void tick().then(() => {
      if (artistListEl) {
        artistListEl.scrollTop = savedArtistScrollTop;
      }
    });
  }

  function closeAlbum() {
    selectedAlbumKey = null;
    detailQuery = '';
    void tick().then(() => {
      if (albumListEl) {
        albumListEl.scrollTop = savedAlbumScrollTop;
      }
    });
  }

  async function savePlaylistName() {
    if (!selectedPlaylist || isRenamingPlaylist) {
      return;
    }

    const name = editingPlaylistName.trim();
    if (!name || name === selectedPlaylist.name) {
      editingPlaylistName = selectedPlaylist.name;
      isEditingPlaylistName = false;
      return;
    }

    isRenamingPlaylist = true;
    try {
      await onRenamePlaylist(selectedPlaylist.id, name);
    } finally {
      isRenamingPlaylist = false;
      isEditingPlaylistName = false;
    }
  }

  async function editPlaylistName() {
    isEditingPlaylistName = true;
    await tick();
    playlistNameInput?.focus();
    playlistNameInput?.select();
  }

  function handlePlaylistNameKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      playlistNameInput?.blur();
    }

    if (event.key === 'Escape') {
      editingPlaylistName = selectedPlaylist?.name ?? '';
      isEditingPlaylistName = false;
    }
  }

  async function deleteSelectedPlaylist() {
    if (!selectedPlaylist) return;
    playlistToDelete = selectedPlaylist;
    showDeletePlaylistConfirm = true;
  }

  async function chooseSelectedPlaylistCover() {
    if (selectedPlaylist) {
      await onChoosePlaylistCover(selectedPlaylist.id);
    }
  }

  async function removeSelectedPlaylistCover() {
    if (selectedPlaylist) {
      await onRemovePlaylistCover(selectedPlaylist.id);
    }
  }

  async function removePlaylistSong(song: LocalSong) {
    if (!selectedPlaylist || song.id === null) {
      return;
    }

    await onRemoveSongFromPlaylist(selectedPlaylist.id, song);
    selectedPlaylistSongIds = selectedPlaylistSongIds.filter((songId) => songId !== song.id);
  }

  function openSongMenu(event: MouseEvent, song: LocalSong) {
    event.preventDefault();
    contextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 230),
      y: Math.min(event.clientY, window.innerHeight - 230),
      song
    };
  }

  function openPlaylistMenu(event: MouseEvent, playlist: Playlist) {
    event.preventDefault();
    event.stopPropagation();
    playlistContextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 200),
      y: Math.min(event.clientY, window.innerHeight - 130),
      playlist
    };
  }

  async function renameContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    playlistContextMenu = null;
    if (!playlist) return;
    const name = window.prompt('Rename playlist', playlist.name);
    if (!name || name.trim() === playlist.name) return;
    await onRenamePlaylist(playlist.id, name.trim());
    if (selectedPlaylist?.id === playlist.id) {
      editingPlaylistName = name.trim();
    }
  }

  async function deleteContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    playlistContextMenu = null;
    if (!playlist) return;
    playlistToDelete = playlist;
    showDeletePlaylistConfirm = true;
  }

  async function confirmDeletePlaylist() {
    const playlist = playlistToDelete;
    showDeletePlaylistConfirm = false;
    playlistToDelete = null;
    if (!playlist) return;
    if (selectedPlaylistId === playlist.id) closePlaylist();
    await onDeletePlaylist(playlist.id);
  }

  function cancelDeletePlaylist() {
    showDeletePlaylistConfirm = false;
    playlistToDelete = null;
  }

  async function addCoverContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    playlistContextMenu = null;
    if (!playlist) return;
    await onChoosePlaylistCover(playlist.id);
  }

  function editContextSong() {
    if (!contextMenu) {
      return;
    }

    const song = contextMenu.song;
    contextMenu = null;
    onEditSong(song);
  }

  async function addContextSongToPlaylist(playlistId: number) {
    if (!contextMenu) {
      return;
    }

    const song = contextMenu.song;
    contextMenu = null;
    await onAddSongToPlaylist(playlistId, song);
  }

  async function removeContextSongFromPlaylist() {
    if (!contextMenu || !selectedPlaylist || contextMenu.song.id === null) {
      return;
    }

    const song = contextMenu.song;
    contextMenu = null;
    await onRemoveSongFromPlaylist(selectedPlaylist.id, song);
    selectedPlaylistSongIds = selectedPlaylistSongIds.filter((songId) => songId !== song.id);
  }

  function initialFromText(value: string): string {
    const first = value.trim().charAt(0).toUpperCase();
    return /^[A-Z]$/.test(first) ? first : '#';
  }

  function jumpToLetter(container: HTMLDivElement | undefined, letter: string) {
    if (!container) {
      return;
    }

    const letters = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('')];
    const startIndex = letters.indexOf(letter);
    const searchOrder = startIndex >= 0 ? letters.slice(startIndex) : [letter];

    const target = searchOrder
      .map((candidate) => container.querySelector(`[data-letter="${candidate}"]`) as HTMLElement | null)
      .find(Boolean);

    if (!target) {
      return;
    }

    const containerRect = container.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const top = container.scrollTop + targetRect.top - containerRect.top;
    container.scrollTo({ top, behavior: 'smooth' });
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
</script>

<svelte:window on:click={closeFloatingUi} on:keydown={handleGlobalKeydown} />

<section class="min-h-0 bg-black/42 px-5 py-4">
  {#if !detailMode}
  <div class={`mb-4 grid items-center gap-4 max-lg:grid-cols-1 ${activeView === 'settings' || activeView === 'playlists' ? 'grid-cols-[minmax(200px,1fr)_minmax(260px,420px)]' : activeView === 'songs' ? 'grid-cols-[minmax(200px,1fr)_minmax(220px,380px)_84px_140px]' : 'grid-cols-[minmax(200px,1fr)_minmax(220px,300px)_140px]'}`}>
    <div>
        <h1 class="text-2xl font-bold tracking-normal">{pageTitle}</h1>
        <p class="mt-1 text-xs text-white/42">{pageSubtitle}</p>
        
      </div>
    {#if activeView === 'settings'}
      <label>
        <span class="sr-only">Search settings</span>
        <input
          class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
          bind:value={settingsQuery}
          placeholder="Search settings..."
        />
      </label>
    {:else if activeView === 'playlists'}
      <label>
        <span class="sr-only">Search playlists</span>
        <input
          class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
          bind:value={playlistQuery}
          placeholder="Search playlists..."
        />
      </label>
    {:else}
      <label>
        <span class="sr-only">Search library</span>
        <input
          class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
          bind:value={query}
          placeholder="Search library..."
        />
      </label>
      {#if activeView === 'songs'}
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
      {/if}
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
    {/if}
  </div>
  {/if}

  <div class={`${detailMode ? 'h-full' : 'h-[calc(100%-72px)]'} min-h-0 overflow-hidden`}>
    {#if activeView === 'settings'}
      <SettingsView
        {playerPlacement}
        {onPlayerPlacementChange}
        {seekbarStyle}
        {onSeekbarStyleChange}
        {scanRoots}
        {isScanning}
        {onRemoveScanRoot}
        {dynamicCoverAccent}
        {onDynamicCoverAccentChange}
        {blurredBackground}
        {onBlurredBackgroundChange}
        {fontFamily}
        {onFontFamilyChange}
        {fontSizePercent}
        {onFontSizePercentChange}
        {showQualityInfo}
        {onShowQualityInfoChange}
        {gaplessPlayback}
        {onGaplessPlaybackChange}
        {theme}
        {onThemeChange}
      />
    {:else if activeView === 'playlists'}
      <div class="scrollbar-none h-full overflow-auto pr-2">
        {#if selectedPlaylist}
          <div class="relative mb-7 overflow-hidden rounded-md px-5 pb-6 pt-5">
            <div class="pointer-events-none absolute inset-0 bg-cover bg-center opacity-20 blur-3xl" style={`background-image: ${artworkUrl(selectedPlaylistArtwork) ? `url("${artworkUrl(selectedPlaylistArtwork)}")` : 'none'}`}></div>
            <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.06] via-transparent to-black/30"></div>
            <div class="relative mb-5 flex items-center justify-between gap-4">
              <button
                class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
                type="button"
                title="Back"
                aria-label="Back"
                on:click={closePlaylist}
              >
                <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m15 18-6-6 6-6" />
                </svg>
              </button>
              <label class="w-full max-w-xl">
                <span class="sr-only">Search songs in playlist</span>
                <input
                  class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
                  bind:value={detailQuery}
                  placeholder={detailSearchPlaceholder}
                />
              </label>
            </div>
            <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
              <div class="group relative grid aspect-square w-[148px] shrink-0 place-items-center overflow-hidden rounded-md bg-white/[0.07] text-5xl font-black text-white/30 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
                {#if artworkUrl(selectedPlaylistArtwork)}
                  <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedPlaylistArtwork} alt="" />
                {:else}
                  {selectedPlaylist.name.charAt(0).toUpperCase()}
                {/if}
                <div class="absolute inset-x-2 bottom-2 flex justify-end gap-2 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100">
                  <button class="grid h-8 w-8 place-items-center rounded-full bg-white text-black shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md" type="button" title="Change cover" aria-label="Change cover" on:click={chooseSelectedPlaylistCover}>
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M12 20h9" />
                      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
                    </svg>
                  </button>
                  <button class="grid h-8 w-8 place-items-center rounded-full bg-black text-white shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md" type="button" title="Remove cover" aria-label="Remove cover" on:click={removeSelectedPlaylistCover}>
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M3 6h18" />
                      <path d="M8 6V4h8v2" />
                      <path d="M19 6l-1 14H6L5 6" />
                      <path d="M10 11v5M14 11v5" />
                    </svg>
                  </button>
                </div>
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex max-w-3xl items-center gap-2">
                  {#if isEditingPlaylistName}
                  <input
                    class="h-14 min-w-0 flex-1 rounded-md border border-white/10 bg-white/[0.025] px-3 text-5xl font-black text-white outline-none focus:border-[color:var(--accent-mid)] max-xl:text-4xl"
                    bind:this={playlistNameInput}
                    bind:value={editingPlaylistName}
                    on:blur={savePlaylistName}
                    on:keydown={handlePlaylistNameKeydown}
                  />
                  {:else}
                  <button class="min-w-0 truncate text-left text-6xl font-black leading-tight text-white outline-none transition hover:text-white/80 focus-visible:ring-2 focus-visible:ring-white/24 max-xl:text-5xl" type="button" title="Edit playlist name" on:click={editPlaylistName}>
                    {selectedPlaylist.name}
                  </button>
                  {/if}
                </div>
                <p class="mt-2 text-sm text-white/58">{selectedPlaylist.song_count} {selectedPlaylist.song_count === 1 ? 'song' : 'songs'}</p>
                <div class="mt-5 flex flex-wrap items-center gap-2">
                  <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105" title="Play playlist" on:click={() => playFirstSong(selectedPlaylistVisibleSongs)}>
                    <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M8 5v14l11-7z" />
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </div>

          {#if isLoadingPlaylist}
            <p class="text-sm text-white/42">Loading playlist...</p>
          {:else if selectedPlaylistVisibleSongs.length}
            <div class="grid h-8 grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36 max-lg:grid-cols-[40px_minmax(180px,1fr)_72px]">
              <span>#</span>
              <span>Title</span>
              <span class="max-lg:hidden">Artist</span>
              <span class="text-right">Time</span>
            </div>
            {#each selectedPlaylistVisibleSongs as song, index}
              <div class={`grid min-h-11 w-full grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition max-lg:grid-cols-[40px_minmax(180px,1fr)_72px] ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.035]'}`} on:contextmenu={(event) => openSongMenu(event, song)} role="presentation">
                <button class="text-left text-sm text-white/36" on:click={() => onChooseSong(song, selectedPlaylistVisibleSongs)}>{index + 1}</button>
                <button class="flex min-w-0 items-center gap-2 text-left" on:click={() => onChooseSong(song, selectedPlaylistVisibleSongs)}>
                  {#if artworkUrl(song.artwork)}
                    <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
                  {:else}
                    <span class="h-8 w-8 shrink-0 rounded-sm bg-white/10"></span>
                  {/if}
                  <span class="min-w-0">
                    <span class="block truncate text-sm font-semibold text-white">{song.title}</span>
                    <span class="block truncate text-xs text-white/36">{song.album}</span>
                  </span>
                </button>
                <span class="truncate text-xs text-white/42 max-lg:hidden">{song.artist}</span>
                <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
              </div>
            {/each}
          {:else}
            <div class="mx-auto flex min-h-[260px] max-w-xl flex-col items-center justify-center text-center">
              <p class="text-sm font-bold uppercase text-white/34">{detailQuery.trim() ? 'No songs found' : 'Empty playlist'}</p>
              <h2 class="mt-3 text-3xl font-black tracking-normal">{detailQuery.trim() ? 'Oops, no songs in this playlist match :(' : 'Add songs from Library.'}</h2>
              <p class="mt-3 text-sm leading-6 text-white/48">{detailQuery.trim() ? 'Try a different search inside this playlist.' : 'Right-click any song in Library, then choose this playlist.'}</p>
            </div>
          {/if}
        {:else}
          <form class="mb-5 grid grid-cols-[minmax(180px,360px)_auto] items-center gap-3 max-md:grid-cols-1" on:submit|preventDefault={createPlaylistFromInput}>
            <input
              class="h-10 rounded-md border border-white/10 bg-white/[0.045] px-3 text-sm text-white outline-none placeholder:text-white focus:border-[color:var(--accent-mid)]"
              bind:value={newPlaylistName}
              placeholder="New playlist name"
            />
            <button
              class="h-10 rounded-md border border-white/14 px-4 text-sm font-bold text-white transition hover:bg-white/[0.08] disabled:opacity-40"
              disabled={!newPlaylistName.trim() || isCreatingPlaylist}
            >
              Create Playlist
            </button>
          </form>

          {#if filteredPlaylists.length}
            <div class="scrollbar-none grid max-h-[calc(100%-60px)] grid-cols-5 gap-x-6 overflow-auto pr-2 max-2xl:grid-cols-4 max-lg:grid-cols-3 max-md:grid-cols-2">
              {#each filteredPlaylists as playlist}
                <button class="flex min-w-0 items-center gap-3 border-b border-white/[0.04] px-2 py-4 text-left transition hover:bg-white/[0.035]" on:click={() => openPlaylist(playlist)} on:contextmenu={(event) => openPlaylistMenu(event, playlist)}>
                  <span class="grid h-11 w-11 shrink-0 place-items-center overflow-hidden rounded-sm bg-white/[0.07] text-xs font-black text-white/40">
                    {#if artworkUrl(playlist.cover_path)}
                      <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={playlist.cover_path} alt="" />
                    {:else}
                      {playlist.name.charAt(0).toUpperCase()}
                    {/if}
                  </span>
                  <span class="min-w-0">
                    <span class="block truncate text-sm font-bold text-white">{playlist.name}</span>
                    <span class="mt-1 block text-xs text-white/52">{playlist.song_count} {playlist.song_count === 1 ? 'song' : 'songs'}</span>
                  </span>
                </button>
              {/each}
            </div>
          {:else}
            <div class="mx-auto flex h-[calc(100%-60px)] max-w-xl flex-col items-center justify-center text-center">
              <p class="text-sm font-bold uppercase text-white/34">{playlistQuery.trim() ? 'No playlists found' : 'No playlists yet'}</p>
              <h2 class="mt-3 text-4xl font-black tracking-normal">{playlistQuery.trim() ? 'Oops, no such playlist found :(' : 'Build a queue worth keeping.'}</h2>
              <p class="mt-3 text-sm leading-6 text-white/48">{playlistQuery.trim() ? 'Try another playlist name.' : 'Create a playlist, then right-click songs in Library to add them.'}</p>
            </div>
          {/if}
        {/if}
      </div>
    {:else if activeView === 'songs'}
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
                on:contextmenu={(event) => openSongMenu(event, song)}
              >
                <span class="flex min-w-0 items-center gap-2">
                  {#if artworkUrl(song.artwork)}
                    <LazyArtwork rootClass="h-7 w-7 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
                  {:else}
                    <span class="h-7 w-7 shrink-0 rounded-sm bg-white/10"></span>
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
                on:contextmenu={(event) => openSongMenu(event, song)}
              >
                <span class={`relative block aspect-square overflow-hidden rounded-md bg-white/[0.07] ${song.path === currentPath ? 'ring-2 ring-[var(--accent)]' : ''}`}>
                  {#if artworkUrl(song.artwork)}
                    <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={previewArtwork(song)} alt="" />
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
    {:else if activeView === 'albums'}
      {#if selectedAlbum}
        <div class="scrollbar-none h-full overflow-auto">
          <div class="relative mb-8 overflow-hidden rounded-md px-5 pb-6 pt-5">
            <div class="pointer-events-none absolute inset-0 bg-cover bg-center opacity-20 blur-3xl" style={`background-image: ${artworkUrl(selectedAlbum.artwork) ? `url("${artworkUrl(selectedAlbum.artwork)}")` : 'none'}`}></div>
            <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.05] via-transparent to-black/30"></div>
            <div class="relative mb-5 flex items-center justify-between gap-4">
              <button
                class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
                type="button"
                title="Back"
                aria-label="Back"
                on:click={closeAlbum}
              >
                <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m15 18-6-6 6-6" />
                </svg>
              </button>
              <label class="w-full max-w-xl">
                <span class="sr-only">Search songs in album</span>
                <input
                  class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
                  bind:value={detailQuery}
                  placeholder={detailSearchPlaceholder}
                />
              </label>
            </div>
            <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
              <div class="aspect-square w-[148px] shrink-0 overflow-hidden rounded-md bg-white/8 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
                {#if artworkUrl(selectedAlbum.artwork)}
                  <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedAlbum.artwork} alt="" />
                {/if}
              </div>
              <div class="min-w-0">
                <h2 class="truncate text-6xl font-black max-xl:text-5xl">{selectedAlbum.title}</h2>
                <p class="mt-3 text-sm text-white/62">By {selectedAlbum.artist}</p>
                <p class="mt-1 text-xs text-white/42">{selectedAlbum.song_count} {selectedAlbum.song_count === 1 ? 'track' : 'tracks'} / {formatDuration(selectedAlbum.duration)}</p>
                <div class="mt-5 flex items-center gap-2">
                <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105" title="Play album" on:click={() => playFirstSong(selectedAlbumVisibleSongs)}>
                  <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                </button>
                </div>
              </div>
            </div>
          </div>

          <div class="grid grid-cols-[minmax(0,1fr)_340px] gap-8 max-2xl:grid-cols-1">
            <div>
              <div class="grid h-8 grid-cols-[48px_minmax(240px,1fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36">
                <span>#</span>
                <span>Title</span>
                <span class="text-right">Duration</span>
              </div>
              {#each selectedAlbumVisibleSongs as song, index}
                <button class={`grid min-h-11 w-full grid-cols-[48px_minmax(240px,1fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.045]'}`} on:click={() => onChooseSong(song, selectedAlbumVisibleSongs)} on:contextmenu={(event) => openSongMenu(event, song)}>
                  <span class="text-sm text-white/36">{song.track_number ?? index + 1}</span>
                  <span class="flex min-w-0 items-center gap-2">
                    {#if artworkUrl(song.artwork)}
                      <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
                    {:else}
                      <span class="h-8 w-8 shrink-0 rounded-sm bg-white/10"></span>
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
                <div class="mx-auto flex min-h-[220px] max-w-xl flex-col items-center justify-center text-center px-2">
                  <p class="text-sm font-bold uppercase text-white/34">No songs found</p>
                  <h2 class="mt-3 text-3xl font-black tracking-normal">Oops, no songs in this album match :(</h2>
                  <p class="mt-3 text-sm leading-6 text-white/48">Try a different search inside this album.</p>
                </div>
              {/if}
            </div>
            <div>
              <h3 class="mb-3 text-sm font-black">More albums from {selectedAlbum.artist}</h3>
              <div class="grid grid-cols-2 gap-3">
                {#each albumEntries.filter((album) => album.artist === selectedAlbum.artist && album.key !== selectedAlbum.key).slice(0, 6) as album}
                  <button class="min-w-0 rounded-md bg-white/[0.035] p-2 text-left transition hover:bg-white/[0.07]" on:click={() => openAlbum(album.key)}>
                    <div class="aspect-square overflow-hidden rounded bg-white/8">
                      {#if artworkUrl(album.artwork)}
                        <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
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
        <div class="grid h-full grid-cols-[minmax(0,1fr)_24px]">
          <div class="scrollbar-none grid max-h-full grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-3 overflow-auto pr-2" bind:this={albumListEl}>
            {#if albumEntries.length}
              {#each albumEntries as album}
              <button data-letter={initialFromText(album.title)} class="text-left opacity-82 transition hover:opacity-100" on:click={() => openAlbum(album.key)}>
                <div class="aspect-square overflow-hidden rounded-md bg-white/8">
                  {#if artworkUrl(album.artwork)}
                    <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
                  {/if}
                </div>
                <p class="mt-2 truncate text-sm font-semibold">{album.title}</p>
                <p class="truncate text-xs text-white/40">{album.artist}</p>
              </button>
              {/each}
            {:else}
              <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
                <p class="text-sm font-bold uppercase text-white/34">No albums found</p>
                <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such album found :(</h2>
                <p class="mt-3 text-sm leading-6 text-white/48">Try another album or artist name.</p>
              </div>
            {/if}
          </div>
          <AlphabetRail onJump={(letter) => jumpToLetter(albumListEl, letter)} />
        </div>
      {/if}
    {:else}
      {#if selectedArtist}
        <div class="scrollbar-none h-full overflow-auto">
          <div class="relative mb-8 overflow-hidden rounded-md px-5 pb-6 pt-5">
            <div class="pointer-events-none absolute inset-0 bg-cover bg-center opacity-20 blur-3xl" style={`background-image: ${artworkUrl(selectedArtist.artwork) ? `url("${artworkUrl(selectedArtist.artwork)}")` : 'none'}`}></div>
            <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.05] via-transparent to-black/30"></div>
            <div class="relative mb-5 flex items-center justify-between gap-4">
              <button
                class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
                type="button"
                title="Back"
                aria-label="Back"
                on:click={closeArtist}
              >
                <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m15 18-6-6 6-6" />
                </svg>
              </button>
              <label class="w-full max-w-xl">
                <span class="sr-only">Search songs by artist</span>
                <input
                  class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
                  bind:value={detailQuery}
                  placeholder={detailSearchPlaceholder}
                />
              </label>
            </div>
            <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
              <div class="aspect-square w-[148px] shrink-0 overflow-hidden rounded-full bg-white/8 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
                {#if artworkUrl(selectedArtist.artwork)}
                  <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedArtist.artwork} alt="" />
                {:else}
                  <span class="grid h-full w-full place-items-center text-5xl font-black text-white/28">{selectedArtist.name.charAt(0).toUpperCase()}</span>
                {/if}
              </div>
              <div class="min-w-0">
                <h2 class="truncate text-6xl font-black max-xl:text-5xl">{selectedArtist.name}</h2>
                <p class="mt-3 text-sm text-white/62">{selectedArtist.songs.length} {selectedArtist.songs.length === 1 ? 'song' : 'songs'} / {selectedArtist.albums.length} {selectedArtist.albums.length === 1 ? 'album' : 'albums'}</p>
                <div class="mt-5 flex items-center gap-2">
                <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105" title="Play artist" on:click={() => playFirstSong(selectedArtistVisibleSongs)}>
                  <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                </button>
                </div>
              </div>
            </div>
          </div>

          <div class="grid grid-cols-[minmax(0,1fr)_340px] gap-8 max-2xl:grid-cols-1">
            <div>
              <h3 class="mb-3 text-base font-black">Popular</h3>
              <div class="grid h-8 grid-cols-[48px_minmax(240px,1fr)_minmax(140px,0.6fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36">
                <span>#</span>
                <span>Title</span>
                <span class="max-lg:hidden">Album</span>
                <span class="text-right">Duration</span>
              </div>
              {#each selectedArtistVisibleSongs as song, index}
                <button class={`grid min-h-11 w-full grid-cols-[48px_minmax(240px,1fr)_minmax(140px,0.6fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.045]'}`} on:click={() => onChooseSong(song, selectedArtistVisibleSongs)} on:contextmenu={(event) => openSongMenu(event, song)}>
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
                <div class="mx-auto flex min-h-[220px] max-w-xl flex-col items-center justify-center text-center px-2">
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
                  <button class="min-w-0 rounded-md bg-white/[0.035] p-2 text-left transition hover:bg-white/[0.07]" on:click={() => { selectedAlbumKey = album.key; activeView = 'albums'; }}>
                    <div class="aspect-square overflow-hidden rounded bg-white/8">
                      {#if artworkUrl(album.artwork)}
                        <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={album.artwork} alt="" />
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
        <div class="grid h-full grid-cols-[minmax(0,1fr)_24px]">
          <div class="scrollbar-none grid max-h-full grid-cols-5 gap-x-3 overflow-auto pr-2 max-2xl:grid-cols-4 max-lg:grid-cols-3 max-md:grid-cols-2" bind:this={artistListEl}>
            {#if artistEntries.length}
              {#each artistEntries as artist}
              <button data-letter={initialFromText(artist.name)} class="flex min-w-0 items-center gap-3 border-b border-white/[0.04] px-2 py-3 text-left transition hover:bg-white/[0.035]" on:click={() => openArtist(artist.name)}>
                {#if artworkUrl(artist.artwork)}
                  <LazyArtwork rootClass="h-10 w-10 shrink-0 rounded-sm overflow-hidden opacity-90" imageClass="h-full w-full object-cover" path={artist.artwork} alt="" />
                {:else}
                  <span class="grid h-10 w-10 shrink-0 place-items-center rounded-sm bg-white/[0.06] text-xs font-bold text-white/32">
                    {artist.name.charAt(0).toUpperCase()}
                  </span>
                {/if}
                <span class="min-w-0">
                  <span class="block truncate text-sm font-semibold">{artist.name}</span>
                  <span class="text-xs text-white/36">{artist.song_count} {artist.song_count === 1 ? 'song' : 'songs'}</span>
                </span>
              </button>
              {/each}
            {:else}
              <div class="col-span-full mx-auto flex min-h-[320px] max-w-xl flex-col items-center justify-center text-center">
                <p class="text-sm font-bold uppercase text-white/34">No artists found</p>
                <h2 class="mt-3 text-4xl font-black tracking-normal">Oops, no such artist found :(</h2>
                <p class="mt-3 text-sm leading-6 text-white/48">Try another artist name.</p>
              </div>
            {/if}
          </div>
          <AlphabetRail onJump={(letter) => jumpToLetter(artistListEl, letter)} />
        </div>
      {/if}
    {/if}
  </div>

  {#if contextMenu}
    <div
      role="menu"
      tabindex="-1"
      class="fixed z-50 w-56 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
      style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px;`}
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <div class="border-b border-white/[0.06] px-3 py-2">
        <p class="truncate text-xs font-bold text-white">{contextMenu.song.title}</p>
        <p class="truncate text-[11px] text-white/42">{contextMenu.song.artist}</p>
      </div>
      <button role="menuitem" class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white" on:click={editContextSong}>
        Edit metadata
      </button>
      {#if selectedPlaylist}
        <button role="menuitem" class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-red-100/72 transition hover:bg-red-500/10 hover:text-red-100" on:click={removeContextSongFromPlaylist}>
          Remove from playlist
        </button>
      {/if}
      {#if playlists.length}
        <p class="border-t border-white/[0.06] px-3 pb-1 pt-2 text-[10px] font-bold uppercase text-white/32">Add to playlist</p>
        {#each playlists as playlist}
          <button role="menuitem" class="flex h-8 w-full items-center justify-between gap-3 px-3 text-left text-xs text-white/72 transition hover:bg-white/[0.08] hover:text-white" on:click={() => addContextSongToPlaylist(playlist.id)}>
            <span class="truncate">{playlist.name}</span>
            <span class="text-white/32">{playlist.song_count}</span>
          </button>
        {/each}
      {:else}
        <p class="px-3 py-3 text-xs leading-5 text-white/44">Create a playlist first, then right-click songs to add them.</p>
      {/if}
    </div>
  {/if}

  {#if playlistContextMenu}
    <div
      role="menu"
      tabindex="-1"
      class="fixed z-50 w-52 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
      style={`left: ${playlistContextMenu.x}px; top: ${playlistContextMenu.y}px;`}
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <div class="border-b border-white/[0.06] px-3 py-2">
        <p class="truncate text-xs font-bold text-white">{playlistContextMenu.playlist.name}</p>
        <p class="truncate text-[11px] text-white/42">{playlistContextMenu.playlist.song_count} {playlistContextMenu.playlist.song_count === 1 ? 'song' : 'songs'}</p>
      </div>
      <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white" on:click={renameContextPlaylist}>
        <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
        </svg>
        Rename
      </button>
      <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white" on:click={addCoverContextPlaylist}>
        <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <circle cx="8.5" cy="8.5" r="1.5" />
          <path d="m21 15-5-5L5 21" />
        </svg>
        Add Cover
      </button>
      <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-red-100/72 transition hover:bg-red-500/10 hover:text-red-100" on:click={deleteContextPlaylist}>
        <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 6h18" />
          <path d="M8 6V4h8v2" />
          <path d="M19 6l-1 14H6L5 6" />
        </svg>
        Delete
      </button>
    </div>
  {/if}

  <ConfirmDialog
    open={showDeletePlaylistConfirm}
    title="Delete playlist"
    message={playlistToDelete ? `Delete playlist "${playlistToDelete.name}"?` : ''}
    confirmLabel="Delete"
    cancelLabel="Cancel"
    onConfirm={confirmDeletePlaylist}
    onCancel={cancelDeletePlaylist}
  />

</section>
