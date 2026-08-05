import {
  addSongToPlaylist,
  chooseAlbumCover,
  chooseArtistCover,
  choosePlaylistCover,
  chooseSongCover,
  createPlaylist,
  deletePlaylist,
  fetchAlbumArtworkManual,
  fetchArtistArtworkManual,
  fetchAllMissingArtwork,
  exportPlaylist,
  importPlaylist,
  playlistSongIds,
  removeAlbumCover,
  removeArtistCover,
  removePlaylistCover,
  removeSongCover,
  removeSongFromPlaylist,
  renamePlaylist,
  updateSongMetadata
} from '../tauri';
import type { LibrarySnapshot, LocalSong, Playlist, SongMetadataUpdate } from '../types';

type LibraryStore = {
  addFolder: () => Promise<LibrarySnapshot>;
  rescan: () => Promise<LibrarySnapshot>;
  removeScanRoot: (root: string) => Promise<LibrarySnapshot>;
  setPlaylists: (playlists: Playlist[]) => void;
};

type LibraryActionDependencies = {
  libraryStore: LibraryStore;
  applySnapshot: (snapshot: LibrarySnapshot) => void;
  getAutoFetchArtwork: () => boolean;
  setStatus: (status: string) => void;
  setMetadataEditorSong: (song: LocalSong | null) => void;
  setSavingMetadata: (saving: boolean) => void;
};

