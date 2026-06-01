<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { artworkUrl } from '../tauri';

  export let path: string | null = null;
  export let alt = '';
  export let rootClass = '';
  export let imageClass = 'h-full w-full object-cover';

  let root: HTMLSpanElement;
  let isVisible = false;
  let loaded = false;
  let observer: IntersectionObserver | null = null;

  $: src = isVisible ? artworkUrl(path) : null;
  $: if (!src) {
    loaded = false;
  }
  $: if (src) {
    loaded = false;
  }

  onMount(() => {
    observer = new IntersectionObserver(
      ([entry]) => {
        isVisible = entry.isIntersecting;
      },
      { rootMargin: '48px', threshold: 0.01 }
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
      loading="lazy"
      decoding="async"
      on:load={() => (loaded = true)}
    />
  {/if}
</span>
