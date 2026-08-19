import { writable } from 'svelte/store';

import {
  getLibrarySnapshot,
  librarySources,
  pickAndScanFolder,
  removeLibraryScanRoot,
  rescanLibrarySource,
  rescanLibrary
} from '../tauri';
import type { LibrarySource } from '../tauri';
import type { AlbumEntry, ArtistEntry, GenreEntry, LibrarySnapshot, LocalSong, Playlist } from '../types';

type LibraryState = {
  songs: LocalSong[];
  playlists: Playlist[];
  artists: ArtistEntry[];
  albums: AlbumEntry[];
  genres: GenreEntry[];
  folderCount: number;
  scanRoots: LibrarySource[];
  isScanning: boolean;
};

const initialState: LibraryState = {
  songs: [],
  playlists: [],
  artists: [],
  albums: [],
  genres: [],
  folderCount: 0,
  scanRoots: [],
  isScanning: false
};

export function createLibraryStore() {
  const { subscribe, set } = writable(initialState);
  let state = initialState;
  let songsByPath = new Map<string, LocalSong>();

  function setState(nextState: LibraryState) {
    state = nextState;
    set(nextState);
  }

  async function refreshScanRoots() {
    setState({ ...state, scanRoots: await librarySources() });
  }

  function applySnapshot(snapshot: LibrarySnapshot) {
    songsByPath = new Map(snapshot.songs.map((song) => [song.path, song]));
    setState({
      ...state,
      songs: snapshot.songs,
      playlists: snapshot.playlists,
      artists: snapshot.artists ?? [],
      albums: snapshot.albums ?? [],
      genres: snapshot.genres ?? [],
      folderCount: snapshot.folder_count ?? state.folderCount
    });
  }

  function appendIndexedSongs(batch: LocalSong[]) {
    if (!batch.length) {
      return;
    }

    for (const incoming of batch) {
      const existing = songsByPath.get(incoming.path);
      songsByPath.set(
        incoming.path,
        existing && !incoming.artwork
          ? {
              ...incoming,
              artwork: existing.artwork,
              artwork_thumb: existing.artwork_thumb,
              artwork_preview: existing.artwork_preview,
              lyrics: existing.lyrics ?? incoming.lyrics
            }
          : incoming
      );
    }

    // Catalog views remain on their last complete snapshot until scanning finishes.
    setState({ ...state, songs: Array.from(songsByPath.values()) });
  }


  async function scan(action: () => Promise<LibrarySnapshot>) {
    setState({ ...state, isScanning: true });
    try {
      const snapshot = await action();
      applySnapshot(snapshot);
      await refreshScanRoots();
      return snapshot;
    } finally {
      setState({ ...state, isScanning: false });
    }
  }

  return {
    subscribe,
    applySnapshot,
    appendIndexedSongs,
    refreshScanRoots,

    async load() {
      const snapshot = await getLibrarySnapshot();
      applySnapshot(snapshot);
      await refreshScanRoots();
      return snapshot;
    },

    addFolder() {
      return scan(pickAndScanFolder);
    },

    rescan() {
      return scan(rescanLibrary);
    },

    removeScanRoot(root: string) {
      return scan(() => removeLibraryScanRoot(root));
    },

    rescanSource(root: string) {
      return scan(() => rescanLibrarySource(root));
    },

    setPlaylists(playlists: Playlist[]) {
      setState({ ...state, playlists });
    }
  };
}
