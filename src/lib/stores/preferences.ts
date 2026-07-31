import { writable } from 'svelte/store';

export type PlayerPlacement = 'right' | 'bottom';
export type SidebarMode = 'expanded' | 'collapsed';
export type SeekbarStyle = 'standard' | 'waveform';
export type RepeatMode = 'off' | 'all' | 'one';

type Preferences = {
  playerPlacement: PlayerPlacement;
  sidebarMode: SidebarMode;
  seekbarStyle: SeekbarStyle;
  dynamicCoverAccent: boolean;
  blurredNowPlayingBackground: boolean;
  fontFamily: string;
  fontSizePercent: number;
  showQualityInfo: boolean;
  gaplessPlayback: boolean;
  autoFetchArtwork: boolean;
  shuffleEnabled: boolean;
  repeatMode: RepeatMode;
  fullPlayerLyricsOpen: boolean;
};

const defaults: Preferences = {
  playerPlacement: 'bottom',
  sidebarMode: 'expanded',
  seekbarStyle: 'waveform',
  dynamicCoverAccent: true,
  blurredNowPlayingBackground: true,
  fontFamily: 'Plus Jakarta Sans',
  fontSizePercent: 100,
  showQualityInfo: true,
  gaplessPlayback: true,
  autoFetchArtwork: false,
  shuffleEnabled: false,
  repeatMode: 'off',
  fullPlayerLyricsOpen: false
};

function readChoice<T extends string>(key: string, fallback: T, choices: T[]): T {
  const value = window.localStorage.getItem(key);
  return choices.includes(value as T) ? (value as T) : fallback;
}

function readBoolean(key: string, fallback: boolean) {
  const value = window.localStorage.getItem(key);
  return value === null ? fallback : value === 'true';
}

function readNumber(key: string, fallback: number, min: number, max: number) {
  const value = Number(window.localStorage.getItem(key));
  return Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

export function createPreferencesStore() {
  const { subscribe, set } = writable(defaults);
  let preferences = defaults;

  function update(next: Partial<Preferences>, storageKey?: string) {
    preferences = { ...preferences, ...next };
    set(preferences);
    if (storageKey) {
      window.localStorage.setItem(storageKey, String(Object.values(next)[0]));
    }
  }

  return {
    subscribe,

    load() {
      preferences = {
        playerPlacement: readChoice('orca.playerPlacement', defaults.playerPlacement, ['right', 'bottom']),
        sidebarMode: readChoice('orca.sidebarMode', defaults.sidebarMode, ['expanded', 'collapsed']),
        seekbarStyle: readChoice('orca.seekbarStyle', defaults.seekbarStyle, ['standard', 'waveform']),
        dynamicCoverAccent: readBoolean('orca.dynamicCoverAccent', defaults.dynamicCoverAccent),
        blurredNowPlayingBackground: readBoolean('orca.blurredNowPlayingBackground', defaults.blurredNowPlayingBackground),
        fontFamily: window.localStorage.getItem('orca.fontFamily') || defaults.fontFamily,
        fontSizePercent: readNumber('orca.fontSizePercent', defaults.fontSizePercent, 80, 120),
        showQualityInfo: readBoolean('orca.showQualityInfo', defaults.showQualityInfo),
        gaplessPlayback: readBoolean('orca.gaplessPlayback', defaults.gaplessPlayback),
        autoFetchArtwork: readBoolean('orca.autoFetchArtwork', defaults.autoFetchArtwork),
        shuffleEnabled: readBoolean('orca.shuffleEnabled', defaults.shuffleEnabled),
        repeatMode: readChoice('orca.repeatMode', defaults.repeatMode, ['off', 'all', 'one']),
        fullPlayerLyricsOpen: readBoolean('orca.fullPlayerLyricsOpen', defaults.fullPlayerLyricsOpen)
      };
      set(preferences);
    },

    setPlayerPlacement: (value: PlayerPlacement) => update({ playerPlacement: value }, 'orca.playerPlacement'),
    setSidebarMode: (value: SidebarMode) => update({ sidebarMode: value }, 'orca.sidebarMode'),
    setSeekbarStyle: (value: SeekbarStyle) => update({ seekbarStyle: value }, 'orca.seekbarStyle'),
    setDynamicCoverAccent: (value: boolean) => update({ dynamicCoverAccent: value }, 'orca.dynamicCoverAccent'),
    setBlurredNowPlayingBackground: (value: boolean) => update({ blurredNowPlayingBackground: value }, 'orca.blurredNowPlayingBackground'),
    setFontFamily: (value: string) => update({ fontFamily: value }, 'orca.fontFamily'),
    setFontSizePercent: (value: number) => update({ fontSizePercent: Math.min(120, Math.max(80, Math.round(value))) }, 'orca.fontSizePercent'),
    setShowQualityInfo: (value: boolean) => update({ showQualityInfo: value }, 'orca.showQualityInfo'),
    setGaplessPlayback: (value: boolean) => update({ gaplessPlayback: value }, 'orca.gaplessPlayback'),
    setAutoFetchArtwork: (value: boolean) => update({ autoFetchArtwork: value }, 'orca.autoFetchArtwork'),
    toggleShuffle: () => update({ shuffleEnabled: !preferences.shuffleEnabled }, 'orca.shuffleEnabled'),
    cycleRepeat: () => update({ repeatMode: preferences.repeatMode === 'off' ? 'all' : preferences.repeatMode === 'all' ? 'one' : 'off' }, 'orca.repeatMode'),
    setFullPlayerLyricsOpen: (value: boolean) => update({ fullPlayerLyricsOpen: value }, 'orca.fullPlayerLyricsOpen')
  };
}
