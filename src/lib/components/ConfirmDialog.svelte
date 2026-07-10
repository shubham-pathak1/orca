<script lang="ts">
  export let open: boolean = false;
  export let title: string = '';
  export let message: string = '';
  export let confirmLabel: string = 'OK';
  export let cancelLabel: string = 'Cancel';
  export let onConfirm: () => void = () => {};
  export let onCancel: () => void = () => {};

  function handleBackgroundClick(event: MouseEvent) {
    if ((event.target as HTMLElement).classList.contains('confirm-overlay')) {
      onCancel();
    }
  }
</script>

{#if open}
  <div class="confirm-overlay fixed inset-0 z-50 flex items-start justify-center p-6" on:click={handleBackgroundClick}>
    <div class="w-full max-w-lg rounded-md bg-[#121212] border border-white/8 shadow-[0_24px_60px_rgba(0,0,0,0.6)]">
      <div class="px-6 py-4">
        {#if title}
          <h3 class="text-lg font-bold">{title}</h3>
        {/if}
        <p class="mt-2 text-sm text-white/64">{message}</p>
      </div>
      <div class="flex justify-end gap-3 border-t border-white/6 px-4 py-3">
        <button class="h-9 rounded-md border border-white/10 px-3 text-sm text-white/64 hover:bg-white/[0.03]" on:click={onCancel}>{cancelLabel}</button>
        <button class="h-9 rounded-md bg-[var(--accent)] px-3 text-sm font-bold text-black" on:click={onConfirm}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-overlay { background: rgba(0,0,0,0.6); }
</style>
