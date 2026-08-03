<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { artworkUrl } from '../tauri';
  import { artworkSuspended } from '../stores/artwork-visibility';

  export let path: string | null = null;
  export let alt = '';
  export let rootClass = '';
  export let imageClass = 'h-full w-full object-cover';

  let root: HTMLSpanElement;
  let isVisible = false;
  let loaded = false;
  let observer: IntersectionObserver | null = null;

  // Only compute the URL when visible — avoids unnecessary convertFileSrc calls
  $: src = isVisible && !$artworkSuspended ? artworkUrl(path) : null;

  // Reset loaded fade-in whenever src changes
  $: if (src) {
    loaded = false;
  }

  onMount(() => {
    observer = new IntersectionObserver(
      ([entry]) => {
        isVisible = entry.isIntersecting;
        // When leaving viewport, drop loaded flag so next appearance fades in cleanly
        if (!entry.isIntersecting) {
          loaded = false;
        }
      },
      { rootMargin: '120px', threshold: 0 }
    );
    observer.observe(root);
  });

  onDestroy(() => {
    observer?.disconnect();
    observer = null;
    isVisible = false;
  });
</script>

<span bind:this={root} class={`${rootClass} relative block`}>
  {#if src}
    <img
      class={`${imageClass} transition-opacity duration-150 ${loaded ? 'opacity-100' : 'opacity-0'}`}
      {src}
      {alt}
      decoding="async"
      on:load={() => (loaded = true)}
    />
  {/if}
</span>
