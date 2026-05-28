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
  export let theme: 'default' = 'default';
  export let onThemeChange: (theme: 'default') => void = () => {};

  let activeTab = 'Appearance';
  const tabs = ['Appearance', 'Interface', 'Library', 'Audio'];
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

  function updateFontSize(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    onFontSizePercentChange(Number(target.value));
  }

  function folderName(path: string) {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }

  async function removeFolder(root: string) {
    if (!window.confirm(`Remove "${folderName(root)}" from Orca? Songs from this folder will be removed from the library.`)) {
      return;
    }

    await onRemoveScanRoot(root);
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
    <section class="max-w-[820px]">
      <div class="grid grid-cols-[1fr_44px] items-center gap-5 border-t border-white/10 pt-5">
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
    </section>
  {/if}
</div>
