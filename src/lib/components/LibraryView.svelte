<script lang="ts">
  import { artworkUrl } from '../tauri';
  import type { LocalSong, Playlist, ArtistEntry, AlbumEntry, GenreEntry } from '../types';
  import type { ActiveView } from '../navigation';
  import ArtistsView from './ArtistsView.svelte';
  import AlbumsView from './AlbumsView.svelte';
  import PlaylistsView from './PlaylistsView.svelte';
  import GenreView from './GenreView.svelte';
  import SettingsView from './SettingsView.svelte';
  import SongsView from './SongsView.svelte';

  // ── Routing ────────────────────────────────────────────────────────────────
  export let activeView: ActiveView = 'songs';

  // ── Data ───────────────────────────────────────────────────────────────────
  export let songs: LocalSong[] = [];
  export let playlists: Playlist[] = [];
  export let filteredSongs: LocalSong[] = [];
  export let artists: ArtistEntry[] = [];
  export let albums: AlbumEntry[] = [];
  export let genres: GenreEntry[] = [];
  export let query = '';
  export let selectedPath: string | null = null;
  export let currentPath: string | null = null;
  export let artistCount = 0;
  export let albumCount = 0;
  export let status = 'Ready';

  // ── Song actions ───────────────────────────────────────────────────────────
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onEditSong: (song: LocalSong) => void = () => {};
  export let onAddSongToPlaylist: (playlistId: number, song: LocalSong) => Promise<void> | void = () => {};

  // ── Playlist actions ───────────────────────────────────────────────────────
  export let onCreatePlaylist: (name: string) => Promise<void> | void = () => {};
  export let onLoadPlaylistSongIds: (playlistId: number) => Promise<number[]> = async () => [];
  export let onRenamePlaylist: (playlistId: number, name: string) => Promise<void> | void = () => {};
  export let onDeletePlaylist: (playlistId: number) => Promise<void> | void = () => {};
  export let onChoosePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onRemovePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onImportPlaylist: () => Promise<void> | void = () => {};
  export let onExportPlaylist: (playlistId: number) => Promise<void> | void = () => {};
  export let onRemoveSongFromPlaylist: (playlistId: number, song: LocalSong) => Promise<void> | void = () => {};

  // ── Cover actions ──────────────────────────────────────────────────────────
  export let onChooseArtistCover: (artistName: string) => Promise<void> | void = () => {};
  export let onRemoveArtistCover: (artistName: string) => Promise<void> | void = () => {};
  export let onChooseAlbumCover: (albumKey: string) => Promise<void> | void = () => {};
  export let onRemoveAlbumCover: (albumKey: string) => Promise<void> | void = () => {};
  export let onFetchArtistArtworkManual: (artistName: string) => Promise<void> | void = () => {};
  export let onFetchAlbumArtworkManual: (albumKey: string, artist: string, album: string) => Promise<void> | void = () => {};

  // ── Settings props ─────────────────────────────────────────────────────────
  export let playerPlacement: 'right' | 'bottom' = 'right';
  export let onPlayerPlacementChange: (placement: 'right' | 'bottom') => void = () => {};
  export let sidebarMode: 'expanded' | 'collapsed' = 'expanded';
  export let onSidebarModeChange: (mode: 'expanded' | 'collapsed') => void = () => {};
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
  export let onSeekbarStyleChange: (style: 'standard' | 'waveform') => void = () => {};
  export let scanRoots: string[] = [];
  export let isScanning = false;
  export let onRemoveScanRoot: (root: string) => Promise<void> | void = () => {};
  export let dynamicCoverAccent = true;
  export let onDynamicCoverAccentChange: (enabled: boolean) => void = () => {};
  export let blurredBackground = true;
  export let onBlurredBackgroundChange: (enabled: boolean) => void = () => {};
  export let autoFetchArtwork = false;
  export let onAutoFetchArtworkChange: (enabled: boolean) => void = () => {};
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

  // ── Local state ────────────────────────────────────────────────────────────
  let settingsQuery = '';

  // Shared song context menu (needs access to playlists + playlist detail state)
  let contextMenu: { x: number; y: number; song: LocalSong } | null = null;

  // Synced from PlaylistsView so song context menu knows if we're inside a playlist
  let activePlaylist: Playlist | null = null;
  let activePlaylistSongIds: number[] = [];

  // For cross-view navigation: artist detail → album detail
  let initialAlbumKey: string | null = null;

  // Synced from child views to toggle page header visibility / height
  let artistsInDetail = false;
  let albumsInDetail = false;
  let playlistsInDetail = false;
  let genreInDetail = false;

  $: isInDetail =
    (activeView === 'artists' && artistsInDetail) ||
    (activeView === 'albums' && albumsInDetail) ||
    (activeView === 'playlists' && playlistsInDetail) ||
    (activeView === 'genres' && genreInDetail);

  // Page header text
  $: pageTitle =
    activeView === 'settings' ? 'Settings'
    : activeView === 'playlists' ? 'Playlists'
    : activeView === 'artists' ? 'Artists'
    : activeView === 'albums' ? 'Albums'
    : activeView === 'genres' ? 'Genres'
    : 'Main Library';

  $: pageSubtitle =
    activeView === 'settings' ? 'Tune Orca for the way you listen'
    : activeView === 'playlists' ? `${playlists.length} ${playlists.length === 1 ? 'playlist' : 'playlists'}`
    : activeView === 'artists' ? `${artistCount} ${artistCount === 1 ? 'artist' : 'artists'}`
    : activeView === 'albums' ? `${albumCount} ${albumCount === 1 ? 'album' : 'albums'}`
    : activeView === 'genres' ? `${genres.length} ${genres.length === 1 ? 'genre' : 'genres'}`
    : `${songs.length} songs / ${artistCount} artists / ${albumCount} albums`;

  // Reset initialAlbumKey after AlbumsView has consumed it
  $: if (activeView !== 'albums') initialAlbumKey = null;

  // ── Context menu actions ───────────────────────────────────────────────────
  function openSongMenu(event: MouseEvent, song: LocalSong) {
    event.preventDefault();
    contextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 230),
      y: Math.min(event.clientY, window.innerHeight - 230),
      song
    };
  }

  function closeContextMenu() { contextMenu = null; }

  function closeAllFloating() { closeContextMenu(); }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeAllFloating();
  }

  function editContextSong() {
    const song = contextMenu?.song;
    closeContextMenu();
    if (song) onEditSong(song);
  }

  async function addContextSongToPlaylist(playlistId: number) {
    const song = contextMenu?.song;
    closeContextMenu();
    if (song) await onAddSongToPlaylist(playlistId, song);
  }

  async function removeContextSongFromPlaylist() {
    if (!contextMenu || !activePlaylist || contextMenu.song.id === null) return;
    const song = contextMenu.song;
    closeContextMenu();
    await onRemoveSongFromPlaylist(activePlaylist.id, song);
    activePlaylistSongIds = activePlaylistSongIds.filter((id) => id !== song.id);
  }

  async function fetchSongAlbumArt() {
    const song = contextMenu?.song;
    closeContextMenu();
    if (song) await onFetchAlbumArtworkManual(`${song.album_artist}:${song.album}`, song.album_artist, song.album);
  }

  // Called when user clicks an album from the artist detail page
  function handleOpenAlbum(key: string) {
    initialAlbumKey = key;
    activeView = 'albums';
  }
