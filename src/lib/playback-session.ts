import type { QueueSessionState } from './stores/queue';

export type PlaybackSession = {
  currentPath: string;
  positionMs: number;
  queue: QueueSessionState;
};

const SESSION_STORAGE_KEY = 'orca.playbackSession';

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];
}

export function loadPlaybackSession(): PlaybackSession | null {
  try {
    const rawSession = window.localStorage.getItem(SESSION_STORAGE_KEY);
    if (!rawSession) {
      return null;
    }

    const parsed: unknown = JSON.parse(rawSession);
    if (!parsed || typeof parsed !== 'object') {
      return null;
    }

    const session = parsed as Record<string, unknown>;
    const queue = session.queue as Record<string, unknown> | null;
    if (typeof session.currentPath !== 'string' || !session.currentPath || !queue) {
      return null;
    }

    return {
      currentPath: session.currentPath,
      positionMs: typeof session.positionMs === 'number' && Number.isFinite(session.positionMs)
        ? Math.max(0, Math.floor(session.positionMs))
        : 0,
      queue: {
        orderPaths: stringArray(queue.orderPaths),
        removedPaths: stringArray(queue.removedPaths),
        manualPaths: stringArray(queue.manualPaths),
        shufflePlayedPaths: stringArray(queue.shufflePlayedPaths)
      }
    };
  } catch {
    return null;
  }
}

export function savePlaybackSession(session: PlaybackSession) {
  window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
}
