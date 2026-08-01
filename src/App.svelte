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
    addSongToPlaylist,
    artworkUrl,
    chooseSongCover,
    choosePlaylistCover,
    chooseArtistCover,
    removeArtistCover,
    chooseAlbumCover,
    removeAlbumCover,
    createPlaylist,
    deletePlaylist,
    libraryFolderCount,
    queueNextPlayback,
    playlistSongIds,
    removePlaylistCover,
    removeSongCover,
    removeSongFromPlaylist,
    renamePlaylist,
    updateSongMetadata,
    updateMediaControls,
    fetchArtistArtworkManual,
    fetchAlbumArtworkManual,
    fetchAllMissingArtwork
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
  import type { LibrarySnapshot, LocalSong, PlaybackState, Playlist, SongMetadataUpdate, ArtistEntry, AlbumEntry, GenreEntry } from './lib/types';

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
  let isHandlingTrackEnd = false;
  let handledEndedPath: string | null = null;
  let queuedNextForPath: string | null = null;
  let queuedNextPath: string | null = null;
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

  onMount(() => {
    preferencesStore.load();
    theme = readPreference('orca.theme', 'default', ['default']);

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

    playbackStore.startPolling(handlePlaybackSnapshot);

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
    };
  });

  function readPreference<T extends string>(key: string, fallback: T, allowed: T[]): T {
    const value = window.localStorage.getItem(key);
    return allowed.includes(value as T) ? (value as T) : fallback;
  }

  function readBooleanPreference(key: string, fallback: boolean) {
    const value = window.localStorage.getItem(key);
    return value === null ? fallback : value === 'true';
  }

  function readNumberPreference(key: string, fallback: number, min: number, max: number) {
    const value = Number(window.localStorage.getItem(key));
    return Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
  }

  function fontStack(value: string) {
    if (value === 'System') {
      return 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif';
    }
    if (value === 'Plus Jakarta Sans') {
      return '"Plus Jakarta Sans", ui-sans-serif, system-ui, sans-serif';
    }
    if (value.startsWith('file:')) {
      return '"OrcaCustomFont", ui-sans-serif, system-ui, sans-serif';
    }
    // Custom font name — wrap in quotes and fall back gracefully
    return `"${value}", ui-sans-serif, system-ui, sans-serif`;
  }

  import { convertFileSrc } from '@tauri-apps/api/core';

  function registerFontFile(filePath: string) {
    const existing = document.getElementById('orca-custom-font-face');
    if (existing) existing.remove();
    const assetUrl = convertFileSrc(filePath);
    const style = document.createElement('style');
    style.id = 'orca-custom-font-face';
    style.textContent = `@font-face { font-family: 'OrcaCustomFont'; src: url('${assetUrl}'); font-display: swap; }`;
    document.head.appendChild(style);
  }

  $: if (fontFamily.startsWith('file:')) {
    registerFontFile(fontFamily.slice(5));
  } else {
    const existing = document.getElementById('orca-custom-font-face');
    if (existing) existing.remove();
  }

  function applyRootFontSize(value: number) {
    if (typeof document === 'undefined') {
      return;
    }

    document.documentElement.style.fontSize = `${16 * (value / 100)}px`;
  }

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
      queuedNextForPath = null;
      queuedNextPath = null;
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

  async function handlePlaybackSnapshot(nextPlayback: PlaybackState) {
    const previousPlayback = playback;
    playbackStore.set(nextPlayback);

    if (previousPlayback.current_path !== nextPlayback.current_path) {
      selectedPath = nextPlayback.current_path;
      queuedNextForPath = null;
      queuedNextPath = null;
      handledEndedPath = null;
      
      // If we seamlessly transitioned out of the context (artist/album) into the full library,
      // clear the context so UI and Prev/Next buttons use the full library again.
      if (nextPlayback.current_path && queueStore.hasContext() && !queueOrderPaths.includes(nextPlayback.current_path)) {
        queueStore.clearContext();
      }
    } else if (previousPlayback.is_playing && previousPlayback.position_ms > nextPlayback.position_ms + 1000) {
      queuedNextForPath = null;
      queuedNextPath = null;
      handledEndedPath = null;
    }

    if (taskbarSupported && previousPlayback.is_playing !== nextPlayback.is_playing) {
      void setPlaybackState(nextPlayback.is_playing).catch(() => {});
    }

    await maybeQueueNextTrack(nextPlayback);

    if (!nextPlayback.current_path || isHandlingTrackEnd || handledEndedPath === nextPlayback.current_path) {
      return;
    }

    const endingPosition = Math.max(previousPlayback.position_ms, nextPlayback.position_ms);
    const nearEnd = nextPlayback.duration_ms > 0 && endingPosition >= Math.max(0, nextPlayback.duration_ms - 1500);
    const playbackStoppedAtEnd =
      previousPlayback.current_path === nextPlayback.current_path &&
      previousPlayback.is_playing &&
      !nextPlayback.is_playing &&
      nearEnd;

    if (!playbackStoppedAtEnd) {
      return;
    }

    handledEndedPath = nextPlayback.current_path;
    isHandlingTrackEnd = true;
    try {
      await handleTrackEnded(nextPlayback.current_path);
    } finally {
      isHandlingTrackEnd = false;
    }
  }

  async function maybeQueueNextTrack(nextPlayback: PlaybackState) {
    if (!gaplessPlayback || !nextPlayback.current_path || !nextPlayback.is_playing || nextPlayback.duration_ms <= 0) {
      return;
    }

    const remainingMs = nextPlayback.duration_ms - nextPlayback.position_ms;
    if (remainingMs > 5000 || remainingMs < 0) {
      return;
    }

    if (queuedNextForPath === nextPlayback.current_path && queuedNextPath) {
      return;
    }

    let nextSong = pickNextSong(nextPlayback.current_path);

    // Same fallback as handleTrackEnded: if context is done, peek into full library
    if (!nextSong && queueStore.hasContext() && repeatMode === 'off') {
      const fullLibrary = songs.filter((s) => !queueRemovedPathSet.has(s.path) || s.path === nextPlayback.current_path);
      const currentIndexInLibrary = fullLibrary.findIndex((s) => s.path === nextPlayback.current_path);
      if (currentIndexInLibrary >= 0 && currentIndexInLibrary < fullLibrary.length - 1) {
        nextSong = fullLibrary[currentIndexInLibrary + 1];
      }
    }

    if (!nextSong) {
      return;
    }

    queuedNextForPath = nextPlayback.current_path;
    queuedNextPath = nextSong.path;

    try {
      await queueNextPlayback(nextSong.path);
    } catch {
      queuedNextForPath = null;
      queuedNextPath = null;
    }
  }

  function pickNextSong(currentPath: string) {
    return queueStore.pickNext(orderedPlaybackSongs, currentPath, shuffleEnabled, repeatMode);
  }


  async function handleTrackEnded(path: string) {
    let nextSong = pickNextSong(path);

    // If the context (artist/album) is exhausted and repeat is off,
    // fall back to the full library and continue from the next song.
    if (!nextSong && queueStore.hasContext() && repeatMode === 'off') {
      const fullLibrary = songs.filter((s) => !queueRemovedPathSet.has(s.path) || s.path === path);
      const currentIndexInLibrary = fullLibrary.findIndex((s) => s.path === path);
      if (currentIndexInLibrary >= 0 && currentIndexInLibrary < fullLibrary.length - 1) {
        queueStore.clearContext();
        nextSong = fullLibrary[currentIndexInLibrary + 1];
      }
    }

    if (nextSong) {
      await chooseSong(nextSong);
    }
  }

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
      const image = new Image();
      image.crossOrigin = 'anonymous';
      image.src = src;
      await image.decode();

      const canvas = document.createElement('canvas');
      canvas.width = 48;
      canvas.height = 48;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      if (!context) {
        return;
      }

      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let r = 0;
      let g = 0;
      let b = 0;
      let count = 0;

      for (let index = 0; index < pixels.length; index += 16) {
        const red = pixels[index];
        const green = pixels[index + 1];
        const blue = pixels[index + 2];
        const max = Math.max(red, green, blue);
        const min = Math.min(red, green, blue);
        const brightness = (red + green + blue) / 3;

        if (max - min > 18 && brightness > 34 && brightness < 232) {
          r += red;
          g += green;
          b += blue;
          count += 1;
        }
      }

      if (count > 0) {
        accentRgb = `${Math.round(r / count)},${Math.round(g / count)},${Math.round(b / count)}`;
      }
    } catch {
      sampledArtwork = null;
      accentRgb = defaultAccentRgb;
    }
  }

  async function addFolder() {
    status = 'Scanning folder...';
    try {
      const snapshot = await libraryStore.addFolder();
      applyLibrarySnapshot(snapshot);
      status = `${snapshot.songs.length} tracks loaded`;
    } catch (error) {
      status = error instanceof Error ? error.message : 'Scan cancelled';
    }
  }

  async function refreshLibrary() {
    status = 'Refreshing library...';
    try {
      const snapshot = await libraryStore.rescan();
      applyLibrarySnapshot(snapshot);
      status = `${snapshot.songs.length} tracks loaded`;
      if (autoFetchArtwork) {
        void fetchAllMissingArtwork();
      }
    } catch (error) {
      status = error instanceof Error ? error.message : 'Refresh failed';
    }
  }

  async function addPlaylist(name: string) {
    libraryStore.setPlaylists(await createPlaylist(name));
  }

  async function removeScanRoot(root: string) {
    status = 'Removing folder...';
    try {
      const snapshot = await libraryStore.removeScanRoot(root);
      applyLibrarySnapshot(snapshot);
      status = `${snapshot.songs.length} tracks loaded`;
    } catch (error) {
      status = error instanceof Error ? error.message : 'Could not remove folder';
    }
  }

  async function renameExistingPlaylist(playlistId: number, name: string) {
    libraryStore.setPlaylists(await renamePlaylist(playlistId, name));
    status = `Renamed playlist to ${name}`;
  }

  async function deleteExistingPlaylist(playlistId: number) {
    libraryStore.setPlaylists(await deletePlaylist(playlistId));
    status = 'Deleted playlist';
  }

  async function handleChoosePlaylistCover(playlistId: number) {
    libraryStore.setPlaylists(await choosePlaylistCover(playlistId));
    status = 'Updated playlist cover';
  }

  async function handleRemovePlaylistCover(playlistId: number) {
    libraryStore.setPlaylists(await removePlaylistCover(playlistId));
    status = 'Removed playlist cover';
  }

  async function handleFetchArtistArtworkManual(artistName: string) {
    try {
      const snapshot = await fetchArtistArtworkManual(artistName);
      applyLibrarySnapshot(snapshot);
      status = 'Fetched artist artwork';
    } catch (e: any) {
      status = `Error: ${e}`;
      console.error(e);
    }
  }

  async function handleFetchAlbumArtworkManual(albumKey: string, artist: string, album: string) {
    try {
      const snapshot = await fetchAlbumArtworkManual(albumKey, artist, album);
      applyLibrarySnapshot(snapshot);
      status = 'Fetched album artwork';
    } catch (e: any) {
      status = `Error: ${e}`;
      console.error(e);
    }
  }

  async function chooseExistingArtistCover(artistName: string) {
    const snapshot = await chooseArtistCover(artistName);
    applyLibrarySnapshot(snapshot);
    status = 'Updated artist cover';
  }

  async function clearExistingArtistCover(artistName: string) {
    const snapshot = await removeArtistCover(artistName);
    applyLibrarySnapshot(snapshot);
    status = 'Removed artist cover';
  }

  async function chooseExistingAlbumCover(albumKey: string) {
    const snapshot = await chooseAlbumCover(albumKey);
    applyLibrarySnapshot(snapshot);
    status = 'Updated album cover';
  }

  async function clearExistingAlbumCover(albumKey: string) {
    const snapshot = await removeAlbumCover(albumKey);
    applyLibrarySnapshot(snapshot);
    status = 'Removed album cover';
  }

  async function loadPlaylistSongs(playlistId: number) {
    return playlistSongIds(playlistId);
  }

  async function addToPlaylist(playlistId: number, song: LocalSong) {
    if (song.id === null) {
      status = 'Song is not saved in the library yet';
      return;
    }

    const updatedPlaylists = await addSongToPlaylist(playlistId, song.id);
    libraryStore.setPlaylists(updatedPlaylists);
    const playlist = updatedPlaylists.find((item) => item.id === playlistId);
    status = playlist ? `Added to ${playlist.name}` : 'Added to playlist';
  }

  function editSongMetadata(song: LocalSong) {
    metadataEditorSong = song;
  }

  async function saveSongMetadata(update: SongMetadataUpdate) {
    isSavingMetadata = true;
    status = 'Saving metadata...';
    try {
      const snapshot = await updateSongMetadata(update);
      applyLibrarySnapshot(snapshot);
      metadataEditorSong = null;
      status = 'Updated song metadata';
    } catch (error) {
      status = error instanceof Error ? error.message : 'Could not save metadata';
    } finally {
      isSavingMetadata = false;
    }
  }

  async function replaceSongCover(song: LocalSong) {
    isSavingMetadata = true;
    status = 'Choosing cover...';
    try {
      const snapshot = await chooseSongCover(song.path);
      applyLibrarySnapshot(snapshot);
      status = 'Updated song cover';
    } catch (error) {
      status = error instanceof Error ? error.message : 'Cover change cancelled';
    } finally {
      isSavingMetadata = false;
    }
  }

  async function clearSongCover(song: LocalSong) {
    isSavingMetadata = true;
    status = 'Removing cover...';
    try {
      const snapshot = await removeSongCover(song.path);
      applyLibrarySnapshot(snapshot);
      status = 'Removed song cover';
    } catch (error) {
      status = error instanceof Error ? error.message : 'Could not remove cover';
    } finally {
      isSavingMetadata = false;
    }
  }

  async function removeFromPlaylist(playlistId: number, song: LocalSong) {
    if (song.id === null) {
      status = 'Song is not saved in the library yet';
      return;
    }

    libraryStore.setPlaylists(await removeSongFromPlaylist(playlistId, song.id));
    status = `Removed ${song.title} from playlist`;
  }

  async function chooseSong(song: LocalSong, contextSongs?: LocalSong[]) {
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    selectedPath = song.path;
    if (contextSongs && contextSongs.length) {
      queueStore.setContext(contextSongs, song.path);
    } else {
      queueStore.recordPlayed(song.path);
    }
    try {
      await playbackStore.play(song.path);
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function playSongByOffset(offset: number) {
    const currentPath = playback.current_path ?? selectedPath;
    const currentIndex = orderedPlaybackSongs.findIndex((song) => song.path === currentPath);
    if (currentIndex < 0 || orderedPlaybackSongs.length === 0) {
      return;
    }

    if (orderedPlaybackSongs.length === 1) {
      return;
    }

    if (shuffleEnabled && orderedPlaybackSongs.length > 1) {
      // In shuffle mode: pick a random unplayed song.
      // If all songs have been played and repeat is off, stop.
      const unplayed = orderedPlaybackSongs.filter((s) => !queueState.shufflePlayedPaths.has(s.path));
      if (unplayed.length === 0) {
        if (repeatMode === 'off') {
          return; // album/context is over
        }
        // repeat all: reset and play again
        queueStore.resetShuffle(currentPath ?? '');
        const candidates = orderedPlaybackSongs.filter((s) => s.path !== currentPath);
        if (!candidates.length) return;
        const next = candidates[Math.floor(Math.random() * candidates.length)];
        await chooseSong(next);
        return;
      }
      const next = unplayed[Math.floor(Math.random() * unplayed.length)];
      await chooseSong(next);
      return;
    }

    let nextIndex = currentIndex + offset;
    
    // If skipping past the end of the context (artist/album) and repeat is off,
    // transition to the next song in the full library instead of looping back.
    if (nextIndex >= orderedPlaybackSongs.length && queueStore.hasContext() && repeatMode === 'off') {
      const fullLibrary = songs.filter((s) => !queueRemovedPathSet.has(s.path) || s.path === currentPath);
      const currentIndexInLibrary = fullLibrary.findIndex((s) => s.path === currentPath);
      if (currentIndexInLibrary >= 0 && currentIndexInLibrary < fullLibrary.length - 1) {
        queueStore.clearContext();
        await chooseSong(fullLibrary[currentIndexInLibrary + 1]);
        return;
      }
    }

    // Wrap around for Prev button, or Next button if repeat is 'all' or no context is set
    nextIndex = (nextIndex + orderedPlaybackSongs.length) % orderedPlaybackSongs.length;
    await chooseSong(orderedPlaybackSongs[nextIndex]);
  }

  async function playPreviousSong() {
    await playSongByOffset(-1);
  }

  async function playNextSong() {
    await playSongByOffset(1);
  }

  async function togglePlayback() {
    if (!playback.current_path && selectedSong) {
      await chooseSong(selectedSong);
      return;
    }

    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    await (playback.is_playing ? playbackStore.pause() : playbackStore.resume());
  }

  async function seek(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    await playbackStore.seek(Number(target.value));
  }

  async function seekToPosition(positionMs: number) {
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    await playbackStore.seek(positionMs);
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
