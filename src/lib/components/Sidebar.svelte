<script lang="ts">
  import { iconPath, navItems, type ActiveView } from '../navigation';

  export let activeView: ActiveView = 'songs';
  export let isScanning = false;
  export let folderCount = 0;
  export let onSelect: (view: ActiveView) => void = () => {};
  export let onAddFolder: () => void = () => {};
  export let onRefresh: () => void = () => {};
</script>

<aside class="flex min-h-0 flex-col border-r border-white/8 bg-black/70 px-3 py-4 max-md:hidden">
  <div class="mb-6 flex items-center gap-2 px-1">
    <img class="h-6 w-6 rounded-sm object-cover" src="/orca_logo.png" alt="" />
    <span class="truncate text-base font-bold text-white">Orca</span>
  </div>

  <nav class="flex flex-col gap-1">
    {#each navItems as item}
      <button
        class={`sidebar-item flex h-10 items-center gap-3 rounded-md px-3 text-left text-sm font-semibold transition ${activeView === item.id ? 'sidebar-item-active bg-white/[0.13] text-white shadow-[inset_3px_0_0_var(--accent)]' : 'text-white/62 hover:bg-white/[0.06] hover:text-white'}`}
        on:click={() => onSelect(item.id)}
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d={iconPath(item.icon)} />
        </svg>
        <span>{item.label}</span>
      </button>
    {/each}
  </nav>

  <div class="mt-auto flex flex-col gap-2 border-t border-white/8 pt-4">
    <button class="sidebar-item flex items-center gap-3 rounded-md px-3 py-2 text-left text-xs font-semibold text-white/58 hover:bg-white/[0.06] hover:text-white" disabled={isScanning} on:click={onAddFolder}>
      <span class="relative grid h-4 w-4 shrink-0 place-items-center">
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5z" />
        </svg>
        {#if folderCount > 0}
          <span class="absolute -right-2 -top-2 grid h-4 min-w-4 place-items-center rounded-full bg-white px-1 text-[9px] font-black leading-none text-black">{folderCount}</span>
        {/if}
      </span>
      <span>Folder</span>
    </button>
    <button class="sidebar-item flex items-center gap-3 rounded-md px-3 py-2 text-left text-xs font-semibold text-white/58 hover:bg-white/[0.06] hover:text-white" disabled={isScanning} on:click={onRefresh}>
      <svg class={`h-4 w-4 shrink-0 ${isScanning ? 'animate-spin' : ''}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M20 12a8 8 0 1 1-2.34-5.66" />
        <path d="M20 4v5h-5" />
      </svg>
      <span>{isScanning ? 'Scanning' : 'Refresh'}</span>
    </button>
  </div>
</aside>
