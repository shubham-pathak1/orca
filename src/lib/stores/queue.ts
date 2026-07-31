import { writable } from 'svelte/store';

import type { LocalSong } from '../types';

type RepeatMode = 'off' | 'all' | 'one';

type QueueState = {
  orderPaths: string[];
  removedPaths: string[];
  shufflePlayedPaths: Set<string>;
};

const initialState: QueueState = {
  orderPaths: [],
  removedPaths: [],
  shufflePlayedPaths: new Set()
};

export function createQueueStore() {
  const { subscribe, update } = writable(initialState);
  let state = initialState;

  function setState(nextState: QueueState) {
    state = nextState;
    update(() => nextState);
  }

  function resetShuffle(path: string) {
    setState({ ...state, shufflePlayedPaths: new Set([path]) });
  }

  function playableSongs(songs: LocalSong[], currentPath: string | null) {
    const removedPaths = new Set(state.removedPaths);
    const availableSongs = songs.filter((song) => !removedPaths.has(song.path) || song.path === currentPath);
    if (!state.orderPaths.length) {
      return availableSongs;
    }

    const songsByPath = new Map(availableSongs.map((song) => [song.path, song]));
    return state.orderPaths
      .map((path) => songsByPath.get(path))
      .filter((song): song is LocalSong => Boolean(song));
  }

  return {
    subscribe,

    setContext(contextSongs: LocalSong[], selectedPath: string) {
      setState({
        orderPaths: contextSongs.map((song) => song.path),
        removedPaths: [],
        shufflePlayedPaths: new Set([selectedPath])
      });
    },

    recordPlayed(path: string) {
      setState({ ...state, shufflePlayedPaths: new Set([...state.shufflePlayedPaths, path]) });
    },

    resetShuffle,

    clearContext() {
      setState({ ...state, orderPaths: [], removedPaths: [] });
    },

    syncSongs(songs: LocalSong[]) {
      const availablePaths = new Set(songs.map((song) => song.path));
      setState({
        ...state,
        orderPaths: state.orderPaths.filter((path) => availablePaths.has(path)),
        removedPaths: state.removedPaths.filter((path) => availablePaths.has(path))
      });
    },

    reorder(songs: LocalSong[], currentPath: string | null, sourcePath: string, targetPath: string) {
      if (sourcePath === targetPath || sourcePath === currentPath) {
        return;
      }

      const orderedPaths = playableSongs(songs, currentPath).map((song) => song.path);
      if (!orderedPaths.includes(sourcePath)) {
        return;
      }

      const nextOrder = orderedPaths.filter((path) => path !== sourcePath);
      const targetIndex = nextOrder.indexOf(targetPath);
      if (targetIndex < 0) {
        return;
      }

      nextOrder.splice(targetPath === currentPath ? targetIndex + 1 : targetIndex, 0, sourcePath);
      setState({ ...state, orderPaths: nextOrder });
    },

    remove(currentPath: string | null, path: string) {
      if (path === currentPath) {
        return;
      }

      setState({ ...state, removedPaths: Array.from(new Set([...state.removedPaths, path])) });
    },

    clear(songs: LocalSong[], currentPath: string | null) {
      setState({
        ...state,
        removedPaths: songs.map((song) => song.path).filter((path) => path !== currentPath)
      });
    },

    hasContext() {
      return state.orderPaths.length > 0;
    },

    playableSongs,

    queueSongs(songs: LocalSong[], currentPath: string | null, repeatMode: RepeatMode) {
      const playable = playableSongs(songs, currentPath);
      if (!playable.length) {
        return [];
      }

      const currentIndex = currentPath ? playable.findIndex((song) => song.path === currentPath) : -1;
      if (currentIndex < 0) {
        return playable;
      }
      if (repeatMode === 'one') {
        return [playable[currentIndex]];
      }

      const currentAndRemaining = playable.slice(currentIndex);
      return repeatMode === 'all' ? [...currentAndRemaining, ...playable.slice(0, currentIndex)] : currentAndRemaining;
    },

    pickNext(songs: LocalSong[], currentPath: string, shuffleEnabled: boolean, repeatMode: RepeatMode) {
      const currentIndex = songs.findIndex((song) => song.path === currentPath);
      if (currentIndex < 0 || !songs.length) {
        return null;
      }
      if (repeatMode === 'one') {
        return songs[currentIndex];
      }

      if (shuffleEnabled && songs.length > 1) {
        const unplayed = songs.filter((song) => !state.shufflePlayedPaths.has(song.path));
        if (!unplayed.length) {
          if (repeatMode === 'off') {
            return null;
          }
          const candidates = songs.filter((song) => song.path !== currentPath);
          if (!candidates.length) {
            return null;
          }
          resetShuffle(currentPath);
          return candidates[Math.floor(Math.random() * candidates.length)];
        }
        return unplayed[Math.floor(Math.random() * unplayed.length)];
      }

      if (currentIndex >= songs.length - 1 && repeatMode !== 'all') {
        return null;
      }
      return songs[(currentIndex + 1) % songs.length];
    }
  };
}