function messageFrom(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function playlistImportMessage(error: unknown): string {
  if (error === 'Playlist import cancelled') return 'Playlist import cancelled';
  if (error === 'No tracks from this playlist are in your library') {
    return 'None of this playlist\'s tracks are currently in your library';
  }
  console.error(error);
  return 'Could not import playlist';
}

export function createLibraryActions({
  libraryStore,
  applySnapshot,
  getAutoFetchArtwork,
  setStatus,
  setMetadataEditorSong,
  setSavingMetadata
}: LibraryActionDependencies) {
  async function addFolder() {
    setStatus('Scanning folder...');
    try {
      const snapshot = await libraryStore.addFolder();
      applySnapshot(snapshot);
      setStatus(`${snapshot.songs.length} tracks loaded`);
    } catch (error) {
      setStatus(messageFrom(error, 'Scan cancelled'));
    }
  }

  async function refreshLibrary() {
    setStatus('Refreshing library...');
    try {
      const snapshot = await libraryStore.rescan();
      applySnapshot(snapshot);
      setStatus(`${snapshot.songs.length} tracks loaded`);
      if (getAutoFetchArtwork()) {
        void fetchAllMissingArtwork();
      }
    } catch (error) {
      setStatus(messageFrom(error, 'Refresh failed'));
    }
  }

  async function addPlaylist(name: string) {
    libraryStore.setPlaylists(await createPlaylist(name));
  }

  async function removeScanRoot(root: string) {
    setStatus('Removing folder...');
    try {
      const snapshot = await libraryStore.removeScanRoot(root);
      applySnapshot(snapshot);
      setStatus(`${snapshot.songs.length} tracks loaded`);
    } catch (error) {
      setStatus(messageFrom(error, 'Could not remove folder'));
    }
  }

  async function renameExistingPlaylist(playlistId: number, name: string) {
    libraryStore.setPlaylists(await renamePlaylist(playlistId, name));
    setStatus(`Renamed playlist to ${name}`);
  }

  async function deleteExistingPlaylist(playlistId: number) {
    libraryStore.setPlaylists(await deletePlaylist(playlistId));
    setStatus('Deleted playlist');
  }

  async function handleChoosePlaylistCover(playlistId: number) {
    libraryStore.setPlaylists(await choosePlaylistCover(playlistId));
    setStatus('Updated playlist cover');
  }

  async function handleRemovePlaylistCover(playlistId: number) {
    libraryStore.setPlaylists(await removePlaylistCover(playlistId));
    setStatus('Removed playlist cover');
  }

  async function importExistingPlaylist() {
    setStatus('Importing playlist...');
    try {
      const result = await importPlaylist();
      libraryStore.setPlaylists(result.playlists);
      const unavailable = result.unavailable_tracks
        ? `, ${result.unavailable_tracks} unavailable`
        : '';
      const message = `Imported ${result.imported_tracks} tracks into ${result.playlist_name}${unavailable}`;
      setStatus(message);
      return message;
    } catch (error) {
      const message = playlistImportMessage(error);
      setStatus(message);
      return message;
    }
  }

  async function exportExistingPlaylist(playlistId: number) {
    setStatus('Exporting playlist...');
    try {
      const result = await exportPlaylist(playlistId);
      setStatus(`Exported ${result.exported_tracks} tracks`);
    } catch (error) {
      setStatus(messageFrom(error, 'Could not export playlist'));
    }
  }

  async function handleFetchArtistArtworkManual(artistName: string) {
    try {
      applySnapshot(await fetchArtistArtworkManual(artistName));
      setStatus('Fetched artist artwork');
    } catch (error) {
      setStatus(`Error: ${error}`);
      console.error(error);
    }
  }

  async function handleFetchAlbumArtworkManual(albumKey: string, artist: string, album: string) {
    try {
      applySnapshot(await fetchAlbumArtworkManual(albumKey, artist, album));
      setStatus('Fetched album artwork');
    } catch (error) {
      setStatus(`Error: ${error}`);
      console.error(error);
    }
  }

  async function chooseExistingArtistCover(artistName: string) {
    applySnapshot(await chooseArtistCover(artistName));
    setStatus('Updated artist cover');
  }

  async function clearExistingArtistCover(artistName: string) {
    applySnapshot(await removeArtistCover(artistName));
    setStatus('Removed artist cover');
  }

  async function chooseExistingAlbumCover(albumKey: string) {
    applySnapshot(await chooseAlbumCover(albumKey));
    setStatus('Updated album cover');
  }

  async function clearExistingAlbumCover(albumKey: string) {
    applySnapshot(await removeAlbumCover(albumKey));
    setStatus('Removed album cover');
  }

  function loadPlaylistSongs(playlistId: number) {
    return playlistSongIds(playlistId);
  }

  async function addToPlaylist(playlistId: number, song: LocalSong) {
    if (song.id === null) {
      setStatus('Song is not saved in the library yet');
      return;
    }

    const updatedPlaylists = await addSongToPlaylist(playlistId, song.id);
    libraryStore.setPlaylists(updatedPlaylists);
    const playlist = updatedPlaylists.find((item) => item.id === playlistId);
    setStatus(playlist ? `Added to ${playlist.name}` : 'Added to playlist');
  }

  function editSongMetadata(song: LocalSong) {
    setMetadataEditorSong(song);
  }

  async function saveSongMetadata(update: SongMetadataUpdate) {
    setSavingMetadata(true);
    setStatus('Saving metadata...');
    try {
      applySnapshot(await updateSongMetadata(update));
      setMetadataEditorSong(null);
      setStatus('Updated song metadata');
    } catch (error) {
      setStatus(messageFrom(error, 'Could not save metadata'));
    } finally {
      setSavingMetadata(false);
    }
  }

  async function replaceSongCover(song: LocalSong) {
    setSavingMetadata(true);
    setStatus('Choosing cover...');
    try {
      applySnapshot(await chooseSongCover(song.path));
      setStatus('Updated song cover');
    } catch (error) {
      setStatus(messageFrom(error, 'Cover change cancelled'));
    } finally {
      setSavingMetadata(false);
    }
  }

  async function clearSongCover(song: LocalSong) {
    setSavingMetadata(true);
    setStatus('Removing cover...');
    try {
      applySnapshot(await removeSongCover(song.path));
      setStatus('Removed song cover');
    } catch (error) {
      setStatus(messageFrom(error, 'Could not remove cover'));
    } finally {
      setSavingMetadata(false);
    }
  }

  async function removeFromPlaylist(playlistId: number, song: LocalSong) {
    if (song.id === null) {
      setStatus('Song is not saved in the library yet');
      return;
    }

    libraryStore.setPlaylists(await removeSongFromPlaylist(playlistId, song.id));
    setStatus(`Removed ${song.title} from playlist`);
  }

  return {
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
  };
}
