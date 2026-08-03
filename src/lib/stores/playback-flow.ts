import { queueNextPlayback } from '../tauri';
import type { LocalSong, PlaybackState } from '../types';

type RepeatMode = 'off' | 'all' | 'one';

type PlaybackStore = {
  set: (state: PlaybackState) => void;
  play: (path: string) => Promise<PlaybackState>;
  pause: () => Promise<PlaybackState>;
  resume: () => Promise<PlaybackState>;
  seek: (positionMs: number) => Promise<PlaybackState>;
};

type QueueStore = {
  setContext: (songs: LocalSong[], selectedPath: string) => void;
  recordPlayed: (path: string) => void;
  resetShuffle: (path: string) => void;
  clearContext: () => void;
  hasContext: () => boolean;
  pickNext: (
    songs: LocalSong[],
    currentPath: string,
    shuffleEnabled: boolean,
    repeatMode: RepeatMode
  ) => LocalSong | null;
};

type PlaybackFlowOptions = {
  playbackStore: PlaybackStore;
  queueStore: QueueStore;
  getPlayback: () => PlaybackState;
  getSongs: () => LocalSong[];
  getSelectedSong: () => LocalSong | null;
  getSelectedPath: () => string | null;
  setSelectedPath: (path: string | null) => void;
  getOrderedPlaybackSongs: () => LocalSong[];
  getQueueOrderPaths: () => string[];
  getQueueRemovedPathSet: () => Set<string>;
  getShufflePlayedPathSet: () => Set<string>;
  getGaplessPlayback: () => boolean;
  getShuffleEnabled: () => boolean;
  getRepeatMode: () => RepeatMode;
  onPlaybackStateChange: (isPlaying: boolean) => void;
  onPlaybackError: (error: unknown) => void;
};

export function createPlaybackFlow(options: PlaybackFlowOptions) {
  let isHandlingTrackEnd = false;
  let handledEndedPath: string | null = null;
  let queuedNextForPath: string | null = null;
  let queuedNextPath: string | null = null;

  function resetTrackEndState() {
    handledEndedPath = null;
    queuedNextForPath = null;
    queuedNextPath = null;
  }

  function pickNextSong(currentPath: string) {
    return options.queueStore.pickNext(
      options.getOrderedPlaybackSongs(),
      currentPath,
      options.getShuffleEnabled(),
      options.getRepeatMode()
    );
  }

  function nextSongFromLibrary(currentPath: string) {
    if (!options.queueStore.hasContext() || options.getRepeatMode() !== 'off') {
      return null;
    }

    const songs = options.getSongs().filter((song) => {
      return !options.getQueueRemovedPathSet().has(song.path) || song.path === currentPath;
    });
    const currentIndex = songs.findIndex((song) => song.path === currentPath);
    return currentIndex >= 0 && currentIndex < songs.length - 1 ? songs[currentIndex + 1] : null;
  }

  async function chooseSong(song: LocalSong, contextSongs?: LocalSong[]) {
    resetTrackEndState();
    options.setSelectedPath(song.path);

    if (contextSongs?.length) {
      options.queueStore.setContext(contextSongs, song.path);
    } else {
      options.queueStore.recordPlayed(song.path);
    }

    try {
      await options.playbackStore.play(song.path);
    } catch (error) {
      options.onPlaybackError(error);
    }
  }

  async function handlePlaybackSnapshot(nextPlayback: PlaybackState) {
    const previousPlayback = options.getPlayback();
    options.playbackStore.set(nextPlayback);

    if (previousPlayback.current_path !== nextPlayback.current_path) {
      options.setSelectedPath(nextPlayback.current_path);
      resetTrackEndState();

      if (
        nextPlayback.current_path &&
        options.queueStore.hasContext() &&
        !options.getQueueOrderPaths().includes(nextPlayback.current_path)
      ) {
        options.queueStore.clearContext();
      }
    } else if (previousPlayback.is_playing && previousPlayback.position_ms > nextPlayback.position_ms + 1000) {
      resetTrackEndState();
    }

    if (previousPlayback.is_playing !== nextPlayback.is_playing) {
      options.onPlaybackStateChange(nextPlayback.is_playing);
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
    if (
      !options.getGaplessPlayback() ||
      !nextPlayback.current_path ||
      !nextPlayback.is_playing ||
      nextPlayback.duration_ms <= 0
    ) {
      return;
    }

    const remainingMs = nextPlayback.duration_ms - nextPlayback.position_ms;
    if (remainingMs > 5000 || remainingMs < 0) {
      return;
    }

    if (queuedNextForPath === nextPlayback.current_path && queuedNextPath) {
      return;
    }

    const nextSong = pickNextSong(nextPlayback.current_path) ?? nextSongFromLibrary(nextPlayback.current_path);
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

  async function handleTrackEnded(path: string) {
    let nextSong = pickNextSong(path);
    if (!nextSong) {
      nextSong = nextSongFromLibrary(path);
      if (nextSong) {
        options.queueStore.clearContext();
      }
    }

    if (nextSong) {
      await chooseSong(nextSong);
    }
  }

  async function playSongByOffset(offset: number) {
    const currentPath = options.getPlayback().current_path ?? options.getSelectedPath();
    const songs = options.getOrderedPlaybackSongs();
    const currentIndex = songs.findIndex((song) => song.path === currentPath);
    if (currentIndex < 0 || songs.length <= 1) {
      return;
    }

    if (options.getShuffleEnabled()) {
      const unplayed = songs.filter((song) => !options.getShufflePlayedPathSet().has(song.path));
      if (!unplayed.length) {
        if (options.getRepeatMode() === 'off') {
          return;
        }
        options.queueStore.resetShuffle(currentPath ?? '');
        const candidates = songs.filter((song) => song.path !== currentPath);
        if (!candidates.length) {
          return;
        }
        await chooseSong(candidates[Math.floor(Math.random() * candidates.length)]);
        return;
      }
      await chooseSong(unplayed[Math.floor(Math.random() * unplayed.length)]);
      return;
    }

    let nextIndex = currentIndex + offset;
    if (nextIndex >= songs.length) {
      const nextSong = nextSongFromLibrary(currentPath ?? '');
      if (nextSong) {
        options.queueStore.clearContext();
        await chooseSong(nextSong);
        return;
      }
    }

    nextIndex = (nextIndex + songs.length) % songs.length;
    await chooseSong(songs[nextIndex]);
  }

  async function togglePlayback() {
    const playback = options.getPlayback();
    if (!playback.current_path) {
      const selectedSong = options.getSelectedSong();
      if (selectedSong) {
        await chooseSong(selectedSong);
      }
      return;
    }

    resetTrackEndState();
    await (playback.is_playing ? options.playbackStore.pause() : options.playbackStore.resume());
  }

  async function seek(positionMs: number) {
    resetTrackEndState();
    await options.playbackStore.seek(positionMs);
  }

  function clearQueuedNext() {
    queuedNextForPath = null;
    queuedNextPath = null;
  }

  return {
    chooseSong,
    handlePlaybackSnapshot,
    playPreviousSong: () => playSongByOffset(-1),
    playNextSong: () => playSongByOffset(1),
    togglePlayback,
    seek,
    clearQueuedNext
  };
}
