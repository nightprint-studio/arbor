<script lang="ts">
  /**
   * The appearance settings — for every product, not just Corvus.
   *
   * ## Why this is shared
   *
   * All of it was already global. `appearanceStore` reads and writes
   * `~/.config/arbor/config.toml`, and every product window calls `loadConfig()` on mount, so
   * the font scale, the window-control style and the compact title bar have always applied to
   * Bennu, merula, Picus and the explorer as much as to Corvus. The only thing that was not
   * shared was the *dialog*: turning any of it on meant opening Corvus, changing it there, and
   * coming back — for a setting that had already changed under you.
   *
   * ## What is optional, and why it is not simply always shown
   *
   * Two rows are gated rather than global, because they name a behaviour a product must
   * actually implement. `activityBar` mirrors or hides Corvus's rail through its `AppShell`;
   * `compactFileTree` collapses single-child folder chains in Corvus's three file lists.
   * Showing either in a product that ignores it would be a switch that does nothing, which is
   * worse than an absent one: an absent setting is a missing feature, a dead one is a bug.
   */
  import { Palette, LayoutDashboard } from 'lucide-svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore, PARKED_MODALS_MAX_MIN, PARKED_MODALS_MAX_MAX } from '$lib/stores/appearance.svelte';
  import type { WindowControlsStyle, ActivityBarPosition } from '$lib/types/config';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { isMac } from '$lib/utils/platform';
  import SettingsCard from './SettingsCard.svelte';

  let {
    onOpenThemeEditor,
    onCustomizeBars,
    activityBar = false,
    compactFileTree = false,
    showHeader = true,
  }: {
    /** Opens the theme editor. The theme row is hidden when a product has no route to one. */
    onOpenThemeEditor?: () => void;
    /** Opens this product's "Customize Activity Bar" dialog, when it has one. */
    onCustomizeBars?: () => void;
    /** This product mirrors / hides its icon rail from `activity_bar_position`. */
    activityBar?: boolean;
    /** This product collapses single-child folder chains in its file lists. */
    compactFileTree?: boolean;
    /** Some shells draw their own section header. */
    showHeader?: boolean;
  } = $props();

  const FONT_PRESETS = [0.85, 1.0, 1.15, 1.3];

  // Read-through to the store, so a change made anywhere else (the Command Palette scales the
  // font too) moves the slider here without this component being told.
  const fontScale           = $derived(appearanceStore.fontScale);
  const activityBarPos      = $derived(appearanceStore.activityBarPosition);
  const compactTitleBar     = $derived(appearanceStore.compactTitleBar);
  const parkedModalsMax     = $derived(appearanceStore.parkedModalsMax);
  const compactFileTreeDirs = $derived(appearanceStore.compactFileTreeDirs);

  function onScaleInput(e: Event) {
    const n = parseFloat((e.target as HTMLInputElement).value);
    if (Number.isFinite(n)) appearanceStore.setFontScale(n);
  }

  function isPreset(p: number) {
    return Math.abs(fontScale - p) < 0.005;
  }

  const WC_OPTIONS = [
    { value: 'mac',     label: 'Mac-inspired',  description: 'Coloured trio on the left, with the zoom menu.' },
    { value: 'windows', label: 'Windows',       description: 'Flat rectangular controls on the right.'        },
  ];

  const ACTIVITY_BAR_OPTIONS = [
    { value: 'left',   label: 'Left',   description: 'Built-in bar on the left edge (default).' },
    { value: 'right',  label: 'Right',  description: 'Mirror layout — built-in bar on the right.' },
    { value: 'hidden', label: 'Hidden', description: 'Collapsed; hover the edge to reveal.' },
  ];
</script>

