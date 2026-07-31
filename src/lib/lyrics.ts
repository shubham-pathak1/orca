export type LyricLine = {
  index: number;
  timeMs: number | null;
  text: string;
};

export function parseLyrics(rawLyrics: string): LyricLine[] {
  const rawLines = rawLyrics.split(/\r?\n/);
  const syncedLines: LyricLine[] = [];
  const plainLines: LyricLine[] = [];

  for (const rawLine of rawLines) {
    const timestamps = Array.from(rawLine.matchAll(/\[(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?\]/g));
    const text = rawLine.replace(/\[[^\]]+\]/g, '').trim();
    if (!text) {
      continue;
    }

    if (timestamps.length > 0) {
      for (const timestamp of timestamps) {
        const minutes = Number(timestamp[1]);
        const seconds = Number(timestamp[2]);
        const fraction = timestamp[3] ?? '0';
        const millis = Number(fraction.padEnd(3, '0').slice(0, 3));
        syncedLines.push({
          index: syncedLines.length,
          timeMs: minutes * 60_000 + seconds * 1_000 + millis,
          text
        });
      }
    } else {
      plainLines.push({
        index: plainLines.length,
        timeMs: null,
        text
      });
    }
  }

  const lines = syncedLines.length > 0 ? syncedLines.sort((a, b) => (a.timeMs ?? 0) - (b.timeMs ?? 0)) : plainLines;
  return lines.map((line, index) => ({ ...line, index }));
}

export function findActiveLyricIndex(lines: LyricLine[], positionMs: number): number {
  let activeIndex = 0;
  for (const line of lines) {
    if (line.timeMs !== null && line.timeMs <= positionMs) {
      activeIndex = line.index;
    }
  }
  return activeIndex;
}

export function estimateActiveLyricIndex(lines: LyricLine[], positionMs: number, durationMs: number): number {
  if (!durationMs || durationMs <= 0) {
    return 0;
  }

  const progress = Math.min(Math.max(positionMs / durationMs, 0), 0.999);
  return Math.min(lines.length - 1, Math.floor(progress * lines.length));
}

export function lyricSeekPosition(line: LyricLine, lineCount: number, durationMs: number): number | null {
  if (line.timeMs !== null) {
    return line.timeMs;
  }

  if (!durationMs || lineCount === 0) {
    return null;
  }

  const progress = lineCount === 1 ? 0 : line.index / Math.max(1, lineCount - 1);
  return Math.round(progress * durationMs);
}
