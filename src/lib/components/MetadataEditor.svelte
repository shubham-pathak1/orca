<script lang="ts">
  import { artworkUrl } from '../tauri';
  import type { LocalSong, SongMetadataUpdate } from '../types';

  export let open = false;
  export let song: LocalSong | null = null;
  export let isSaving = false;
  export let onClose: () => void = () => {};
  export let onSave: (update: SongMetadataUpdate) => Promise<void> | void = () => {};
  export let onReplaceCover: (song: LocalSong) => Promise<void> | void = () => {};
  export let onRemoveCover: (song: LocalSong) => Promise<void> | void = () => {};

  let loadedPath: string | null = null;
  let title = '';
  let artist = '';
  let album = '';
  let albumArtist = '';
  let year = '';
  let trackNumber = '';
  let discNumber = '';
  let genre = '';
  let lyrics = '';

  $: if (open && song && song.path !== loadedPath) {
    loadedPath = song.path;
    title = song.title;
    artist = song.artist;
    album = song.album;
    albumArtist = song.album_artist;
    year = song.year?.toString() ?? '';
    trackNumber = song.track_number?.toString() ?? '';
    discNumber = song.disc_number?.toString() ?? '';
    genre = song.genre ?? '';
    lyrics = song.lyrics ?? '';
  }

  $: if (!open) {
    loadedPath = null;
  }

  function parseOptionalNumber(value: string): number | null {
    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }

    const parsed = Number(trimmed);
    return Number.isFinite(parsed) && parsed > 0 ? Math.trunc(parsed) : null;
  }

  async function save() {
    if (!song || isSaving) {
      return;
    }

    await onSave({
      path: song.path,
      title: title.trim(),
      artist: artist.trim(),
      album: album.trim(),
      album_artist: albumArtist.trim(),
      year: parseOptionalNumber(year),
      track_number: parseOptionalNumber(trackNumber),
      disc_number: parseOptionalNumber(discNumber),
      genre: genre.trim() || null,
      lyrics: lyrics.trim() || null
    });
  }
</script>

{#if open && song}
  <div class="fixed inset-0 z-[80] grid place-items-center bg-black/72 px-5 py-6 backdrop-blur-sm" role="presentation" on:click={onClose}>
    <div
      class="grid max-h-[90vh] w-full max-w-5xl grid-cols-[280px_minmax(0,1fr)] overflow-hidden rounded-md border border-white/12 bg-[#101012] shadow-[0_28px_90px_rgba(0,0,0,0.55)] max-lg:grid-cols-1"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Edit song metadata"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <aside class="border-r border-white/8 bg-white/[0.025] p-5 max-lg:border-b max-lg:border-r-0">
        <div class="aspect-square overflow-hidden rounded-md bg-white/[0.06]">
          {#if artworkUrl(song.artwork)}
            <img class="h-full w-full object-cover" src={artworkUrl(song.artwork) ?? ''} alt="" />
          {/if}
        </div>
        <div class="mt-4 min-w-0">
          <p class="truncate text-lg font-black text-white">{song.title}</p>
          <p class="truncate text-sm text-white/50">{song.artist}</p>
        </div>
        <div class="mt-5 grid gap-2">
          <button class="h-10 rounded-md border border-white/12 text-sm font-bold text-white/72 transition hover:bg-white/[0.08] hover:text-white" type="button" on:click={() => onReplaceCover(song)}>
            Replace cover
          </button>
          <button class="h-10 rounded-md border border-white/12 text-sm font-bold text-white/54 transition hover:bg-white/[0.08] hover:text-white" type="button" on:click={() => onRemoveCover(song)}>
            Remove cover
          </button>
        </div>
        <p class="mt-4 break-all text-[11px] leading-5 text-white/32">{song.path}</p>
      </aside>

      <div class="min-h-0 overflow-auto p-5">
        <div class="mb-5 flex items-center justify-between gap-4">
          <div>
            <h2 class="text-2xl font-black">Edit metadata</h2>
            <p class="mt-1 text-xs text-white/42">Writes directly to the audio file.</p>
          </div>
          <button class="grid h-9 w-9 place-items-center rounded-full border border-white/12 text-white/54 transition hover:bg-white/[0.08] hover:text-white" type="button" aria-label="Close editor" on:click={onClose}>
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="grid grid-cols-2 gap-3 max-md:grid-cols-1">
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Title
            <input class="metadata-input" bind:value={title} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Artist
            <input class="metadata-input" bind:value={artist} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Album
            <input class="metadata-input" bind:value={album} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Album artist
            <input class="metadata-input" bind:value={albumArtist} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Year
            <input class="metadata-input" inputmode="numeric" bind:value={year} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Genre
            <input class="metadata-input" bind:value={genre} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Track number
            <input class="metadata-input" inputmode="numeric" bind:value={trackNumber} />
          </label>
          <label class="grid gap-1 text-xs font-bold uppercase text-white/38">
            Disc number
            <input class="metadata-input" inputmode="numeric" bind:value={discNumber} />
          </label>
        </div>

        <label class="mt-5 grid gap-1 text-xs font-bold uppercase text-white/38">
          Lyrics
          <textarea
            class="min-h-56 resize-y rounded-md border border-white/10 bg-white/[0.045] p-3 text-sm normal-case leading-6 text-white outline-none transition placeholder:text-white/28 focus:border-[color:var(--accent-mid)]"
            bind:value={lyrics}
            placeholder="[00:12.00] Timed LRC or plain pasted lyrics"
          ></textarea>
        </label>

        <div class="mt-5 flex justify-end gap-2">
          <button class="h-10 rounded-md border border-white/12 px-4 text-sm font-bold text-white/58 transition hover:bg-white/[0.08] hover:text-white" type="button" on:click={onClose}>
            Cancel
          </button>
          <button
            class="h-10 rounded-md bg-white px-5 text-sm font-black text-black transition hover:bg-[var(--accent)] disabled:opacity-45"
            type="button"
            disabled={isSaving || !title.trim() || !artist.trim() || !album.trim() || !albumArtist.trim()}
            on:click={save}
          >
            {isSaving ? 'Saving...' : 'Save metadata'}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .metadata-input {
    height: 2.5rem;
    border-radius: 0.375rem;
    border: 1px solid rgb(255 255 255 / 0.1);
    background: rgb(255 255 255 / 0.045);
    padding: 0 0.75rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: white;
    outline: none;
    text-transform: none;
    transition:
      border-color 140ms ease,
      background 140ms ease;
  }

  .metadata-input:focus {
    border-color: var(--accent-mid);
    background: rgb(255 255 255 / 0.06);
  }
</style>
