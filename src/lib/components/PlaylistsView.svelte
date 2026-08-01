<script lang="ts">
  import { tick } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { formatDuration } from '../format';
  import type { LocalSong, Playlist } from '../types';
  import LazyArtwork from './LazyArtwork.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  export let playlists: Playlist[] = [];
  export let songs: LocalSong[] = [];
  export let currentPath: string | null = null;
  export let onChooseSong: (song: LocalSong, contextSongs?: LocalSong[]) => void = () => {};
  export let onCreatePlaylist: (name: string) => Promise<void> | void = () => {};
  export let onLoadPlaylistSongIds: (playlistId: number) => Promise<number[]> = async () => [];
  export let onRenamePlaylist: (playlistId: number, name: string) => Promise<void> | void = () => {};
  export let onDeletePlaylist: (playlistId: number) => Promise<void> | void = () => {};
  export let onChoosePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onRemovePlaylistCover: (playlistId: number) => Promise<void> | void = () => {};
  export let onOpenSongMenu: (event: MouseEvent, song: LocalSong) => void = () => {};

  // Exported so LibraryView's shared song context menu knows we're inside a playlist
  export let selectedPlaylist: Playlist | null = null;
  export let selectedPlaylistSongIds: number[] = [];

  // Exported so LibraryView can toggle header / height
  export let isInDetail = false;

  let playlistQuery = '';
  let detailQuery = '';
  let newPlaylistName = '';
  let isCreatingPlaylist = false;
  let editingPlaylistName = '';
  let playlistNameInput: HTMLInputElement;
  let isEditingPlaylistName = false;
  let isLoadingPlaylist = false;
  let isRenamingPlaylist = false;
  let showDeletePlaylistConfirm = false;
  let playlistToDelete: Playlist | null = null;
  let playlistContextMenu: { x: number; y: number; playlist: Playlist } | null = null;

  $: isInDetail = Boolean(selectedPlaylist);

  $: filteredPlaylists = playlists.filter((p) =>
    p.name.toLowerCase().includes(playlistQuery.trim().toLowerCase())
  );
  $: selectedPlaylistSongs = selectedPlaylistSongIds
    .map((id) => songs.find((s) => s.id === id))
    .filter((s): s is LocalSong => Boolean(s));
  $: selectedPlaylistVisibleSongs = filterDetailSongs(selectedPlaylistSongs, detailQuery);
  $: selectedPlaylistArtwork = selectedPlaylist?.cover_path
    ?? selectedPlaylistSongs.find((s) => s.artwork_preview ?? s.artwork_thumb)?.artwork_preview
    ?? null;

  function filterDetailSongs(sourceSongs: LocalSong[], searchQuery: string) {
    const needle = searchQuery.trim().toLowerCase();
    if (!needle) return sourceSongs;
    return sourceSongs.filter((song) =>
      [song.title, song.artist, song.album].some((v) => v.toLowerCase().includes(needle))
    );
  }

  function rowArtwork(song: LocalSong): string | null {
    return song.artwork_thumb ?? song.artwork_preview ?? null;
  }

  async function createPlaylistFromInput() {
    const name = newPlaylistName.trim();
    if (!name || isCreatingPlaylist) return;
    isCreatingPlaylist = true;
    try {
      await onCreatePlaylist(name);
      newPlaylistName = '';
    } finally {
      isCreatingPlaylist = false;
    }
  }

  export async function openPlaylist(playlist: Playlist) {
    selectedPlaylist = playlist;
    editingPlaylistName = playlist.name;
    detailQuery = '';
    isLoadingPlaylist = true;
    try {
      selectedPlaylistSongIds = await onLoadPlaylistSongIds(playlist.id);
    } finally {
      isLoadingPlaylist = false;
    }
  }

  export function closePlaylist() {
    selectedPlaylist = null;
    selectedPlaylistSongIds = [];
    editingPlaylistName = '';
    detailQuery = '';
  }

  async function savePlaylistName() {
    if (!selectedPlaylist || isRenamingPlaylist) return;
    const name = editingPlaylistName.trim();
    if (!name || name === selectedPlaylist.name) {
      editingPlaylistName = selectedPlaylist.name;
      isEditingPlaylistName = false;
      return;
    }
    isRenamingPlaylist = true;
    try {
      await onRenamePlaylist(selectedPlaylist.id, name);
    } finally {
      isRenamingPlaylist = false;
      isEditingPlaylistName = false;
    }
  }

  async function editPlaylistName() {
    isEditingPlaylistName = true;
    await tick();
    playlistNameInput?.focus();
    playlistNameInput?.select();
  }

  function handlePlaylistNameKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') { event.preventDefault(); playlistNameInput?.blur(); }
    if (event.key === 'Escape') { editingPlaylistName = selectedPlaylist?.name ?? ''; isEditingPlaylistName = false; }
  }

  async function chooseSelectedPlaylistCover() {
    if (selectedPlaylist) await onChoosePlaylistCover(selectedPlaylist.id);
  }

  async function removeSelectedPlaylistCover() {
    if (selectedPlaylist) await onRemovePlaylistCover(selectedPlaylist.id);
  }

  function playFirstSong(sourceSongs: LocalSong[]) {
    const first = sourceSongs[0];
    if (first) onChooseSong(first, sourceSongs);
  }

  function openPlaylistMenu(event: MouseEvent, playlist: Playlist) {
    event.preventDefault();
    event.stopPropagation();
    playlistContextMenu = {
      x: Math.min(event.clientX, window.innerWidth - 200),
      y: Math.min(event.clientY, window.innerHeight - 130),
      playlist
    };
  }

  function closePlaylistContextMenu() { playlistContextMenu = null; }

  async function renameContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    closePlaylistContextMenu();
    if (playlist) {
      const newName = prompt('Enter new playlist name:', playlist.name);
      if (newName && newName.trim() && newName !== playlist.name) {
        await onRenamePlaylist(playlist.id, newName.trim());
      }
    }
  }

  async function deleteContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    closePlaylistContextMenu();
    if (playlist && confirm(`Are you sure you want to delete "${playlist.name}"?`)) {
      await onDeletePlaylist(playlist.id);
    }
  }

  async function addCoverContextPlaylist() {
    const playlist = playlistContextMenu?.playlist;
    closePlaylistContextMenu();
    if (playlist) await onChoosePlaylistCover(playlist.id);
  }

  async function confirmDeletePlaylist() {
    const playlist = playlistToDelete;
    showDeletePlaylistConfirm = false;
    playlistToDelete = null;
    if (!playlist) return;
    if (selectedPlaylist?.id === playlist.id) closePlaylist();
    await onDeletePlaylist(playlist.id);
  }

  function cancelDeletePlaylist() {
    showDeletePlaylistConfirm = false;
    playlistToDelete = null;
  }
