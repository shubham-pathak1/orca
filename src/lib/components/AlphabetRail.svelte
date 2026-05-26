<script lang="ts">
  export let onJump: (letter: string) => void = () => {};

  const letters = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('')];
</script>

<div class="flex w-6 shrink-0 flex-col items-center justify-center gap-0.5 border-l border-white/8 bg-black/20 py-2">
  {#each letters as letter}
    <button
      class="grid h-[3.45%] min-h-3 w-full place-items-center text-[9px] font-bold leading-none text-white/34 transition hover:text-white focus:text-[var(--accent)]"
      title={`Jump to ${letter}`}
      on:click={() => onJump(letter)}
    >
      {letter}
    </button>
  {/each}
</div>