{#if showHeader}
  <SectionHeader
    title="Appearance"
    description="The look of the interface. These settings are shared by every Arbor window."
  />
{/if}

<SettingsCard>
  {#if onOpenThemeEditor}
    <FormRow label="Color theme" description="Active theme applied across the entire UI">
      <div class="theme-row">
        <span class="theme-name">{themeStore.activeTheme.name}</span>
        <button class="btn-open-editor" onclick={onOpenThemeEditor}>
          <Palette size={13} />
          Open Theme Editor
        </button>
      </div>
    </FormRow>
  {/if}

  <!-- macOS paints the real traffic lights over the title bar, so this faux-control
       toggle has no effect there and is hidden. -->
  {#if !isMac}
    <FormRow label="Window controls" description="Style and side of the close/minimize/maximize buttons in the title bar.">
      <RadioGroup
        value={appearanceStore.windowControlsStyle}
        options={WC_OPTIONS}
        appearance="segment"
        size="sm"
        onchange={(v) => appearanceStore.setWindowControlsStyle(v as WindowControlsStyle)}
      />
    </FormRow>
  {/if}

  <FormRow label="Compact title bar" description="Reduce the title-bar height for narrow displays.">
    <Toggle checked={compactTitleBar} onchange={(v) => appearanceStore.setCompactTitleBar(v)} />
  </FormRow>

  {#if compactFileTree}
    <FormRow
      label="Compact file tree folders"
      description="IntelliJ-style — collapse chains of single-child folders into one row across the file panel, stage area, and commit detail file list. Conflict lists always compact regardless of this setting."
    >
      <Toggle checked={compactFileTreeDirs} onchange={(v) => appearanceStore.setCompactFileTreeDirs(v)} />
    </FormRow>
  {/if}

  <FormRow
    label="Minimized dialogs cap"
    description="Maximum number of dialogs that can sit minimized in the status-bar panel at the same time. New minimize attempts past this cap are refused with a toast — no parked dialog is auto-closed."
  >
    <NumberStepper
      value={parkedModalsMax}
      min={PARKED_MODALS_MAX_MIN}
      max={PARKED_MODALS_MAX_MAX}
      step={1}
      onchange={(v) => appearanceStore.setParkedModalsMax(v)}
      ariaLabel="Minimized dialogs cap"
    />
  </FormRow>

  {#if activityBar}
    <FormRow label="Activity bar" description="Position of the icon rail. Hidden collapses the bar — hover the screen edge to bring it back temporarily.">
      <RadioGroup
        value={activityBarPos}
        options={ACTIVITY_BAR_OPTIONS}
        appearance="segment"
        size="sm"
        onchange={(v) => appearanceStore.setActivityBarPosition(v as ActivityBarPosition)}
      />
    </FormRow>
  {/if}

  {#if onCustomizeBars}
    <FormRow label="Activity bar contents" description="Reorder the icons on the rails, and hide the tools you do not use.">
      <button class="btn-open-editor" onclick={onCustomizeBars}>
        <LayoutDashboard size={13} />
        Customize Activity Bar…
      </button>
    </FormRow>
  {/if}

  <FormRow label="Font scale" description="Scales all UI text proportionally">
    <div class="inline-control">
      <input
        type="range"
        min="0.8"
        max="1.4"
        step="0.05"
        value={fontScale}
        oninput={onScaleInput}
        class="slider"
      />
      <div class="preset-row">
        {#each FONT_PRESETS as p}
          <button
            type="button"
            class="preset-btn"
            class:active={isPreset(p)}
            onclick={() => appearanceStore.setFontScale(p)}
            use:tooltip={`Set to ${(p * 100).toFixed(0)}%`}
          >
            {(p * 100).toFixed(0)}%
          </button>
        {/each}
      </div>
      <span class="value-chip">{(fontScale * 100).toFixed(0)}%</span>
    </div>
  </FormRow>
</SettingsCard>

<style>
  .theme-row { display: flex; align-items: center; gap: 10px; }

  .theme-name {
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-weight: 500;
  }

  .btn-open-editor {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .btn-open-editor:hover {
    background: var(--bg-hover);
    color: var(--accent);
    border-color: var(--accent);
  }

  /* Owned here rather than by the host shell: Corvus's settings panel used to supply the gap
     as a `:global` rule, which made the row look right in exactly one of the five places this
     now renders. */
  .inline-control { display: flex; align-items: center; gap: 10px; }

  .preset-row {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .preset-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-2xs);
    font-weight: 500;
    padding: 3px 7px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    min-width: 32px;
  }
  .preset-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
  .preset-btn.active { background: var(--accent-subtle); color: var(--accent); }
</style>