</script>

<svelte:window on:click={closePlaylistContextMenu} />

<div class="scrollbar-none h-full overflow-auto pr-2">
  {#if selectedPlaylist}
    <!-- Playlist detail -->
    <div class="relative mb-7 overflow-hidden rounded-md px-5 pb-6 pt-5">
      <div class="pointer-events-none absolute inset-0 transform-gpu bg-cover bg-center opacity-20 blur-3xl"
        style={`background-image: ${artworkUrl(selectedPlaylistArtwork) ? `url("${artworkUrl(selectedPlaylistArtwork)}")` : 'none'}`}></div>
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/[0.06] via-transparent to-black/30"></div>
      <div class="relative mb-5 flex items-center justify-between gap-4">
        <button class="grid h-10 w-10 shrink-0 place-items-center rounded-full border border-white/12 bg-black/24 text-white/70 transition hover:border-white/24 hover:bg-white/[0.08] hover:text-white"
          type="button" title="Back" aria-label="Back" on:click={closePlaylist}>
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
        <label class="w-full max-w-xl">
          <span class="sr-only">Search songs in playlist</span>
          <input class="h-10 w-full rounded-md border border-white/10 bg-white/[0.04] px-3 text-sm text-white caret-white outline-none transition placeholder:text-white focus:border-[color:var(--accent-mid)]"
            bind:value={detailQuery} placeholder="Search songs in this playlist..." />
        </label>
      </div>
      <div class="relative grid grid-cols-[148px_minmax(0,1fr)] items-end gap-5 max-md:grid-cols-1">
        <div class="group relative grid aspect-square w-[148px] shrink-0 place-items-center overflow-hidden rounded-md bg-white/[0.07] text-5xl font-black text-white/30 shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
          {#if artworkUrl(selectedPlaylistArtwork)}
            <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={selectedPlaylistArtwork} alt="" />
          {:else}
            {selectedPlaylist.name.charAt(0).toUpperCase()}
          {/if}
          <div class="absolute inset-x-2 bottom-2 flex justify-end gap-2 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100">
            <button class="grid h-8 w-8 place-items-center rounded-full bg-white text-black shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md"
              type="button" title="Change cover" aria-label="Change cover" on:click={chooseSelectedPlaylistCover}>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
              </svg>
            </button>
            <button class="grid h-8 w-8 place-items-center rounded-full bg-black text-white shadow-[0_10px_28px_rgba(0,0,0,0.36)] backdrop-blur-md"
              type="button" title="Remove cover" aria-label="Remove cover" on:click={removeSelectedPlaylistCover}>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" /><path d="M10 11v5M14 11v5" />
              </svg>
            </button>
          </div>
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex max-w-3xl items-center gap-2">
            {#if isEditingPlaylistName}
              <input class="h-14 min-w-0 flex-1 rounded-md border border-white/10 bg-white/[0.025] px-3 text-5xl font-black text-white outline-none focus:border-[color:var(--accent-mid)] max-xl:text-4xl"
                bind:this={playlistNameInput} bind:value={editingPlaylistName}
                on:blur={savePlaylistName} on:keydown={handlePlaylistNameKeydown} />
            {:else}
              <button class="min-w-0 truncate text-left text-6xl font-black leading-tight text-white outline-none transition hover:text-white/80 focus-visible:ring-2 focus-visible:ring-white/24 max-xl:text-5xl"
                type="button" title="Edit playlist name" on:click={editPlaylistName}>
                {selectedPlaylist.name}
              </button>
            {/if}
          </div>
          <p class="mt-2 text-sm text-white/58">{selectedPlaylist.song_count} {selectedPlaylist.song_count === 1 ? 'song' : 'songs'}</p>
          <div class="mt-5 flex flex-wrap items-center gap-2">
            <button class="grid h-11 w-11 place-items-center rounded-full bg-[var(--accent)] text-black transition hover:scale-105"
              title="Play playlist" on:click={() => playFirstSong(selectedPlaylistVisibleSongs)}>
              <svg class="ml-0.5 h-5 w-5" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    {#if isLoadingPlaylist}
      <p class="text-sm text-white/42">Loading playlist...</p>
    {:else if selectedPlaylistVisibleSongs.length}
      <div class="grid h-8 grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center border-b border-white/8 px-2 text-[11px] font-bold uppercase text-white/36 max-lg:grid-cols-[40px_minmax(180px,1fr)_72px]">
        <span>#</span><span>Title</span><span class="max-lg:hidden">Artist</span><span class="text-right">Time</span>
      </div>
      {#each selectedPlaylistVisibleSongs as song, index}
        <div class={`grid min-h-11 w-full grid-cols-[48px_minmax(220px,1fr)_minmax(140px,0.6fr)_72px] items-center gap-3 border-b border-white/[0.035] px-2 text-left transition max-lg:grid-cols-[40px_minmax(180px,1fr)_72px] ${song.path === currentPath ? 'bg-[var(--accent-soft)]' : 'hover:bg-white/[0.035]'}`}
          on:contextmenu={(e) => onOpenSongMenu(e, song)} role="presentation">
          <button class="text-left text-sm text-white/36" on:click={() => onChooseSong(song, selectedPlaylistVisibleSongs)}>{index + 1}</button>
          <button class="flex min-w-0 items-center gap-2 text-left" on:click={() => onChooseSong(song, selectedPlaylistVisibleSongs)}>
            {#if artworkUrl(song.artwork)}
              <LazyArtwork rootClass="h-8 w-8 shrink-0 rounded-sm overflow-hidden" imageClass="h-full w-full object-cover" path={rowArtwork(song)} alt="" />
            {:else}
              <img src="/cover.png" class="h-8 w-8 shrink-0 rounded-sm object-cover" alt="" />
            {/if}
            <span class="min-w-0">
              <span class="block truncate text-sm font-semibold text-white">{song.title}</span>
              <span class="block truncate text-xs text-white/36">{song.album}</span>
            </span>
          </button>
          <span class="truncate text-xs text-white/42 max-lg:hidden">{song.artist}</span>
          <span class="text-right text-xs text-white/48">{formatDuration(song.duration)}</span>
        </div>
      {/each}
    {:else}
      <div class="mx-auto flex min-h-[260px] max-w-xl flex-col items-center justify-center text-center">
        <p class="text-sm font-bold uppercase text-white/34">{detailQuery.trim() ? 'No songs found' : 'Empty playlist'}</p>
        <h2 class="mt-3 text-3xl font-black tracking-normal">{detailQuery.trim() ? 'Oops, no songs in this playlist match :(' : 'Add songs from Library.'}</h2>
        <p class="mt-3 text-sm leading-6 text-white/48">{detailQuery.trim() ? 'Try a different search inside this playlist.' : 'Right-click any song in Library, then choose this playlist.'}</p>
      </div>
    {/if}

  {:else}
    <!-- Playlist grid -->
    <form class="mb-5 flex flex-wrap items-center gap-3" on:submit|preventDefault={createPlaylistFromInput}>
      <input class="h-10 w-full max-w-[360px] rounded-md border border-white/10 bg-white/[0.045] px-3 text-sm text-white outline-none placeholder:text-white focus:border-[color:var(--accent-mid)]"
        bind:value={newPlaylistName} placeholder="New playlist name" />
      <button class="h-10 rounded-md border border-white/14 px-4 text-sm font-bold text-white transition hover:bg-white/[0.08] disabled:opacity-40"
        disabled={!newPlaylistName.trim() || isCreatingPlaylist}>
        Create Playlist
      </button>
    </form>

    {#if filteredPlaylists.length}
      <div class="scrollbar-none grid max-h-[calc(100%-60px)] grid-cols-5 gap-x-6 overflow-auto pr-2 max-2xl:grid-cols-4 max-lg:grid-cols-3 max-md:grid-cols-2">
        {#each filteredPlaylists as playlist}
          <button class="flex min-w-0 items-center gap-3 border-b border-white/[0.04] px-2 py-4 text-left transition hover:bg-white/[0.035]"
            on:click={() => openPlaylist(playlist)} on:contextmenu={(e) => openPlaylistMenu(e, playlist)}>
            <span class="grid h-11 w-11 shrink-0 place-items-center overflow-hidden rounded-sm bg-white/[0.07] text-xs font-black text-white/40">
              {#if artworkUrl(playlist.cover_path)}
                <LazyArtwork rootClass="h-full w-full" imageClass="h-full w-full object-cover" path={playlist.cover_path} alt="" />
              {:else}
                {playlist.name.charAt(0).toUpperCase()}
              {/if}
            </span>
            <span class="min-w-0">
              <span class="block truncate text-sm font-bold text-white">{playlist.name}</span>
              <span class="mt-1 block text-xs text-white/52">{playlist.song_count} {playlist.song_count === 1 ? 'song' : 'songs'}</span>
            </span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="mx-auto flex h-[calc(100%-60px)] max-w-xl flex-col items-center justify-center text-center">
        <p class="text-sm font-bold uppercase text-white/34">{playlistQuery.trim() ? 'No playlists found' : 'No playlists yet'}</p>
        <h2 class="mt-3 text-4xl font-black tracking-normal">{playlistQuery.trim() ? 'Oops, no such playlist found :(' : 'Build a queue worth keeping.'}</h2>
        <p class="mt-3 text-sm leading-6 text-white/48">{playlistQuery.trim() ? 'Try another playlist name.' : 'Create a playlist, then right-click songs in Library to add them.'}</p>
      </div>
    {/if}
  {/if}
</div>

{#if playlistContextMenu}
  <div role="menu" tabindex="-1"
    class="fixed z-50 w-52 overflow-hidden rounded-md border border-white/10 bg-[#151515] py-1 text-sm shadow-[0_18px_70px_rgba(0,0,0,0.45)]"
    style={`left: ${playlistContextMenu.x}px; top: ${playlistContextMenu.y}px;`}
    on:click|stopPropagation on:keydown|stopPropagation>
    <div class="border-b border-white/[0.06] px-3 py-2">
      <p class="truncate text-xs font-bold text-white">{playlistContextMenu.playlist.name}</p>
      <p class="truncate text-[11px] text-white/42">{playlistContextMenu.playlist.song_count} {playlistContextMenu.playlist.song_count === 1 ? 'song' : 'songs'}</p>
    </div>
    <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white" on:click={renameContextPlaylist}>
      <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
      </svg>
      Rename
    </button>
    <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-white/78 transition hover:bg-white/[0.08] hover:text-white" on:click={addCoverContextPlaylist}>
      <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="8.5" cy="8.5" r="1.5" /><path d="m21 15-5-5L5 21" />
      </svg>
      Add Cover
    </button>
    <button role="menuitem" class="flex h-9 w-full items-center gap-2.5 px-3 text-left text-xs font-semibold text-red-100/72 transition hover:bg-red-500/10 hover:text-red-100" on:click={deleteContextPlaylist}>
      <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" />
      </svg>
      Delete
    </button>
  </div>
{/if}

<ConfirmDialog
  open={showDeletePlaylistConfirm}
  title="Delete playlist"
  message={playlistToDelete ? `Delete playlist "${playlistToDelete.name}"?` : ''}
  confirmLabel="Delete"
  cancelLabel="Cancel"
  onConfirm={confirmDeletePlaylist}
  onCancel={cancelDeletePlaylist}
/>
