import { writable } from 'svelte/store';

import {
  pausePlayback,
  playSong,
  playbackSnapshot,
  resumePlayback,
  seekPlayback,
  setVolume
} from '../tauri';
import type { PlaybackState } from '../types';

const VOLUME_PREFERENCE_KEY = 'orca.volume';

const initialPlaybackState: PlaybackState = {
  current_path: null,
  position_ms: 0,
  duration_ms: 0,
  is_playing: false,
  volume: 1
};

export function createPlaybackStore(initialState: PlaybackState = initialPlaybackState) {
  const { subscribe, set } = writable(initialState);
  let currentState = initialState;
  let preMuteVolume = initialState.volume > 0 ? initialState.volume : 1;
  let pollingTimer: number | undefined;
  let isPolling = false;

  function setState(nextState: PlaybackState) {
    currentState = nextState;
    if (nextState.volume > 0) {
      preMuteVolume = nextState.volume;
    }
    set(nextState);
  }

  async function update(action: () => Promise<PlaybackState>, persistVolume = false) {
    const nextState = await action();
    setState(nextState);
    if (persistVolume) {
      window.localStorage.setItem(VOLUME_PREFERENCE_KEY, String(nextState.volume));
    }
    return nextState;
  }

  function setPlaybackVolume(volume: number) {
    return update(() => setVolume(volume), true);
  }

  function toggleMute() {
    return setPlaybackVolume(currentState.volume > 0 ? 0 : preMuteVolume);
  }

  function adjustVolume(amount: number) {
    return setPlaybackVolume(Math.min(1, Math.max(0, currentState.volume + amount)));
  }

  async function restoreVolume() {
    const savedVolume = Number(window.localStorage.getItem(VOLUME_PREFERENCE_KEY));
    if (Number.isFinite(savedVolume) && savedVolume >= 0 && savedVolume <= 1) {
      return setPlaybackVolume(savedVolume);
    }
  }

  return {
    subscribe,
    set: setState,

    play(path: string) {
      return update(() => playSong(path));
    },

    pause() {
      return update(pausePlayback);
    },

    resume() {
      return update(resumePlayback);
    },

    seek(positionMs: number) {
      return update(() => seekPlayback(positionMs));
    },

    setVolume: setPlaybackVolume,
    toggleMute,
    adjustVolume,
    restoreVolume,

    startPolling(onSnapshot: (snapshot: PlaybackState) => void | Promise<void>, intervalMs = 500) {
      if (pollingTimer !== undefined) {
        return;
      }

      pollingTimer = window.setInterval(async () => {
        if (isPolling) {
          return;
        }

        isPolling = true;
        try {
          await onSnapshot(await playbackSnapshot());
        } finally {
          isPolling = false;
        }
      }, intervalMs);
    },

    stopPolling() {
      if (pollingTimer === undefined) {
        return;
      }

      window.clearInterval(pollingTimer);
      pollingTimer = undefined;
      isPolling = false;
    }
  };
}
