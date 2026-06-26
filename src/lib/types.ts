export type LocalSong = {
  id: number | null;
  path: string;
  title: string;
  artist: string;
  album_artist: string;
  album: string;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
  genre: string | null;
  duration: number;
  artwork: string | null;
  artwork_thumb: string | null;
  artwork_preview: string | null;
  lyrics: string | null;
  sample_rate: number | null;
  bitrate: number | null;
  bit_depth: number | null;
  format: string | null;
  modified_at?: number | null;
  file_size?: number | null;
};

export type PlaybackState = {
  current_path: string | null;
  position_ms: number;
  duration_ms: number;
  is_playing: boolean;
  volume: number;
};

export type Playlist = {
  id: number;
  name: string;
  cover_path: string | null;
  song_count: number;
};

export type LibrarySnapshot = {
  songs: LocalSong[];
  playlists: Playlist[];
  playback: PlaybackState;
  folder_count?: number;
};

export type SongMetadataUpdate = {
  path: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
  genre: string | null;
  lyrics: string | null;
};
