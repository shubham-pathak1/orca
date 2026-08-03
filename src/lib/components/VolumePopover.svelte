<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  export let volume = 1;
  export let onVolume: (event: Event) => void = () => {};
  export let onToggleMute: () => void = () => {};
  export let onAdjustVolume: (amount: number) => void = () => {};

  let open = false;
  let group: HTMLDivElement;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function show() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    open = true;
  }

  function hide() {
    hideTimer = setTimeout(() => {
      open = false;
      hideTimer = undefined;
    }, 800);
  }

  function closeOnOutsideClick(event: MouseEvent) {
    if (group && !group.contains(event.target as Node)) {
      open = false;
      if (hideTimer) clearTimeout(hideTimer);
      hideTimer = undefined;
    }
  }

  function handleWheel(event: WheelEvent) {
    onAdjustVolume(event.deltaY < 0 ? 0.05 : -0.05);
  }

  function toggleMute() {
    onToggleMute();
    show();
  }

  onMount(() => window.addEventListener('click', closeOnOutsideClick));
  onDestroy(() => {
    window.removeEventListener('click', closeOnOutsideClick);
    if (hideTimer) clearTimeout(hideTimer);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute bottom-6 right-6 z-40"
  bind:this={group}
  on:wheel|preventDefault|stopPropagation={handleWheel}
  on:mouseenter={show}
  on:mouseleave={hide}
>
  <div class="group relative">
    <button class="grid h-8 w-8 place-items-center rounded-md text-white/64 transition hover:text-white" type="button" aria-label="Volume" on:click={toggleMute}>
      {#if volume === 0}
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
          <path d="M11 5 6 9H3v6h3l5 4V5Z" />
          <line x1="22" y1="9" x2="16" y2="15" />
          <line x1="16" y1="9" x2="22" y2="15" />
        </svg>
      {:else if volume < 0.5}
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
          <path d="M11 5 6 9H3v6h3l5 4V5Z" />
          <path d="M15.5 8.5a5 5 0 0 1 0 7" />
        </svg>
      {:else}
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
          <path d="M11 5 6 9H3v6h3l5 4V5Z" />
          <path d="M15.5 8.5a5 5 0 0 1 0 7" />
          <path d="M18.4 5.6a9 9 0 0 1 0 12.8" />
        </svg>
      {/if}
    </button>

    {#if open}
      <div class="pointer-events-auto absolute bottom-full right-0 z-20 mb-2 animate-in fade-in-0 zoom-in-95 duration-150">
        <div class="rounded-xl border border-white/8 bg-[#171719]/90 p-3 shadow-[0_8px_30px_rgba(0,0,0,0.45)] backdrop-blur-md">
          <input
            class="h-28 w-3 [direction:rtl] [writing-mode:vertical-lr]"
            style="accent-color: var(--accent)"
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={volume}
            on:input={onVolume}
            aria-label="Volume level"
            on:mouseenter={show}
            on:mouseleave={hide}
          />
        </div>
      </div>
    {/if}
  </div>
</div>