</script>

<svelte:window on:click={closeAllFloating} on:keydown={handleGlobalKeydown} />

<section class="min-h-0 bg-black/42 px-5 py-4">
  {#if activeView === 'songs'}
    <!-- SongsView manages its own header via slot -->
    <SongsView
      {songs}
      {filteredSongs}
      {query}
      {selectedPath}
      {currentPath}
      {status}
      {onChooseSong}
      onOpenSongMenu={openSongMenu}
    >
      <div>
        <h1 class="text-2xl font-bold tracking-normal">{pageTitle}</h1>
        <p class="mt-1 text-xs text-white/42">{pageSubtitle}</p>
      </div>
      <label>
        <span class="sr-only">Search library</span>
        <input
          class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
          bind:value={query}
          placeholder="Search library..."
        />
      </label>
    </SongsView>
  {:else}
    <!-- Shared page header (hidden when a view is in detail mode) -->
    {#if !isInDetail}
      <div class="mb-4 grid items-center gap-4 max-lg:grid-cols-1 grid-cols-[minmax(200px,1fr)_minmax(200px,300px)]">
        <div>
          <h1 class="text-2xl font-bold tracking-normal">{pageTitle}</h1>
          <p class="mt-1 text-xs text-white/42">{pageSubtitle}</p>
        </div>
        {#if activeView === 'settings'}
          <label>
            <span class="sr-only">Search settings</span>
            <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
              bind:value={settingsQuery} placeholder="Search settings..." />
          </label>
        {:else}
          <label>
            <span class="sr-only">Search library</span>
            <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
              bind:value={query} placeholder="Search {activeView}..." />
          </label>
        {/if}
      </div>
    {/if}

    <!-- View container -->
    <div class={`${isInDetail ? 'h-full' : 'h-[calc(100%-72px)]'} min-h-0 overflow-hidden`}>
      {#if activeView === 'settings'}
        <SettingsView
          {playerPlacement} {onPlayerPlacementChange}
          {sidebarMode} {onSidebarModeChange}
          {seekbarStyle} {onSeekbarStyleChange}
          {scanRoots} {isScanning} {onRemoveScanRoot}
          {dynamicCoverAccent} {onDynamicCoverAccentChange}
          blurredBackground={blurredBackground} {onBlurredBackgroundChange}
          {fontFamily} {onFontFamilyChange}
          {fontSizePercent} {onFontSizePercentChange}
          {showQualityInfo} {onShowQualityInfoChange}
          {gaplessPlayback} {onGaplessPlaybackChange}
          {autoFetchArtwork} {onAutoFetchArtworkChange}
          {theme} {onThemeChange}
        />
      {:else if activeView === 'playlists'}
        <PlaylistsView
          {playlists} {songs} {query} {currentPath}
          {onChooseSong} {onCreatePlaylist} {onLoadPlaylistSongIds}
          {onRenamePlaylist} {onDeletePlaylist}
          {onChoosePlaylistCover} {onRemovePlaylistCover}
          {onImportPlaylist} {onExportPlaylist}
          onOpenSongMenu={openSongMenu}
          bind:selectedPlaylist={activePlaylist}
          bind:selectedPlaylistSongIds={activePlaylistSongIds}
          bind:isInDetail={playlistsInDetail}
        />
      {:else if activeView === 'albums'}
        <AlbumsView
          {albums} {songs} {query} {currentPath} {showQualityInfo}
          {onChooseSong} {onChooseAlbumCover} {onRemoveAlbumCover} {onFetchAlbumArtworkManual}
          onOpenSongMenu={openSongMenu}
          {initialAlbumKey}
          bind:isInDetail={albumsInDetail}
        />
      {:else if activeView === 'genres'}
        <GenreView
          {genres} {songs} {query} {currentPath}
          {onChooseSong}
          onOpenSongMenu={openSongMenu}
          bind:isInDetail={genreInDetail}
        />
      {:else}
        <!-- artists (default fallback) -->
        <ArtistsView
          {artists} {albums} {songs} {query} {currentPath}
          {onChooseSong} {onChooseArtistCover} {onRemoveArtistCover} {onFetchArtistArtworkManual}
          onOpenSongMenu={openSongMenu}
          onOpenAlbum={handleOpenAlbum}
          bind:isInDetail={artistsInDetail}
        />
      {/if}
    </div>
  {/if}

  <!-- ── Shared song context menu ──────────────────────────────────────────── -->
  {#if contextMenu}
    <div role="menu" tabindex="-1"
      class="fixed z-50 w-56 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
      style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px;`}
      on:click|stopPropagation on:keydown|stopPropagation>
      <div class="border-b border-white/[0.06] px-3 py-2">
        <p class="truncate text-xs font-bold text-white">{contextMenu.song.title}</p>
        <p class="truncate text-[11px] text-white/42">{contextMenu.song.artist}</p>
      </div>
      <button role="menuitem"
        class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white"
        on:click={editContextSong}>
        Edit metadata
      </button>
      <button role="menuitem"
        class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white"
        on:click={fetchSongAlbumArt}>
        Fetch cover online
      </button>
      {#if activePlaylist}
        <button role="menuitem"
          class="flex h-9 w-full items-center px-3 text-left text-xs font-semibold text-red-100/72 transition hover:bg-red-500/10 hover:text-red-100"
          on:click={removeContextSongFromPlaylist}>
          Remove from playlist
        </button>
      {/if}
      {#if playlists.length}
        <p class="border-t border-white/[0.06] px-3 pb-1 pt-2 text-[10px] font-bold uppercase text-white/32">Add to playlist</p>
        {#each playlists as playlist}
          <button role="menuitem"
            class="flex h-8 w-full items-center justify-between gap-3 px-3 text-left text-xs text-white/72 transition hover:bg-white/[0.08] hover:text-white"
            on:click={() => addContextSongToPlaylist(playlist.id)}>
            <span class="truncate">{playlist.name}</span>
            <span class="text-white/32">{playlist.song_count}</span>
          </button>
        {/each}
      {:else}
        <p class="px-3 py-3 text-xs leading-5 text-white/44">Create a playlist first, then right-click songs to add them.</p>
      {/if}
    </div>
  {/if}
</section>
