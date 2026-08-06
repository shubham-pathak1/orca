<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import AppBackdrop from './lib/components/AppBackdrop.svelte';
  import DetailsPanel from './lib/components/DetailsPanel.svelte';
  import FullPlayer from './lib/components/FullPlayer.svelte';
  import LibraryView from './lib/components/LibraryView.svelte';
  import MetadataEditor from './lib/components/MetadataEditor.svelte';
  import PlayerBar from './lib/components/PlayerBar.svelte';
  import QueuePanel from './lib/components/QueuePanel.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import {
    artworkUrl,
    updateMediaControls,
    fetchAlbumArtworkManual,
    fetchAllMissingArtwork,
    pickLyricsFile
  } from './lib/tauri';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { register, unregister, isRegistered } from '@tauri-apps/plugin-global-shortcut';
  import { isSupported as isTaskbarSupported, setPlaybackState, setNavigationEnabled } from 'tauri-plugin-taskbar';
  import type { ActiveView } from './lib/navigation';
  import { createLibraryStore } from './lib/stores/library';
  import { createPlaybackStore } from './lib/stores/playback';
  import { createPreferencesStore } from './lib/stores/preferences';
  import { createQueueStore } from './lib/stores/queue';
  import { artworkSuspended } from './lib/stores/artwork-visibility';
  import { createLibraryActions } from './lib/stores/library-actions';
  import { createPlaybackFlow } from './lib/stores/playback-flow';
  import {
    applyRootFontSize,
    fontStack,
    readStoredString,
    sampleArtworkAccent,
    syncCustomFont
  } from './lib/app-utils';
  import type { LibrarySnapshot, LocalSong, PlaybackState, Playlist, ArtistEntry, AlbumEntry, GenreEntry } from './lib/types';

  const libraryStore = createLibraryStore();
  let songs: LocalSong[] = [];
  let playlists: Playlist[] = [];
  let artists: ArtistEntry[] = [];
  let albums: AlbumEntry[] = [];
  let genres: GenreEntry[] = [];
  let folderCount = 0;
  let scanRoots: string[] = [];
  let isScanning = false;
  const unsubscribeLibrary = libraryStore.subscribe((state) => {
    songs = state.songs;
    playlists = state.playlists;
    artists = state.artists;
    albums = state.albums;
    genres = state.genres;
    folderCount = state.folderCount;
    scanRoots = state.scanRoots;
    isScanning = state.isScanning;
  });
  const playbackStore = createPlaybackStore();
  let playback: PlaybackState;
  const unsubscribePlayback = playbackStore.subscribe((state) => {
    playback = state;
  });
  let query = '';
  let activeView: ActiveView = 'songs';
  let status = 'Ready';
  let selectedPath: string | null = null;
  let fullPlayerOpen = false;
  let fullPlayerLyricsOpen = false;
  let queueOpen = false;

  $: artworkSuspended.set(fullPlayerOpen);
  let accentRgb = '245,245,245';
  let sampledArtwork: string | null = null;
  const preferencesStore = createPreferencesStore();
  let playerPlacement: 'right' | 'bottom' = 'bottom';
  let sidebarMode: 'expanded' | 'collapsed' = 'expanded';
  let seekbarStyle: 'standard' | 'waveform' = 'waveform';
  let dynamicCoverAccent = true;
  let blurredNowPlayingBackground = true;
  let fontFamily = 'Plus Jakarta Sans';
  let fontSizePercent = 100;
  let showQualityInfo = true;
  let gaplessPlayback = true;
  let autoFetchArtwork = false;
  let theme: 'default' = 'default';
  let shuffleEnabled = false;
  let repeatMode: 'off' | 'all' | 'one' = 'off';
  const unsubscribePreferences = preferencesStore.subscribe((preferences) => {
    playerPlacement = preferences.playerPlacement;
    sidebarMode = preferences.sidebarMode;
    seekbarStyle = preferences.seekbarStyle;
    dynamicCoverAccent = preferences.dynamicCoverAccent;
    blurredNowPlayingBackground = preferences.blurredNowPlayingBackground;
    fontFamily = preferences.fontFamily;
    fontSizePercent = preferences.fontSizePercent;
    showQualityInfo = preferences.showQualityInfo;
    gaplessPlayback = preferences.gaplessPlayback;
    autoFetchArtwork = preferences.autoFetchArtwork;
    shuffleEnabled = preferences.shuffleEnabled;
    repeatMode = preferences.repeatMode;
    fullPlayerLyricsOpen = preferences.fullPlayerLyricsOpen;
  });
  const queueStore = createQueueStore();
  let queueState = { orderPaths: [] as string[], removedPaths: [] as string[], shufflePlayedPaths: new Set<string>() };
  const unsubscribeQueue = queueStore.subscribe((state) => {
    queueState = state;
  });
  let metadataEditorSong: LocalSong | null = null;
  let isSavingMetadata = false;
  let taskbarSupported = false;
  let previousControlSync = { path: null as string | null, playing: false };
  $: bottomRowSize = '96px';
  $: defaultAccentRgb = '245,245,245';
  $: effectiveAccentRgb = dynamicCoverAccent && sampledArtwork ? accentRgb : defaultAccentRgb;
  $: if (playback.current_path) {
    window.localStorage.setItem('orca.lastPlayedPath', playback.current_path);
  }
  $: if (playback.current_path !== previousControlSync.path || playback.is_playing !== previousControlSync.playing) {
    const nowPlaying = songs.find((s) => s.path === playback.current_path);
    if (nowPlaying) {
      updateMediaControls({
        title: nowPlaying.title || nowPlaying.path.split('/').pop()?.split('\\').pop() || 'Unknown',
        artist: nowPlaying.artist,
        album: nowPlaying.album,
        duration: nowPlaying.duration ? nowPlaying.duration / 1000 : undefined,
        cover_url: 'file://' + (nowPlaying.artwork || 'D:\\projects\\orca\\public\\cover.png'),
        playing: playback.is_playing,
        progress: playback.position_ms / 1000,
      }).catch(console.error);
      previousControlSync = { path: playback.current_path, playing: playback.is_playing };
      if (taskbarSupported) {
        invoke('plugin:taskbar|set_playback_state', {
          isPlaying: playback.is_playing,
          is_playing: playback.is_playing
        }).catch(console.error);
      }
    }
  }

  $: filteredSongs = songs.filter((song) => {
    const needle = query.trim().toLowerCase();
    if (!needle) {
      return true;
    }

    return [song.title, song.artist, song.album, song.format ?? ''].some((value) =>
      value.toLowerCase().includes(needle)
    );
  });

  $: nowPlaying = songs.find((song) => song.path === playback.current_path) ?? null;
  $: selectedSong = songs.find((song) => song.path === selectedPath) ?? nowPlaying ?? filteredSongs[0] ?? null;
  $: currentQueuePath = playback.current_path ?? selectedPath;
  $: queueOrderPaths = queueState.orderPaths;
  $: queueRemovedPaths = queueState.removedPaths;
  $: queueRemovedPathSet = new Set(queueRemovedPaths);
  $: orderedPlaybackSongs = queueStore.playableSongs(songs, currentQueuePath);
  $: queueSongs = queueStore.queueSongs(songs, playback.current_path ?? selectedPath, repeatMode);
  $: albumCount = albums.length;
  $: artistCount = artists.length;
  $: ambientArtwork = artworkUrl((nowPlaying ?? selectedSong)?.artwork_preview ?? (nowPlaying ?? selectedSong)?.artwork ?? null);
  $: shellStyle = [
    `--cover-art: ${ambientArtwork ? `url("${ambientArtwork}")` : 'url("/cover.png")'}`,
    `--accent: rgb(${effectiveAccentRgb})`,
    `--accent-soft: rgba(${effectiveAccentRgb}, 0.18)`,
    `--accent-mid: rgba(${effectiveAccentRgb}, 0.34)`,
    `font-family: ${fontStack(fontFamily)}`
  ].join('; ');
  $: if (dynamicCoverAccent && ambientArtwork && ambientArtwork !== sampledArtwork) {
    void sampleAccent(ambientArtwork);
  }
  $: applyRootFontSize(fontSizePercent);
  $: if (typeof window !== 'undefined') {
    window.localStorage.setItem('orca.fullPlayerLyricsOpen', String(fullPlayerLyricsOpen));
  }

  const playbackFlow = createPlaybackFlow({
    playbackStore,
    queueStore,
    getPlayback: () => playback,
    getSongs: () => songs,
    getSelectedSong: () => selectedSong,
    getSelectedPath: () => selectedPath,
    setSelectedPath: (path) => (selectedPath = path),
    getOrderedPlaybackSongs: () => orderedPlaybackSongs,
    getQueueOrderPaths: () => queueOrderPaths,
    getQueueRemovedPathSet: () => queueRemovedPathSet,
    getShufflePlayedPathSet: () => queueState.shufflePlayedPaths,
    getGaplessPlayback: () => gaplessPlayback,
    getShuffleEnabled: () => shuffleEnabled,
    getRepeatMode: () => repeatMode,
    onPlaybackStateChange: (isPlaying) => {
      if (taskbarSupported) {
        void setPlaybackState(isPlaying).catch(() => {});
      }
    },
    onPlaybackError: (error) => {
      status = error instanceof Error ? error.message : String(error);
    }
  });

  onMount(() => {
    preferencesStore.load();
    theme = readStoredString('orca.theme', 'default', ['default']);

    const lastPlayedPath = window.localStorage.getItem('orca.lastPlayedPath');
    if (lastPlayedPath) {
      selectedPath = lastPlayedPath;
    }

    void (async () => {
      taskbarSupported = await isTaskbarSupported().catch(() => false);
      if (taskbarSupported) {
        // autoAttach is true in tauri.conf.json, so buttons are attached automatically.
        // We just need to set the initial state after a short delay to ensure window is ready.
        await new Promise(r => setTimeout(r, 500));
        await setNavigationEnabled(true, true).catch(e => console.error('Taskbar nav error:', e));
        await invoke('plugin:taskbar|set_playback_state', {
          isPlaying: false,
          is_playing: false
        }).catch(e => console.error('Taskbar state error:', e));
      }
      const snapshot = await libraryStore.load();
      applyLibrarySnapshot(snapshot);
      
      if (autoFetchArtwork) {
        void fetchAllMissingArtwork();
      }

      // Show accurate library count from the snapshot
      status = snapshot.songs && snapshot.songs.length ? `${snapshot.songs.length} tracks loaded` : 'Add a folder to build your library';

      await playbackStore.restoreVolume();
    })();

    playbackStore.startPolling(playbackFlow.handlePlaybackSnapshot);

    window.addEventListener('keydown', handleKeydown);

    let unlisteners: Array<() => void> = [];
    listen('media-play', () => playbackStore.resume()).then(u => unlisteners.push(u));
    listen('media-pause', () => playbackStore.pause()).then(u => unlisteners.push(u));
    listen('media-toggle', () => togglePlayback()).then(u => unlisteners.push(u));
    listen('media-next', () => playNextSong()).then(u => unlisteners.push(u));
    listen('media-prev', () => playPreviousSong()).then(u => unlisteners.push(u));

    async function safeRegister(key: string, handler: (e: any) => void) {
      if (await isRegistered(key)) {
        await unregister(key);
      }
      await register(key, handler).catch(e => console.error(`Failed to register ${key}:`, e));
    }

    void safeRegister('MediaPlayPause', (event) => {
      if (event.state === 'Pressed') {
        void togglePlayback();
      }
    });
    void safeRegister('MediaTrackNext', (event) => {
      if (event.state === 'Pressed') {
        void playNextSong();
      }
    });
    void safeRegister('MediaTrackPrevious', (event) => {
      if (event.state === 'Pressed') {
        void playPreviousSong();
      }
    });

    const unlisten = listen<number>('scan-progress', (event) => {
      if (isScanning) {
        status = `Scanning... ${event.payload} songs found`;
      }
    });
    
    const unlistenLibrary = listen('library-refreshed', async () => {
      try {
        const snapshot = await libraryStore.load();
        applyLibrarySnapshot(snapshot);
      } catch (error) {
        console.error('Failed to get library snapshot after refresh', error);
      }
    });

    const unlistenLibraryWatcher = listen('library-watcher-refreshed', async () => {
      try {
        const snapshot = await libraryStore.load();
        applyLibrarySnapshot(snapshot);
        status = `Library updated: ${snapshot.songs.length} tracks`;
      } catch (error) {
        console.error('Failed to get library snapshot after automatic refresh', error);
      }
    });

    return () => {
      playbackStore.stopPolling();
      unsubscribeLibrary();
      unsubscribePreferences();
      unsubscribePlayback();
      unsubscribeQueue();
      if (typeof window !== 'undefined') {
        window.removeEventListener('keydown', handleKeydown);
      }
      void unregister('MediaPlayPause');
      void unregister('MediaTrackNext');
      void unregister('MediaTrackPrevious');
      unlisteners.forEach(u => u());
      void unlisten.then((fn) => fn());
      void unlistenLibrary.then((fn) => fn());
      void unlistenLibraryWatcher.then((fn) => fn());
    };
  });

  $: syncCustomFont(fontFamily);

  function setPlayerPlacement(placement: 'right' | 'bottom') {
    preferencesStore.setPlayerPlacement(placement);
  }

  function setSidebarMode(mode: 'expanded' | 'collapsed') {
    preferencesStore.setSidebarMode(mode);
  }

  function setSeekbarStyle(style: 'standard' | 'waveform') {
    preferencesStore.setSeekbarStyle(style);
  }

  function setTheme(value: 'default') {
    theme = value;
    window.localStorage.setItem('orca.theme', value);
    if (!sampledArtwork) {
      accentRgb = '245,245,245';
    }
  }

  function setDynamicCoverAccent(enabled: boolean) {
    preferencesStore.setDynamicCoverAccent(enabled);
    if (!enabled) {
      sampledArtwork = null;
      accentRgb = '245,245,245';
    } else if (ambientArtwork) {
      sampledArtwork = null;
      void sampleAccent(ambientArtwork);
    }
  }

  function setBlurredNowPlayingBackground(enabled: boolean) {
    preferencesStore.setBlurredNowPlayingBackground(enabled);
  }

  function setFontFamily(value: string) {
    preferencesStore.setFontFamily(value);
  }

  function setFontSizePercent(value: number) {
    preferencesStore.setFontSizePercent(value);
  }

  function setShowQualityInfo(enabled: boolean) {
    preferencesStore.setShowQualityInfo(enabled);
  }

  function setGaplessPlayback(enabled: boolean) {
    preferencesStore.setGaplessPlayback(enabled);
    if (!enabled) {
      playbackFlow.clearQueuedNext();
    }
  }

  function setAutoFetchArtwork(enabled: boolean) {
    preferencesStore.setAutoFetchArtwork(enabled);
  }

  function toggleShuffle() {
    preferencesStore.toggleShuffle();
  }

  function cycleRepeat() {
    preferencesStore.cycleRepeat();
  }



  function toggleQueue() {
    queueOpen = !queueOpen;
  }

  function reorderQueueSong(sourcePath: string, targetPath: string) {
    queueStore.reorder(songs, playback.current_path ?? selectedPath, sourcePath, targetPath);
  }

  function removeQueueSong(path: string) {
    queueStore.remove(playback.current_path ?? selectedPath, path);
  }

  function clearQueue() {
    queueStore.clear(songs, playback.current_path ?? selectedPath);
  }

  function applyLibrarySnapshot(snapshot: LibrarySnapshot) {
    libraryStore.applySnapshot(snapshot);
    playbackStore.set(snapshot.playback);
    queueStore.syncSongs(snapshot.songs);

    if (metadataEditorSong) {
      metadataEditorSong = songs.find((song) => song.path === metadataEditorSong?.path) ?? metadataEditorSong;
    }
  }

  const {
    addFolder,
    refreshLibrary,
    addPlaylist,
    removeScanRoot,
    renameExistingPlaylist,
    deleteExistingPlaylist,
    handleChoosePlaylistCover,
    handleRemovePlaylistCover,
    importExistingPlaylist,
    exportExistingPlaylist,
    handleFetchArtistArtworkManual,
    handleFetchAlbumArtworkManual,
    chooseExistingArtistCover,
    clearExistingArtistCover,
    chooseExistingAlbumCover,
    clearExistingAlbumCover,
    loadPlaylistSongs,
    addToPlaylist,
    editSongMetadata,
    saveSongMetadata,
    replaceSongCover,
    clearSongCover,
    removeFromPlaylist
  } = createLibraryActions({
    libraryStore,
    applySnapshot: applyLibrarySnapshot,
    getAutoFetchArtwork: () => autoFetchArtwork,
    setStatus: (message) => (status = message),
    setMetadataEditorSong: (song) => (metadataEditorSong = song),
    setSavingMetadata: (saving) => (isSavingMetadata = saving)
  });

  async function handleKeydown(event: KeyboardEvent) {
    const key = event.key.toLowerCase();
    if (event.key === 'F11') {
      event.preventDefault();
      await toggleFullscreen();
      return;
    }

    if (key === 'l' && !event.altKey && !event.ctrlKey && !event.metaKey && !isTextEntryTarget(event)) {
      event.preventDefault();
      if (!fullPlayerOpen) {
        fullPlayerOpen = true;
        fullPlayerLyricsOpen = true;
      } else {
        fullPlayerLyricsOpen = !fullPlayerLyricsOpen;
      }
      return;
    }

    if (key === 'q' && !event.altKey && !event.ctrlKey && !event.metaKey && !isTextEntryTarget(event)) {
      event.preventDefault();
      toggleQueue();
      return;
    }

    if (event.key === 'Escape' && queueOpen) {
      event.preventDefault();
      queueOpen = false;
      return;
    }

    if (event.altKey && key === 'l') {
      event.preventDefault();
      activeView = 'songs';
      fullPlayerOpen = false;
      return;
    }

    if (event.altKey && key === 'a') {
      event.preventDefault();
      activeView = 'artists';
      fullPlayerOpen = false;
      return;
    }

    if (event.altKey && key === 'b') {
      event.preventDefault();
      activeView = 'albums';
      fullPlayerOpen = false;
      return;
    }

    if (event.altKey && key === 'p') {
      event.preventDefault();
      activeView = 'playlists';
      fullPlayerOpen = false;
      return;
    }

    if (event.altKey && event.key === 'ArrowRight') {
      event.preventDefault();
      await playNextSong();
      return;
    }

    if (event.altKey && event.key === 'ArrowLeft') {
      event.preventDefault();
      await playPreviousSong();
      return;
    }

    if (event.code === 'Space' && !event.altKey && !event.ctrlKey && !event.metaKey && !isTextEntryTarget(event)) {
      event.preventDefault();
      await togglePlayback();
      return;
    }

    if (key === 'm' && !event.altKey && !event.ctrlKey && !event.metaKey && !isTextEntryTarget(event)) {
      event.preventDefault();
      await toggleMute();
      return;
    }

    if (shouldIgnorePlaybackShortcut(event)) {
      return;
    }
  }

  function shouldIgnorePlaybackShortcut(event: KeyboardEvent) {
    if (isTextEntryTarget(event)) {
      return true;
    }

    const target = event.target as HTMLElement | null;
    return Boolean(target?.closest('button'));
  }

  function isTextEntryTarget(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    return Boolean(target?.closest('input, textarea, select, [contenteditable="true"]'));
  }

  function suppressNativeContextMenu(event: MouseEvent) {
    event.preventDefault();
  }

  async function toggleFullscreen() {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.setFullscreen(!(await appWindow.isFullscreen()));
      return;
    } catch {
      // Browser fallback for Vite preview.
    }

    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await document.documentElement.requestFullscreen();
    }
  }

  async function sampleAccent(src: string) {
    sampledArtwork = src;

    try {
      accentRgb = await sampleArtworkAccent(src);
    } catch {
      sampledArtwork = null;
      accentRgb = defaultAccentRgb;
    }
  }

  async function chooseSong(song: LocalSong, contextSongs?: LocalSong[]) {
    await playbackFlow.chooseSong(song, contextSongs);
  }

  async function playPreviousSong() {
    await playbackFlow.playPreviousSong();
  }

  async function playNextSong() {
    await playbackFlow.playNextSong();
  }

  async function togglePlayback() {
    await playbackFlow.togglePlayback();
  }

  async function seek(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    await playbackFlow.seek(Number(target.value));
  }

  async function seekToPosition(positionMs: number) {
    await playbackFlow.seek(positionMs);
  }

  async function changeVolume(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    await playbackStore.setVolume(Number(target.value));
  }

  async function toggleMute() {
    await playbackStore.toggleMute();
  }

  async function adjustVolumeByAmount(amount: number) {
    await playbackStore.adjustVolume(amount);
  }
