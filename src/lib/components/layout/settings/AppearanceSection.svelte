<script lang="ts">
  import { Palette } from 'lucide-svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onOpenThemeEditor }: { onOpenThemeEditor: () => void } = $props();

  let fontScale = $state(parseFloat(localStorage.getItem('arbor:font-scale') ?? '1'));

  const FONT_PRESETS = [0.85, 1.0, 1.15, 1.3];

  $effect(() => {
    document.documentElement.style.setProperty('--font-scale', String(fontScale));
    localStorage.setItem('arbor:font-scale', String(fontScale));
  });

  function isPreset(p: number) {
    return Math.abs(fontScale - p) < 0.005;
  }
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

  <FormRow label="Font scale" description="Scales all UI text proportionally">
    <div class="inline-control">
      <input
        type="range"
        min="0.8"
        max="1.4"
        step="0.05"
        bind:value={fontScale}
        class="slider"
      />
      <div class="preset-row">
        {#each FONT_PRESETS as p}
          <button
            type="button"
            class="preset-btn"
            class:active={isPreset(p)}
            onclick={() => fontScale = p}
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
