export type LyricWord = {
  text: string;
  timeMs: number;
  endTimeMs: number;
};

export type LyricLine = {
  index: number;
  timeMs: number | null;
  text: string;
  words?: LyricWord[];
};

function timestampMs(minutes: string, seconds: string, fraction = '0'): number {
  return Number(minutes) * 60_000 + Number(seconds) * 1_000
    + Number(fraction.padEnd(3, '0').slice(0, 3));
}

export function parseLyrics(rawLyrics: string): LyricLine[] {
  const rawLines = rawLyrics.split(/\r?\n/);
  const syncedLines: LyricLine[] = [];
  const plainLines: LyricLine[] = [];
  const boundaries: number[] = [];
  const offsetMs = Number(rawLyrics.match(/\[offset:\s*([+-]?\d+)\]/i)?.[1] ?? 0);

  for (const rawLine of rawLines) {
    const prefix = rawLine.match(/^\s*((?:\[\d{1,2}:\d{2}(?:[.:]\d{1,3})?\])+)/)?.[1] ?? '';
    const timestamps = Array.from(prefix.matchAll(/\[(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?\]/g));
    const content = (prefix ? rawLine.slice(rawLine.indexOf(prefix) + prefix.length) : rawLine).trim();
    if (!prefix && /^\[[a-z]+:.*\]$/i.test(content)) {
      continue;
    }
    const markers = Array.from(content.matchAll(/<(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?>/g));
    const text = content.replace(/<\d{1,2}:\d{2}(?:[.:]\d{1,3})?>/g, '').trim();

    if (timestamps.length > 0) {
      for (const timestamp of timestamps) {
        const timeMs = timestampMs(timestamp[1], timestamp[2], timestamp[3]) + offsetMs;
        boundaries.push(timeMs);
        if (!text) continue;
        const words: LyricWord[] = [];
        if (markers.length) {
          // Repeated line timestamps share word timings relative to the first occurrence.
          const shiftMs = timeMs - timestampMs(timestamps[0][1], timestamps[0][2], timestamps[0][3]);
          let start = 0;
          let wordTimeMs = timeMs;
          for (const marker of markers) {
            const endTimeMs = timestampMs(marker[1], marker[2], marker[3]) + shiftMs;
            const wordText = content.slice(start, marker.index);
            if (wordText) words.push({ text: wordText, timeMs: wordTimeMs, endTimeMs: Math.max(wordTimeMs, endTimeMs) });
            wordTimeMs = endTimeMs;
            start = marker.index! + marker[0].length;
          }
          const tail = content.slice(start);
          if (tail) words.push({ text: tail, timeMs: wordTimeMs, endTimeMs: NaN });
          if (words.length) {
            words[0].text = words[0].text.trimStart();
            words[words.length - 1].text = words[words.length - 1].text.trimEnd();
          }
        }
        syncedLines.push({
          index: syncedLines.length,
          timeMs,
          text,
          ...(words.length ? { words } : {})
        });
      }
    } else if (text) {
      plainLines.push({
        index: plainLines.length,
        timeMs: null,
        text
      });
    }
  }

  boundaries.sort((a, b) => a - b);
  for (const line of syncedLines) {
    const lastWord = line.words?.at(-1);
    if (lastWord && !Number.isFinite(lastWord.endTimeMs)) {
      // A trailing marker provides an exact end; otherwise use the next lyric boundary.
      lastWord.endTimeMs = boundaries.find((time) => time > lastWord.timeMs) ?? lastWord.timeMs + 1_000;
    }
  }
  const lines = syncedLines.length > 0 ? syncedLines.sort((a, b) => (a.timeMs ?? 0) - (b.timeMs ?? 0)) : plainLines;
  return lines.map((line, index) => ({ ...line, index }));
}

export function findActiveLyricIndex(lines: LyricLine[], positionMs: number): number {
  let activeIndex = -1;
  for (const line of lines) {
    if (line.timeMs !== null && line.timeMs <= positionMs) {
      activeIndex = line.index;
    }
  }
  return activeIndex;
}

export function lyricWordProgress(word: LyricWord, positionMs: number): number {
  if (positionMs < word.timeMs) return 0;
  if (word.endTimeMs <= word.timeMs) return 1;
  return Math.min(1, Math.max(0, (positionMs - word.timeMs) / (word.endTimeMs - word.timeMs)));
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
