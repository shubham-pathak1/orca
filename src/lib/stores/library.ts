import { writable } from 'svelte/store';

import {
  getLibrarySnapshot,
  libraryScanRoots,
  pickAndScanFolder,
  removeLibraryScanRoot,
  rescanLibrary
} from '../tauri';
import type { AlbumEntry, ArtistEntry, LibrarySnapshot, LocalSong, Playlist } from '../types';

type LibraryState = {
  songs: LocalSong[];
  playlists: Playlist[];
  artists: ArtistEntry[];
  albums: AlbumEntry[];
  folderCount: number;
  scanRoots: string[];
  isScanning: boolean;
};

const initialState: LibraryState = {
  songs: [],
  playlists: [],
  artists: [],
  albums: [],
  folderCount: 0,
  scanRoots: [],
  isScanning: false
};

export function createLibraryStore() {
  const { subscribe, set } = writable(initialState);
  let state = initialState;

  function setState(nextState: LibraryState) {
    state = nextState;
    set(nextState);
  }

  async function refreshScanRoots() {
    setState({ ...state, scanRoots: await libraryScanRoots() });
  }

  function applySnapshot(snapshot: LibrarySnapshot) {
    setState({
      ...state,
      songs: snapshot.songs,
      playlists: snapshot.playlists,
      artists: snapshot.artists ?? [],
      albums: snapshot.albums ?? [],
      folderCount: snapshot.folder_count ?? state.folderCount
    });
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

    setPlaylists(playlists: Playlist[]) {
      setState({ ...state, playlists });
    }
  };
}
