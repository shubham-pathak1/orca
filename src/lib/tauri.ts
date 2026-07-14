import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import type { LibrarySnapshot, LocalSong, PlaybackState, Playlist, SongMetadataUpdate, ArtistEntry, AlbumEntry } from './types';

const fallbackPlayback: PlaybackState = {
  current_path: null,
  position_ms: 0,
  duration_ms: 0,
  is_playing: false,
  volume: 1
};

const fileSrcCache = new Map<string, string>();
const FILE_SRC_CACHE_MAX = 2048;

function cachedFileSrc(path: string | null): string | null {
  if (!path) {
    return null;
  }

  const cached = fileSrcCache.get(path);
  if (cached) {
    fileSrcCache.delete(path);
    fileSrcCache.set(path, cached);
    return cached;
  }

  try {
    const url = convertFileSrc(path);
    fileSrcCache.set(path, url);
    if (fileSrcCache.size > FILE_SRC_CACHE_MAX) {
      const oldest = fileSrcCache.keys().next().value;
      if (oldest) {
        fileSrcCache.delete(oldest);
      }
    }
    return url;
  } catch {
    return null;
  }
}

export async function getLibrarySnapshot(): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('library_snapshot').catch(() => ({
    songs: [],
    playlists: [],
    artists: [],
    albums: [],
    playback: fallbackPlayback,
    folder_count: 0
  }));
}

export async function libraryFolderCount(): Promise<number> {
  return invoke<number>('library_folder_count').catch(() => 0);
}

export async function libraryScanRoots(): Promise<string[]> {
  return invoke<string[]>('library_scan_roots').catch(() => []);
}

export async function removeLibraryScanRoot(root: string): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('remove_library_scan_root', { root });
}

export async function pickAndScanFolder(): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('pick_and_scan_folder');
}

export async function rescanLibrary(): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('rescan_library');
}

export async function createPlaylist(name: string): Promise<Playlist[]> {
  return invoke<Playlist[]>('create_playlist', { name });
}

export async function renamePlaylist(playlistId: number, name: string): Promise<Playlist[]> {
  return invoke<Playlist[]>('rename_playlist', { playlistId, name });
}

export async function deletePlaylist(playlistId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('delete_playlist', { playlistId });
}

export async function addSongToPlaylist(playlistId: number, songId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('add_song_to_playlist', { playlistId, songId });
}

export async function removeSongFromPlaylist(playlistId: number, songId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('remove_song_from_playlist', { playlistId, songId });
}

export async function playlistSongIds(playlistId: number): Promise<number[]> {
  return invoke<number[]>('playlist_song_ids', { playlistId });
}

export async function choosePlaylistCover(playlistId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('choose_playlist_cover', { playlistId });
}

export async function removePlaylistCover(playlistId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('remove_playlist_cover', { playlistId });
}

export async function chooseArtistCover(artistName: string): Promise<ArtistEntry[]> {
  return invoke<ArtistEntry[]>('choose_artist_cover', { artistName });
}

export async function removeArtistCover(artistName: string): Promise<ArtistEntry[]> {
  return invoke<ArtistEntry[]>('remove_artist_cover', { artistName });
}

export async function chooseAlbumCover(albumKey: string): Promise<AlbumEntry[]> {
  return invoke<AlbumEntry[]>('choose_album_cover', { albumKey });
}

export async function removeAlbumCover(albumKey: string): Promise<AlbumEntry[]> {
  return invoke<AlbumEntry[]>('remove_album_cover', { albumKey });
}

export async function updateSongMetadata(update: SongMetadataUpdate): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('update_song_metadata', { update });
}

export async function chooseSongCover(path: string): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('choose_song_cover', { path });
}

export async function removeSongCover(path: string): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>('remove_song_cover', { path });
}

export async function playSong(path: string): Promise<PlaybackState> {
  return invoke<PlaybackState>('play_song', { path });
}

export async function queueNextPlayback(path: string): Promise<PlaybackState> {
  return invoke<PlaybackState>('queue_next_playback', { path });
}

export async function pausePlayback(): Promise<PlaybackState> {
  return invoke<PlaybackState>('pause_playback');
}

export async function resumePlayback(): Promise<PlaybackState> {
  return invoke<PlaybackState>('resume_playback');
}

export async function seekPlayback(positionMs: number): Promise<PlaybackState> {
  return invoke<PlaybackState>('seek_playback', { positionMs });
}

export async function setVolume(volume: number): Promise<PlaybackState> {
  return invoke<PlaybackState>('set_volume', { volume });
}

export async function waveformPeaks(path: string, buckets = 96): Promise<number[]> {
  return invoke<number[]>('waveform_peaks', { path, buckets });
}

export async function playbackSnapshot(): Promise<PlaybackState> {
  return invoke<PlaybackState>('playback_snapshot').catch(() => fallbackPlayback);
}

export function artworkUrl(path: string | null): string | null {
  return cachedFileSrc(path);
}

export function audioUrl(path: string | null): string | null {
  return cachedFileSrc(path);
}
