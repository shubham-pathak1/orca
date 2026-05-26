<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { artworkUrl } from '../tauri';

  export let path: string | null = null;
  export let alt = '';
  export let rootClass = '';
  export let imageClass = 'h-full w-full object-cover';

  let root: HTMLSpanElement;
  let isVisible = false;
  let observer: IntersectionObserver | null = null;

  $: src = isVisible ? artworkUrl(path) : null;

  onMount(() => {
    observer = new IntersectionObserver(
      ([entry]) => {
        isVisible = entry.isIntersecting;
      },
      { rootMargin: '160px' }
    );

    observer.observe(root);
  });

  onDestroy(() => {
    observer?.disconnect();
    observer = null;
    isVisible = false;
  });
</script>

<span bind:this={root} class={rootClass}>
  {#if src}
    <img class={imageClass} {src} {alt} loading="lazy" decoding="async" />
  {/if}
</span>
