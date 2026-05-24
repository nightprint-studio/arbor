<script lang="ts">
  import { Palette } from 'lucide-svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import type {
    WindowControlsStyle, ActivityBarPosition,
  } from '$lib/types/config';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onOpenThemeEditor }: { onOpenThemeEditor: () => void } = $props();

  const FONT_PRESETS = [0.85, 1.0, 1.15, 1.3];

  // Reactive read-through to the store so the slider and chip auto-update
  // if any other surface (e.g. command palette) changes the scale.
  const fontScale          = $derived(appearanceStore.fontScale);
  const activityBarPos     = $derived(appearanceStore.activityBarPosition);
  const compactTitleBar    = $derived(appearanceStore.compactTitleBar);

  function onScaleInput(e: Event) {
    const n = parseFloat((e.target as HTMLInputElement).value);
    if (Number.isFinite(n)) appearanceStore.setFontScale(n);
  }

  function isPreset(p: number) {
    return Math.abs(fontScale - p) < 0.005;
  }

  const WC_OPTIONS = [
    { value: 'mac',     label: 'Mac-inspired',  description: 'Coloured trio (close/min/max).' },
    { value: 'windows', label: 'Windows',       description: 'Flat rectangular controls.'    },
  ];

  const ACTIVITY_BAR_OPTIONS = [
    { value: 'left',   label: 'Left',   description: 'Built-in bar on the left edge (default).' },
    { value: 'right',  label: 'Right',  description: 'Mirror layout — built-in bar on the right.' },
    { value: 'hidden', label: 'Hidden', description: 'Collapsed; hover the edge to reveal.' },
  ];
</script>

<SectionHeader title="Appearance" description="Customize the look and feel of the interface." />

<div class="card">
  <FormRow label="Color theme" description="Active theme applied across the entire UI">
    <div class="theme-row">
      <span class="theme-name">{themeStore.activeTheme.name}</span>
      <button class="btn-open-editor" onclick={onOpenThemeEditor}>
        <Palette size={13} />
        Open Theme Editor
      </button>
    </div>
  </FormRow>

  <FormRow label="Window controls" description="Style of the close/minimize/maximize buttons in the title bar. Position and size stay the same.">
    <RadioGroup
      value={appearanceStore.windowControlsStyle}
      options={WC_OPTIONS}
      appearance="segment"
      size="sm"
      onchange={(v) => appearanceStore.setWindowControlsStyle(v as WindowControlsStyle)}
    />
  </FormRow>

  <FormRow label="Compact title bar" description="Reduce the title-bar height for narrow displays.">
    <Toggle
      checked={compactTitleBar}
      onchange={(v) => appearanceStore.setCompactTitleBar(v)}
    />
  </FormRow>

  <FormRow label="Activity bar" description="Position of the icon rail. Hidden collapses the bar — hover the screen edge to bring it back temporarily.">
    <RadioGroup
      value={activityBarPos}
      options={ACTIVITY_BAR_OPTIONS}
      appearance="segment"
      size="sm"
      onchange={(v) => appearanceStore.setActivityBarPosition(v as ActivityBarPosition)}
    />
  </FormRow>

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
</div>

<style>
  .theme-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

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
    font-size: 10.5px;
    font-weight: 500;
    padding: 3px 7px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    min-width: 32px;
  }
  .preset-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
  .preset-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
  }
</style>
