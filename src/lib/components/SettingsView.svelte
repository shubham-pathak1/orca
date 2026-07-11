<script lang="ts">
  export let playerPlacement: 'right' | 'bottom' = 'right';
  export let onPlayerPlacementChange: (placement: 'right' | 'bottom') => void = () => {};
  export let seekbarStyle: 'standard' | 'waveform' = 'standard';
  export let onSeekbarStyleChange: (style: 'standard' | 'waveform') => void = () => {};
  export let scanRoots: string[] = [];
  export let isScanning = false;
  export let onRemoveScanRoot: (root: string) => Promise<void> | void = () => {};
  export let dynamicCoverAccent = true;
  export let onDynamicCoverAccentChange: (enabled: boolean) => void = () => {};
  export let blurredBackground = true;
  export let onBlurredBackgroundChange: (enabled: boolean) => void = () => {};
  export let fontFamily = 'Plus Jakarta Sans';
  export let onFontFamilyChange: (font: string) => void = () => {};
  export let fontSizePercent = 100;
  export let onFontSizePercentChange: (size: number) => void = () => {};
  export let showQualityInfo = true;
  export let onShowQualityInfoChange: (enabled: boolean) => void = () => {};
  export let gaplessPlayback = true;
  export let onGaplessPlaybackChange: (enabled: boolean) => void = () => {};
  export let theme: 'default' = 'default';
  export let onThemeChange: (theme: 'default') => void = () => {};

  let activeTab = 'Appearance';
  const tabs = ['Appearance', 'Interface', 'Library', 'Audio', 'About'];
  const themes: { id: 'default'; label: string }[] = [{ id: 'default', label: 'Default' }];
  const fontOptions = ['Plus Jakarta Sans', 'System', 'Segoe UI'];
  const playerPlacements: { id: 'right' | 'bottom'; title: string; description: string }[] = [
    { id: 'right', title: 'Right side', description: 'Keep the full controls in the side rail' },
    { id: 'bottom', title: 'Bottom bar', description: 'Use a horizontal player across the bottom' }
  ];
  const seekbarStyles: { id: 'standard' | 'waveform'; title: string; description: string }[] = [
    { id: 'standard', title: 'Classic', description: 'Use the simple progress bar' },
    { id: 'waveform', title: 'Waveform', description: 'Show decoded audio peaks while seeking' }
  ];
  const shortcuts = [
    { keys: ['Space'], action: 'Play or pause' },
    { keys: ['Alt', 'N'], action: 'Next song' },
    { keys: ['Alt', 'P'], action: 'Previous song' },
    { keys: ['L'], action: 'Show or hide lyrics in the full player' },
    { keys: ['F11'], action: 'Toggle full screen' }
  ];

  import { openUrl } from '@tauri-apps/plugin-opener';
  import ConfirmDialog from './ConfirmDialog.svelte';

  const releaseLabel = 'v0.1.2-alpha';

  function updateFontSize(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    onFontSizePercentChange(Number(target.value));
  }

  function folderName(path: string) {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }
  let showRemoveFolderConfirm = false;
  let folderToRemove: string | null = null;

  function removeFolder(root: string) {
    folderToRemove = root;
    showRemoveFolderConfirm = true;
  }

  async function confirmRemoveFolder() {
    const root = folderToRemove;
    showRemoveFolderConfirm = false;
    folderToRemove = null;
    if (!root) return;
    await onRemoveScanRoot(root);
  }

  function cancelRemoveFolder() {
    showRemoveFolderConfirm = false;
    folderToRemove = null;
  }

  function openFeedback() {
    void openUrl('https://github.com/shubham-pathak1/orca/issues');
  }
</script>

