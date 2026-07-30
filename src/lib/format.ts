export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return '0:00';
  }

  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}

export function formatTotalDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return '0 mins';
  }

  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);

  if (hours > 0) {
    return `${hours} hr${hours > 1 ? 's' : ''} ${minutes} min${minutes !== 1 ? 's' : ''}`;
  }
  return `${minutes} min${minutes !== 1 ? 's' : ''}`;
}

export function formatQuality(format: string | null, sampleRate: number | null, bitrate: number | null): string {
  const parts = [format, sampleRate ? `${Math.round(sampleRate / 1000)} kHz` : null, bitrate ? `${bitrate} kbps` : null];
  return parts.filter(Boolean).join(' | ');
}
