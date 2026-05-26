<script lang="ts">
  export let isPlaying = false;
  export let compact = false;
  export let large = false;
  export let showModeControls = true;
  export let shuffleEnabled = false;
  export let repeatMode: 'off' | 'all' | 'one' = 'off';
  export let onToggle: () => void = () => {};
  export let onPrevious: () => void = () => {};
  export let onNext: () => void = () => {};
  export let onToggleShuffle: () => void = () => {};
  export let onCycleRepeat: () => void = () => {};

  $: controlSize = large ? 'h-10 w-10' : 'h-9 w-9';
  $: iconSize = large ? 'h-5 w-5' : 'h-[18px] w-[18px]';
  $: playSize = large ? 'h-14 w-14' : compact ? 'h-11 w-11' : 'h-12 w-12';
  $: playIconSize = large ? 'h-6 w-6' : 'h-5 w-5';
  $: modeActiveClass = 'bg-[var(--accent-soft)] text-[var(--accent)]';
  $: modeInactiveClass = 'text-white/62 hover:bg-white/[0.08] hover:text-white';
</script>

<div class={`flex items-center justify-center ${large ? 'gap-6' : compact ? 'gap-3' : 'gap-5'}`}>
  {#if showModeControls}
    <button
      class={`grid ${controlSize} place-items-center rounded-full transition ${shuffleEnabled ? modeActiveClass : modeInactiveClass}`}
      title={shuffleEnabled ? 'Shuffle on' : 'Shuffle off'}
      aria-pressed={shuffleEnabled}
      on:click={onToggleShuffle}
    >
      <svg class={iconSize} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 7h2.5c2.5 0 3.9 1.3 5.4 5s2.9 5 5.4 5H21" />
        <path d="M17 3h4v4" />
        <path d="m17 7 4-4" />
        <path d="M3 17h2.5c1.6 0 2.8-.6 3.8-1.9" />
        <path d="M14.7 8.9c.5-1.1 1.7-1.9 3.1-1.9H21" />
        <path d="M17 21h4v-4" />
        <path d="m17 17 4 4" />
      </svg>
    </button>
  {/if}

  <button class={`grid ${controlSize} place-items-center rounded-full text-white/68 transition hover:bg-white/[0.08] hover:text-white`} title="Previous" on:click={onPrevious}>
    <svg class={iconSize} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
      <path d="M6 5v14" />
      <path d="m18 6-9 6 9 6V6Z" />
    </svg>
  </button>

  <button class={`${playSize} aspect-square shrink-0 rounded-full border border-white/20 bg-white text-black shadow-[0_12px_40px_rgba(0,0,0,0.32)] transition hover:scale-105 grid place-items-center`} title="Play or pause" on:click={onToggle}>
    {#if isPlaying}
      <svg class={playIconSize} viewBox="0 0 24 24" fill="currentColor">
        <path d="M7 5h4v14H7zM13 5h4v14h-4z" />
      </svg>
    {:else}
      <svg class={`ml-0.5 ${playIconSize}`} viewBox="0 0 24 24" fill="currentColor">
        <path d="M8 5v14l11-7z" />
      </svg>
    {/if}
  </button>

  <button class={`grid ${controlSize} place-items-center rounded-full text-white/68 transition hover:bg-white/[0.08] hover:text-white`} title="Next" on:click={onNext}>
    <svg class={iconSize} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
      <path d="M18 5v14" />
      <path d="m6 6 9 6-9 6V6Z" />
    </svg>
  </button>

  {#if showModeControls}
    <button
      class={`relative grid ${controlSize} place-items-center rounded-full transition ${repeatMode !== 'off' ? modeActiveClass : modeInactiveClass}`}
      title={repeatMode === 'one' ? 'Repeat one' : repeatMode === 'all' ? 'Repeat all' : 'Repeat off'}
      aria-pressed={repeatMode !== 'off'}
      on:click={onCycleRepeat}
    >
      <svg class={iconSize} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
        <path d="M17 2.5 21 6l-4 3.5" />
        <path d="M3 11V9a3 3 0 0 1 3-3h15" />
        <path d="M7 21.5 3 18l4-3.5" />
        <path d="M21 13v2a3 3 0 0 1-3 3H3" />
      </svg>
      {#if repeatMode === 'one'}
        <span class="absolute bottom-1 right-1 grid h-3 min-w-3 place-items-center rounded-full bg-current px-0.5 text-[8px] font-black leading-none">
          <span class="text-black">1</span>
        </span>
      {/if}
    </button>
  {/if}
</div>