</script>

<svelte:head>
  <title>Orca</title>
</svelte:head>

<svelte:window on:contextmenu={suppressNativeContextMenu} />

<main class="relative h-screen overflow-hidden bg-[#090a0c] text-[#f4f4f5]" style={shellStyle}>
  <AppBackdrop {shellStyle} blurredBackground={blurredNowPlayingBackground} />

  <div
    class={`relative grid h-full transition-all ${
      playerPlacement === 'right'
        ? (sidebarMode === 'collapsed'
            ? 'grid-cols-[56px_minmax(0,1fr)_250px] grid-rows-[minmax(0,1fr)_96px] xl:grid-rows-[1fr] max-xl:grid-cols-[56px_minmax(0,1fr)]'
            : 'grid-cols-[132px_minmax(0,1fr)_250px] grid-rows-[minmax(0,1fr)_96px] xl:grid-rows-[1fr] max-xl:grid-cols-[132px_minmax(0,1fr)]')
        : (sidebarMode === 'collapsed'
            ? 'grid-cols-[56px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)_96px]'
            : 'grid-cols-[132px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)_96px]')
    } max-md:grid-cols-[56px_minmax(0,1fr)]`}
  >
    <Sidebar {activeView} {isScanning} {folderCount} {sidebarMode} onSelect={(view) => (activeView = view)} onAddFolder={addFolder} onRefresh={refreshLibrary} />
    <LibraryView
      bind:activeView
      {songs}
      {playlists}
      {filteredSongs}
      {artists}
      {albums}
      {genres}
      bind:query
      {selectedPath}
      currentPath={playback.current_path}
      {artistCount}
      {albumCount}
      onChooseSong={chooseSong}
      onCreatePlaylist={addPlaylist}
      onAddSongToPlaylist={addToPlaylist}
      onLoadPlaylistSongIds={loadPlaylistSongs}
      onRenamePlaylist={renameExistingPlaylist}
      onDeletePlaylist={deleteExistingPlaylist}
      onChoosePlaylistCover={handleChoosePlaylistCover}
      onRemovePlaylistCover={handleRemovePlaylistCover}
      onImportPlaylist={importExistingPlaylist}
      onExportPlaylist={exportExistingPlaylist}
      onChooseArtistCover={chooseExistingArtistCover}
      onRemoveArtistCover={clearExistingArtistCover}
      onChooseAlbumCover={chooseExistingAlbumCover}
      onRemoveAlbumCover={clearExistingAlbumCover}
      onFetchArtistArtworkManual={handleFetchArtistArtworkManual}
      onFetchAlbumArtworkManual={handleFetchAlbumArtworkManual}
      onRemoveSongFromPlaylist={removeFromPlaylist}
      onEditSong={(song) => { metadataEditorSong = song; }}
      {playerPlacement}
      onPlayerPlacementChange={setPlayerPlacement}
      {sidebarMode}
      onSidebarModeChange={setSidebarMode}
      {seekbarStyle}
      onSeekbarStyleChange={setSeekbarStyle}
      {scanRoots}
      {isScanning}
      onRemoveScanRoot={removeScanRoot}
      {dynamicCoverAccent}
      onDynamicCoverAccentChange={setDynamicCoverAccent}
      blurredBackground={blurredNowPlayingBackground}
      onBlurredBackgroundChange={setBlurredNowPlayingBackground}
      {fontFamily}
      onFontFamilyChange={setFontFamily}
      {fontSizePercent}
      onFontSizePercentChange={setFontSizePercent}
      {showQualityInfo}
      onShowQualityInfoChange={setShowQualityInfo}
      {gaplessPlayback}
      onGaplessPlaybackChange={setGaplessPlayback}
      {autoFetchArtwork}
      onAutoFetchArtworkChange={setAutoFetchArtwork}
      {theme}
      onThemeChange={setTheme}
      {status}
    />
    {#if playerPlacement === 'right'}
      <DetailsPanel
        song={nowPlaying ?? selectedSong}
        {playback}
        {seekbarStyle}
        {showQualityInfo}
        {shuffleEnabled}
        {repeatMode}
        onToggle={togglePlayback}
        onPrevious={playPreviousSong}
        onNext={playNextSong}
        onToggleShuffle={toggleShuffle}
        onCycleRepeat={cycleRepeat}
        onSeek={seek}
        onVolume={changeVolume}
        onToggleMute={toggleMute}
        onAdjustVolume={adjustVolumeByAmount}
        onOpenFullPlayer={() => (fullPlayerOpen = true)}
      />
    {/if}
    <div class={playerPlacement === 'bottom' ? 'contents' : 'hidden max-xl:contents'}>
      <PlayerBar
        nowPlaying={nowPlaying ?? selectedSong}
        {playback}
        {seekbarStyle}
        {showQualityInfo}
        {shuffleEnabled}
        {repeatMode}
        alwaysVisible={playerPlacement === 'bottom'}
        onToggle={togglePlayback}
        onPrevious={playPreviousSong}
        onNext={playNextSong}
        onToggleShuffle={toggleShuffle}
        onCycleRepeat={cycleRepeat}
        onSeek={seek}
      onVolume={changeVolume}
      onToggleMute={toggleMute}
      onAdjustVolume={adjustVolumeByAmount}
      onOpenFullPlayer={() => (fullPlayerOpen = true)}
      {queueOpen}
      onToggleQueue={toggleQueue}
    />
    </div>
    <FullPlayer
      open={fullPlayerOpen}
      song={nowPlaying ?? selectedSong}
      bind:lyricsOpen={fullPlayerLyricsOpen}
      {playback}
      {seekbarStyle}
      {showQualityInfo}
      blurredBackground={blurredNowPlayingBackground}
      {shuffleEnabled}
      {repeatMode}
      onClose={() => (fullPlayerOpen = false)}
      onToggle={togglePlayback}
      onPrevious={playPreviousSong}
      onNext={playNextSong}
      onToggleShuffle={toggleShuffle}
      onCycleRepeat={cycleRepeat}
      onSeek={seek}
      onSeekTo={seekToPosition}
      {queueOpen}
      onToggleQueue={toggleQueue}
      onVolume={changeVolume}
      onToggleMute={toggleMute}
      onAdjustVolume={adjustVolumeByAmount}
      onEditSong={() => {
        if (nowPlaying ?? selectedSong) {
          metadataEditorSong = nowPlaying ?? selectedSong;
          fullPlayerOpen = false;
        }
      }}
    />
    <MetadataEditor
      open={Boolean(metadataEditorSong)}
      song={metadataEditorSong}
      isSaving={isSavingMetadata}
      onClose={() => (metadataEditorSong = null)}
      onSave={saveSongMetadata}
      onReplaceCover={replaceSongCover}
      onRemoveCover={clearSongCover}
      onImportLyrics={async () => {
        try {
          return await pickLyricsFile();
        } catch (error) {
          status = 'Could not read lyrics file';
          console.error(error);
          return null;
        }
      }}
      onFetchAlbumArtwork={async (song) => {
        try {
          const albumKey = `${song.album_artist}:${song.album}`;
          const snapshot = await fetchAlbumArtworkManual(albumKey, song.album_artist, song.album);
          applyLibrarySnapshot(snapshot);
          // Refresh the local song object in the editor if it's currently open
          if (metadataEditorSong?.id === song.id) {
            metadataEditorSong = snapshot.songs.find(s => s.id === song.id) || null;
          }
          status = 'Fetched album artwork';
        } catch (e: any) {
          status = `Error: ${e}`;
          console.error(e);
        }
      }}
    />
    <QueuePanel
      open={queueOpen}
      songs={queueSongs}
      currentPath={playback.current_path}
      {shuffleEnabled}
      {repeatMode}
      onClose={() => (queueOpen = false)}
      onChooseSong={chooseSong}
      onReorder={reorderQueueSong}
      onRemoveSong={removeQueueSong}
      onClear={clearQueue}
    />
  </div>
</main>