<div class="h-full overflow-auto pr-2">
  <div class="mb-7 flex gap-7 border-b border-white/10 text-sm font-semibold text-white/56">
    {#each tabs as tab}
      <button
        class={`pb-3 transition ${activeTab === tab ? 'border-b-2 border-white text-white' : 'hover:text-white'}`}
        on:click={() => (activeTab = tab)}
      >
        {tab}
      </button>
    {/each}
  </div>

  {#if activeTab === 'Appearance'}
    <section class="max-w-[820px]">
      <div class="mb-7">
        <h3 class="text-sm font-bold text-white">Theme</h3>
        <p class="text-sm text-white/48">Choose your preferred color scheme</p>
        <div class="mt-4 grid grid-cols-[repeat(auto-fill,minmax(110px,1fr))] gap-3">
          {#each themes as themeOption}
            <button
              class={`h-12 rounded-md border text-sm font-bold transition ${theme === themeOption.id ? 'border-white bg-white text-black' : 'border-white/12 bg-black/20 text-white hover:border-white/35'}`}
              on:click={() => onThemeChange(themeOption.id)}
            >
              {themeOption.label}
            </button>
          {/each}
        </div>
        <ConfirmDialog
          open={showRemoveFolderConfirm}
          title="Remove folder"
          message={folderToRemove ? `Remove "${folderName(folderToRemove)}" from Orca? Songs from this folder will be removed from the library.` : ''}
          confirmLabel="Remove"
          cancelLabel="Cancel"
          onConfirm={confirmRemoveFolder}
          onCancel={cancelRemoveFolder}
        />
      </div>

      <div class="mb-7 grid grid-cols-[1fr_152px] items-center gap-5 border-t border-white/10 pt-5 max-md:grid-cols-1">
        <div>
          <h3 class="text-sm font-bold text-white">Font</h3>
          <p class="text-sm text-white/48">Choose from presets, local fonts, or upload your own</p>
        </div>
        <select
          class="h-9 rounded-md border border-white/10 bg-[#161616] px-3 text-sm text-white outline-none"
          bind:value={fontFamily}
          on:change={(event) => onFontFamilyChange((event.currentTarget as HTMLSelectElement).value)}
        >
          {#each fontOptions as font}
            <option value={font}>{font}</option>
          {/each}
        </select>
      </div>

      <div class="mb-7 grid grid-cols-[1fr_340px] items-center gap-5 border-t border-white/10 pt-5 max-md:grid-cols-1">
        <div>
          <h3 class="text-sm font-bold text-white">Font Size</h3>
          <p class="text-sm text-white/48">Adjust the base font size</p>
        </div>
        <div class="grid grid-cols-[1fr_70px_54px] items-center gap-3">
          <input class="w-full" style={`accent-color: var(--accent)`} type="range" min="80" max="120" value={fontSizePercent} on:input={updateFontSize} />
          <input
            class="h-9 rounded-md border border-white/10 bg-white/[0.055] px-2 text-sm text-white outline-none"
            type="number"
            min="80"
            max="120"
            value={fontSizePercent}
            on:input={updateFontSize}
          />
          <span class="text-sm text-white/48">%</span>
        </div>
      </div>

      <div class="space-y-5 border-t border-white/10 pt-5">
        <div class="grid grid-cols-[1fr_44px] items-center gap-5">
          <div>
            <h3 class="text-sm font-bold text-white">Dynamic cover accent</h3>
            <p class="text-sm text-white/48">Tint active controls from the current cover art</p>
          </div>
          <button
            class={`relative h-6 w-11 rounded-full border transition ${dynamicCoverAccent ? 'toggle-switch-on' : 'toggle-switch-off'}`}
            title="Dynamic cover accent"
            on:click={() => onDynamicCoverAccentChange(!dynamicCoverAccent)}
          >
            <span class={`toggle-knob absolute top-1 h-4 w-4 rounded-full transition ${dynamicCoverAccent ? 'left-6' : 'left-1'}`}></span>
          </button>
        </div>

        <div class="grid grid-cols-[1fr_44px] items-center gap-5">
          <div>
            <h3 class="text-sm font-bold text-white">Blurred now playing background</h3>
            <p class="text-sm text-white/48">Show cover art ambience behind the app</p>
          </div>
          <button
            class={`relative h-6 w-11 rounded-full border transition ${blurredBackground ? 'toggle-switch-on' : 'toggle-switch-off'}`}
            title="Blurred now playing background"
            on:click={() => onBlurredBackgroundChange(!blurredBackground)}
          >
            <span class={`toggle-knob absolute top-1 h-4 w-4 rounded-full transition ${blurredBackground ? 'left-6' : 'left-1'}`}></span>
          </button>
        </div>
      </div>
    </section>
  {:else if activeTab === 'Interface'}
    <section class="max-w-[820px] space-y-8">
      <div>
        <h3 class="text-sm font-bold text-white">Player placement</h3>
        <p class="text-sm text-white/48">Choose where the compact player lives in the app shell</p>
        <div class="mt-4 grid grid-cols-2 gap-3 max-md:grid-cols-1">
          {#each playerPlacements as placement}
            <button
              class={`min-h-20 rounded-md border px-4 py-3 text-left transition ${playerPlacement === placement.id ? 'border-white bg-white text-black' : 'border-white/12 bg-black/20 text-white hover:border-white/35 hover:bg-white/[0.05]'}`}
              on:click={() => onPlayerPlacementChange(placement.id)}
            >
              <span class="block text-sm font-bold">{placement.title}</span>
              <span class={`mt-1 block text-xs ${playerPlacement === placement.id ? 'text-black/58' : 'text-white/44'}`}>{placement.description}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="border-t border-white/10 pt-6">
        <h3 class="text-sm font-bold text-white">Seekbar style</h3>
        <p class="text-sm text-white/48">Choose between the classic slider and a decoded waveform</p>
        <div class="mt-4 grid grid-cols-2 gap-3 max-md:grid-cols-1">
          {#each seekbarStyles as style}
            <button
              class={`min-h-20 rounded-md border px-4 py-3 text-left transition ${seekbarStyle === style.id ? 'border-white bg-white text-black' : 'border-white/12 bg-black/20 text-white hover:border-white/35 hover:bg-white/[0.05]'}`}
              on:click={() => onSeekbarStyleChange(style.id)}
            >
              <span class="block text-sm font-bold">{style.title}</span>
              <span class={`mt-1 block text-xs ${seekbarStyle === style.id ? 'text-black/58' : 'text-white/44'}`}>{style.description}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="border-t border-white/10 pt-6">
        <h3 class="text-sm font-bold text-white">Keyboard shortcuts</h3>
        <div class="mt-3 overflow-hidden rounded-md border border-white/10 bg-black/18">
          {#each shortcuts as shortcut}
            <div class="grid min-h-11 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border-b border-white/[0.06] px-4 last:border-b-0">
              <span class="text-sm text-white/62">{shortcut.action}</span>
              <span class="flex items-center gap-1.5">
                {#each shortcut.keys as key, index}
                  {#if index > 0}
                    <span class="text-xs text-white/28">+</span>
                  {/if}
                  <kbd class="rounded border border-white/12 bg-white/[0.055] px-2 py-1 text-[11px] font-bold text-white/72">{key}</kbd>
                {/each}
              </span>
            </div>
          {/each}
        </div>
      </div>
    </section>
  {:else if activeTab === 'Library'}
    <section class="max-w-[900px]">
      <div>
        <h3 class="text-sm font-bold text-white">Music folders</h3>
        <p class="text-sm text-white/48">Remove folders Orca should no longer scan</p>
      </div>

      <div class="mt-4 overflow-hidden rounded-md border border-white/10 bg-black/18">
        {#if scanRoots.length}
          {#each scanRoots as root}
            <div class="grid min-h-14 grid-cols-[minmax(0,1fr)_92px] items-center gap-4 border-b border-white/[0.06] px-4 last:border-b-0">
              <div class="min-w-0">
                <p class="truncate text-sm font-bold text-white">{folderName(root)}</p>
                <p class="truncate text-xs text-white/38">{root}</p>
              </div>
              <button
                class="h-8 rounded-md border border-red-200/18 px-3 text-xs font-bold text-red-100/74 transition hover:border-red-200/34 hover:bg-red-500/12 hover:text-red-50 disabled:cursor-not-allowed disabled:opacity-40"
                type="button"
                disabled={isScanning}
                on:click={() => removeFolder(root)}
              >
                Remove
              </button>
            </div>
          {/each}
        {:else}
          <div class="px-4 py-8 text-sm text-white/44">
            No folders added yet.
          </div>
        {/if}
      </div>

      <p class="mt-3 text-xs text-white/34">
        Removing a folder keeps the files on disk. It only removes that folder from Orca and drops its songs from the library.
      </p>
    </section>
  {:else if activeTab === 'Audio'}
    <section class="max-w-[820px] space-y-5">
      <div class="grid grid-cols-[1fr_44px] items-center gap-5">
        <div>
          <h3 class="text-sm font-bold text-white">Song quality info</h3>
          <p class="text-sm text-white/48">Show format, sample rate, and bitrate in rows and players</p>
        </div>
        <button
          class={`relative h-6 w-11 rounded-full border transition ${showQualityInfo ? 'toggle-switch-on' : 'toggle-switch-off'}`}
          title="Song quality info"
          on:click={() => onShowQualityInfoChange(!showQualityInfo)}
        >
          <span class={`toggle-knob absolute top-1 h-4 w-4 rounded-full transition ${showQualityInfo ? 'left-6' : 'left-1'}`}></span>
        </button>
      </div>

      <div class="grid grid-cols-[1fr_44px] items-center gap-5 border-t border-white/10 pt-5">
        <div>
          <h3 class="text-sm font-bold text-white">Gapless playback</h3>
          <p class="text-sm text-white/48">Preload the next track near the end for smoother transitions</p>
        </div>
        <button
          class={`relative h-6 w-11 rounded-full border transition ${gaplessPlayback ? 'toggle-switch-on' : 'toggle-switch-off'}`}
          title="Gapless playback"
          on:click={() => onGaplessPlaybackChange(!gaplessPlayback)}
        >
          <span class={`toggle-knob absolute top-1 h-4 w-4 rounded-full transition ${gaplessPlayback ? 'left-6' : 'left-1'}`}></span>
        </button>
      </div>
    </section>
  {:else if activeTab === 'About'}
    <section class="max-w-[620px]">
      <div class="flex items-baseline gap-2.5">
        <h3 class="text-xl font-black tracking-[-0.04em] text-white">Orca</h3>
        <span class="text-sm font-semibold text-white/42">{releaseLabel}</span>
      </div>
      <p class="mt-2 text-sm text-white/52">Local-first music player, built for listening to the library you already own.</p>

      <div class="mt-7 flex flex-wrap items-center justify-between gap-4 border-t border-white/10 pt-5">
        <p class="text-sm text-white/58">Found a bug or have an idea? Feedback helps shape Orca.</p>
        <button class="inline-flex h-9 items-center gap-2 rounded-md border border-white/12 bg-white/[0.055] px-3.5 text-xs font-bold text-white transition hover:border-white/30 hover:bg-white/[0.1]" type="button" on:click={openFeedback}>
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M12 2C6.477 2 2 6.477 2 12a10 10 0 0 0 6.838 9.488c.5.092.682-.217.682-.483 0-.237-.009-.866-.014-1.7-2.782.604-3.369-1.34-3.369-1.34-.455-1.156-1.11-1.464-1.11-1.464-.908-.62.069-.608.069-.608 1.004.07 1.532 1.03 1.532 1.03.892 1.529 2.341 1.087 2.91.831.091-.646.349-1.087.635-1.337-2.22-.253-4.555-1.11-4.555-4.943 0-1.092.39-1.986 1.029-2.686-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.026A9.55 9.55 0 0 1 12 6.8a9.55 9.55 0 0 1 2.504.337c1.91-1.295 2.748-1.026 2.748-1.026.546 1.377.203 2.394.1 2.647.64.7 1.028 1.594 1.028 2.686 0 3.842-2.339 4.687-4.566 4.935.359.31.678.92.678 1.852 0 1.337-.012 2.417-.012 2.747 0 .268.18.58.688.482A10.002 10.002 0 0 0 22 12c0-5.523-4.477-10-10-10Z" />
          </svg>
          Report on GitHub
        </button>
      </div>
    </section>
  {/if}
</div>
