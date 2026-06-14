<script lang="ts">
  import { onMount } from 'svelte';
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
    createPlaylist,
    deletePlaylist,
    getLibrarySnapshot,
    libraryFolderCount,
    libraryScanRoots,
    pausePlayback,
    pickAndScanFolder,
    playSong,
    playbackSnapshot,
    queueNextPlayback,
    removeLibraryScanRoot,
    playlistSongIds,
    removePlaylistCover,
    removeSongCover,
    removeSongFromPlaylist,
    renamePlaylist,
    rescanLibrary,
    resumePlayback,
    seekPlayback,
    setVolume,
    updateSongMetadata
  } from './lib/tauri';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { ActiveView } from './lib/navigation';
  import type { LibrarySnapshot, LocalSong, PlaybackState, Playlist, SongMetadataUpdate } from './lib/types';

  let songs: LocalSong[] = [];
  let playlists: Playlist[] = [];
  let playback: PlaybackState = {
    current_path: null,
    position_ms: 0,
    duration_ms: 0,
    is_playing: false,
    volume: 1
  };
  let query = '';
  let activeView: ActiveView = 'songs';
  let isScanning = false;
  let status = 'Ready';
  let selectedPath: string | null = null;
  let fullPlayerOpen = false;
  let fullPlayerLyricsOpen = false;
  let queueOpen = false;
  let accentRgb = '245,245,245';
  let sampledArtwork: string | null = null;
  let playerPlacement: 'right' | 'bottom' = 'right';
  let seekbarStyle: 'standard' | 'waveform' = 'standard';
  let dynamicCoverAccent = true;
  let blurredNowPlayingBackground = true;
  let fontFamily = 'Plus Jakarta Sans';
  let fontSizePercent = 100;
  let showQualityInfo = true;
  let theme: 'default' = 'default';
  let shuffleEnabled = false;
  let repeatMode: 'off' | 'all' | 'one' = 'off';
  let queueOrderPaths: string[] = [];
  let metadataEditorSong: LocalSong | null = null;
  let isSavingMetadata = false;
  let folderCount = 0;
  let scanRoots: string[] = [];
  let isPollingPlayback = false;
  let isHandlingTrackEnd = false;
  let handledEndedPath: string | null = null;
  let queuedNextForPath: string | null = null;
  let queuedNextPath: string | null = null;
  $: bottomRowSize = '96px';
  $: defaultAccentRgb = '245,245,245';
  $: effectiveAccentRgb = dynamicCoverAccent && sampledArtwork ? accentRgb : defaultAccentRgb;

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
  $: orderedPlaybackSongs = orderSongsForQueue(songs, queueOrderPaths);
  $: queueSongs = buildQueueSongs(orderedPlaybackSongs, playback.current_path ?? selectedPath, repeatMode);
  $: albumCount = new Set(songs.map((song) => `${song.album_artist}:${song.album}`)).size;
  $: artistCount = new Set(songs.map((song) => song.artist)).size;
  $: ambientArtwork = artworkUrl((nowPlaying ?? selectedSong)?.artwork_preview ?? (nowPlaying ?? selectedSong)?.artwork ?? null);
  $: shellStyle = [
    `--cover-art: ${ambientArtwork ? `url("${ambientArtwork}")` : 'none'}`,
    `--accent: rgb(${effectiveAccentRgb})`,
    `--accent-soft: rgba(${effectiveAccentRgb}, 0.18)`,
    `--accent-mid: rgba(${effectiveAccentRgb}, 0.34)`,
    `font-family: ${fontStack(fontFamily)}`
  ].join('; ');
  $: if (dynamicCoverAccent && ambientArtwork && ambientArtwork !== sampledArtwork) {
    void sampleAccent(ambientArtwork);
  }
  $: applyRootFontSize(fontSizePercent);

  onMount(() => {
    playerPlacement = readPreference('orca.playerPlacement', 'right', ['right', 'bottom']);
    seekbarStyle = readPreference('orca.seekbarStyle', 'standard', ['standard', 'waveform']);
    theme = readPreference('orca.theme', 'default', ['default']);
    dynamicCoverAccent = readBooleanPreference('orca.dynamicCoverAccent', true);
    blurredNowPlayingBackground = readBooleanPreference('orca.blurredNowPlayingBackground', true);
    fontFamily = readPreference('orca.fontFamily', 'Plus Jakarta Sans', ['Plus Jakarta Sans', 'System', 'Segoe UI']);
    fontSizePercent = readNumberPreference('orca.fontSizePercent', 100, 80, 120);
    showQualityInfo = readBooleanPreference('orca.showQualityInfo', true);
    shuffleEnabled = readBooleanPreference('orca.shuffleEnabled', false);
    repeatMode = readPreference('orca.repeatMode', 'off', ['off', 'all', 'one']);

    void (async () => {
      const snapshot = await getLibrarySnapshot();
      songs = snapshot.songs;
      playlists = snapshot.playlists;
      playback = snapshot.playback;
      folderCount = snapshot.folder_count ?? 0;
      scanRoots = await libraryScanRoots();
      status = songs.length ? `${songs.length} tracks loaded` : 'Add a folder to build your library';
    })();

    const timer = window.setInterval(async () => {
      if (isPollingPlayback) {
        return;
      }

      isPollingPlayback = true;
      try {
        await handlePlaybackSnapshot(await playbackSnapshot());
      } finally {
        isPollingPlayback = false;
      }
    }, 500);

    window.addEventListener('keydown', handleKeydown);

    return () => {
      window.clearInterval(timer);
      window.removeEventListener('keydown', handleKeydown);
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
      return 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
    }

    if (value === 'Segoe UI') {
      return '"Segoe UI", ui-sans-serif, system-ui, sans-serif';
    }

    return '"Plus Jakarta Sans", ui-sans-serif, system-ui, sans-serif';
  }

  function applyRootFontSize(value: number) {
    if (typeof document === 'undefined') {
      return;
    }

    document.documentElement.style.fontSize = `${16 * (value / 100)}px`;
  }

  function setPlayerPlacement(placement: 'right' | 'bottom') {
    playerPlacement = placement;
    window.localStorage.setItem('orca.playerPlacement', placement);
  }

  function setSeekbarStyle(style: 'standard' | 'waveform') {
    seekbarStyle = style;
    window.localStorage.setItem('orca.seekbarStyle', style);
  }

  function setTheme(value: 'default') {
    theme = value;
    window.localStorage.setItem('orca.theme', value);
    if (!sampledArtwork) {
      accentRgb = '245,245,245';
    }
  }

  function setDynamicCoverAccent(enabled: boolean) {
    dynamicCoverAccent = enabled;
    window.localStorage.setItem('orca.dynamicCoverAccent', String(enabled));
    if (!enabled) {
      sampledArtwork = null;
      accentRgb = '245,245,245';
    } else if (ambientArtwork) {
      sampledArtwork = null;
      void sampleAccent(ambientArtwork);
    }
  }

  function setBlurredNowPlayingBackground(enabled: boolean) {
    blurredNowPlayingBackground = enabled;
    window.localStorage.setItem('orca.blurredNowPlayingBackground', String(enabled));
  }

  function setFontFamily(value: string) {
    fontFamily = value;
    window.localStorage.setItem('orca.fontFamily', value);
  }

  function setFontSizePercent(value: number) {
    fontSizePercent = Math.min(120, Math.max(80, Math.round(value)));
    window.localStorage.setItem('orca.fontSizePercent', String(fontSizePercent));
  }

  function setShowQualityInfo(enabled: boolean) {
    showQualityInfo = enabled;
    window.localStorage.setItem('orca.showQualityInfo', String(enabled));
  }

  function toggleShuffle() {
    shuffleEnabled = !shuffleEnabled;
    window.localStorage.setItem('orca.shuffleEnabled', String(shuffleEnabled));
  }

  function cycleRepeat() {
    repeatMode = repeatMode === 'off' ? 'all' : repeatMode === 'all' ? 'one' : 'off';
    window.localStorage.setItem('orca.repeatMode', repeatMode);
  }

  function orderSongsForQueue(sourceSongs: LocalSong[], orderPaths: string[]) {
    if (!orderPaths.length) {
      return sourceSongs;
    }

    const songsByPath = new Map(sourceSongs.map((song) => [song.path, song]));
    const orderedSongs = orderPaths
      .map((path) => songsByPath.get(path))
      .filter((song): song is LocalSong => Boolean(song));
    const orderedPaths = new Set(orderedSongs.map((song) => song.path));
    const missingSongs = sourceSongs.filter((song) => !orderedPaths.has(song.path));
    return [...orderedSongs, ...missingSongs];
  }

  function buildQueueSongs(sourceSongs: LocalSong[], currentPath: string | null, mode: 'off' | 'all' | 'one') {
    if (!sourceSongs.length) {
      return [];
    }

    const currentIndex = currentPath ? sourceSongs.findIndex((song) => song.path === currentPath) : -1;
    if (currentIndex < 0) {
      return sourceSongs;
    }

    if (mode === 'one') {
      return [sourceSongs[currentIndex]];
    }

    const currentAndRemaining = sourceSongs.slice(currentIndex);
    if (mode !== 'all') {
      return currentAndRemaining;
    }

    return [...currentAndRemaining, ...sourceSongs.slice(0, currentIndex)];
  }

  function toggleQueue() {
    queueOpen = !queueOpen;
  }

  function reorderQueueSong(sourcePath: string, targetPath: string) {
    if (sourcePath === targetPath) {
      return;
    }

    const currentPath = playback.current_path ?? selectedPath;
    if (sourcePath === currentPath) {
      return;
    }

    const orderedPaths = orderSongsForQueue(songs, queueOrderPaths).map((song) => song.path);
    const sourceIndex = orderedPaths.indexOf(sourcePath);
    if (sourceIndex < 0) {
      return;
    }

    const nextOrder = orderedPaths.filter((path) => path !== sourcePath);
    const targetIndex = nextOrder.indexOf(targetPath);
    if (targetIndex < 0) {
      return;
    }

    const insertIndex = targetPath === currentPath ? targetIndex + 1 : targetIndex;
    nextOrder.splice(insertIndex, 0, sourcePath);
    queueOrderPaths = nextOrder;
  }

  function applyLibrarySnapshot(snapshot: LibrarySnapshot) {
    songs = snapshot.songs;
    playlists = snapshot.playlists;
    playback = snapshot.playback;
    folderCount = snapshot.folder_count ?? folderCount;
    queueOrderPaths = queueOrderPaths.filter((path) => songs.some((song) => song.path === path));

    if (metadataEditorSong) {
      metadataEditorSong = songs.find((song) => song.path === metadataEditorSong?.path) ?? metadataEditorSong;
    }
  }

  async function handlePlaybackSnapshot(nextPlayback: PlaybackState) {
    const previousPlayback = playback;
    playback = nextPlayback;

    if (previousPlayback.current_path !== nextPlayback.current_path) {
      selectedPath = nextPlayback.current_path;
      queuedNextForPath = null;
      queuedNextPath = null;
      handledEndedPath = null;
    } else if (previousPlayback.is_playing && previousPlayback.position_ms > nextPlayback.position_ms + 1000) {
      queuedNextForPath = null;
      queuedNextPath = null;
      handledEndedPath = null;
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
    if (!nextPlayback.current_path || !nextPlayback.is_playing || nextPlayback.duration_ms <= 0) {
      return;
    }

    const remainingMs = nextPlayback.duration_ms - nextPlayback.position_ms;
    if (remainingMs > 5000 || remainingMs < 0) {
      return;
    }

    if (queuedNextForPath === nextPlayback.current_path && queuedNextPath) {
      return;
    }

    const nextSong = pickNextSong(nextPlayback.current_path);
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
    const currentIndex = orderedPlaybackSongs.findIndex((song) => song.path === currentPath);
    if (currentIndex < 0 || orderedPlaybackSongs.length === 0) {
      return null;
    }

    if (repeatMode === 'one') {
      return orderedPlaybackSongs[currentIndex];
    }

    if (shuffleEnabled && orderedPlaybackSongs.length > 1) {
      let nextIndex = currentIndex;
      while (nextIndex === currentIndex) {
        nextIndex = Math.floor(Math.random() * orderedPlaybackSongs.length);
      }
      return orderedPlaybackSongs[nextIndex];
    }

    const isLastSong = currentIndex >= orderedPlaybackSongs.length - 1;
    if (isLastSong && repeatMode !== 'all') {
      return null;
    }

    return orderedPlaybackSongs[(currentIndex + 1) % orderedPlaybackSongs.length];
  }

  async function handleTrackEnded(path: string) {
    const nextSong = pickNextSong(path);
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

    if (fullPlayerOpen && key === 'l' && !event.altKey && !event.ctrlKey && !event.metaKey && !isTextEntryTarget(event)) {
      event.preventDefault();
      fullPlayerLyricsOpen = !fullPlayerLyricsOpen;
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

    if (shouldIgnorePlaybackShortcut(event)) {
      return;
    }

    if (event.altKey && key === 'n') {
      event.preventDefault();
      await playNextSong();
      return;
    }

    if (event.altKey && key === 'p') {
      event.preventDefault();
      await playPreviousSong();
      return;
    }

    if (event.code === 'Space' && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      await togglePlayback();
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
    isScanning = true;
    status = 'Scanning folder...';
    try {
      songs = await pickAndScanFolder();
      folderCount = await libraryFolderCount();
      scanRoots = await libraryScanRoots();
      status = `${songs.length} tracks loaded`;
    } catch (error) {
      status = error instanceof Error ? error.message : 'Scan cancelled';
    } finally {
      isScanning = false;
    }
  }

  async function refreshLibrary() {
    isScanning = true;
    status = 'Refreshing library...';
    try {
      songs = await rescanLibrary();
      folderCount = await libraryFolderCount();
      scanRoots = await libraryScanRoots();
      status = `${songs.length} tracks loaded`;
    } catch (error) {
      status = error instanceof Error ? error.message : 'Refresh failed';
    } finally {
      isScanning = false;
    }
  }

  async function addPlaylist(name: string) {
    playlists = await createPlaylist(name);
  }

  async function removeScanRoot(root: string) {
    isScanning = true;
    status = 'Removing folder...';
    try {
      const snapshot = await removeLibraryScanRoot(root);
      applyLibrarySnapshot(snapshot);
      scanRoots = await libraryScanRoots();
      status = `${snapshot.songs.length} tracks loaded`;
    } catch (error) {
      status = error instanceof Error ? error.message : 'Could not remove folder';
    } finally {
      isScanning = false;
    }
  }

  async function renameExistingPlaylist(playlistId: number, name: string) {
    playlists = await renamePlaylist(playlistId, name);
    status = `Renamed playlist to ${name}`;
  }

  async function deleteExistingPlaylist(playlistId: number) {
    playlists = await deletePlaylist(playlistId);
    status = 'Deleted playlist';
  }

  async function chooseExistingPlaylistCover(playlistId: number) {
    playlists = await choosePlaylistCover(playlistId);
    status = 'Updated playlist cover';
  }

  async function clearExistingPlaylistCover(playlistId: number) {
    playlists = await removePlaylistCover(playlistId);
    status = 'Removed playlist cover';
  }

  async function loadPlaylistSongs(playlistId: number) {
    return playlistSongIds(playlistId);
  }

  async function addToPlaylist(playlistId: number, song: LocalSong) {
    if (song.id === null) {
      status = 'Song is not saved in the library yet';
      return;
    }

    playlists = await addSongToPlaylist(playlistId, song.id);
    const playlist = playlists.find((item) => item.id === playlistId);
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

    playlists = await removeSongFromPlaylist(playlistId, song.id);
    status = `Removed ${song.title} from playlist`;
  }

  async function chooseSong(song: LocalSong) {
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    selectedPath = song.path;
    playback = await playSong(song.path);
  }

  async function playSongByOffset(offset: number) {
    const currentPath = playback.current_path ?? selectedPath;
    const currentIndex = orderedPlaybackSongs.findIndex((song) => song.path === currentPath);
    if (currentIndex < 0 || orderedPlaybackSongs.length === 0) {
      return;
    }

    let nextIndex = (currentIndex + offset + orderedPlaybackSongs.length) % orderedPlaybackSongs.length;
    if (shuffleEnabled && orderedPlaybackSongs.length > 1) {
      do {
        nextIndex = Math.floor(Math.random() * orderedPlaybackSongs.length);
      } while (nextIndex === currentIndex);
    }
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
    playback = playback.is_playing ? await pausePlayback() : await resumePlayback();
  }

  async function seek(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    playback = await seekPlayback(Number(target.value));
  }

  async function seekToPosition(positionMs: number) {
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
    playback = await seekPlayback(positionMs);
  }

  async function changeVolume(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    playback = await setVolume(Number(target.value));
  }
</script>

<svelte:head>
  <title>Orca</title>
</svelte:head>

<svelte:window on:contextmenu={suppressNativeContextMenu} />

<main class="relative h-screen overflow-hidden bg-[#090a0c] text-[#f4f4f5]" style={shellStyle}>
  <AppBackdrop {shellStyle} blurredBackground={blurredNowPlayingBackground} />

  <div
    class={`relative grid h-full ${playerPlacement === 'right' ? 'grid-cols-[132px_minmax(0,1fr)_250px] grid-rows-[1fr] max-xl:grid-cols-[132px_minmax(0,1fr)]' : 'grid-cols-[132px_minmax(0,1fr)]'} max-md:grid-cols-1`}
    style={playerPlacement === 'right' ? undefined : `grid-template-rows: minmax(0, 1fr) ${bottomRowSize};`}
  >
    <Sidebar {activeView} {isScanning} {folderCount} onSelect={(view) => (activeView = view)} onAddFolder={addFolder} onRefresh={refreshLibrary} />
    <LibraryView
      {activeView}
      {songs}
      {playlists}
      {filteredSongs}
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
      onChoosePlaylistCover={chooseExistingPlaylistCover}
      onRemovePlaylistCover={clearExistingPlaylistCover}
      onRemoveSongFromPlaylist={removeFromPlaylist}
      onEditSong={editSongMetadata}
      {playerPlacement}
      onPlayerPlacementChange={setPlayerPlacement}
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
      {theme}
      onThemeChange={setTheme}
    />
    {#if playerPlacement === 'right'}
      <DetailsPanel
        song={nowPlaying ?? selectedSong}
        {playback}
        {status}
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
        onOpenFullPlayer={() => (fullPlayerOpen = true)}
      />
    {/if}
    <div class={playerPlacement === 'bottom' ? 'contents' : 'hidden max-xl:contents'}>
      <PlayerBar
        {nowPlaying}
        {playback}
        {status}
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
    />
    <MetadataEditor
      open={Boolean(metadataEditorSong)}
      song={metadataEditorSong}
      isSaving={isSavingMetadata}
      onClose={() => (metadataEditorSong = null)}
      onSave={saveSongMetadata}
      onReplaceCover={replaceSongCover}
      onRemoveCover={clearSongCover}
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
    />
  </div>
</main>
