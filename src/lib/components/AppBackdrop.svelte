<script lang="ts">
  export let shellStyle = '';
  export let blurredBackground = true;
</script>

<div class="pointer-events-none absolute inset-0 bg-[#090a0c]" style={shellStyle}></div>
{#if blurredBackground}
  <div class="pointer-events-none absolute inset-[-8%] scale-110 bg-cover bg-center opacity-24 blur-3xl [background-image:var(--cover-art)]"></div>
{/if}
<div class="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,#070808_0%,rgba(9,10,12,0.93)_42%,rgba(9,10,12,0.9)_100%)]"></div>
