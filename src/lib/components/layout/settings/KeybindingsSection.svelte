<script lang="ts">
  import { Info, RotateCcw } from 'lucide-svelte';
  import { DEFAULT_KEYBINDINGS, GROUP_ORDER, formatBinding, type Keybinding } from '$lib/utils/keybindings';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import type { PluginKeybinding } from '$lib/types/plugin';
  import { tooltip } from '$lib/actions/tooltip';

  const pluginKeybindings = $derived(
    contributionStore.forPoint('arbor:keybinding')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map((c): PluginKeybinding => {
        const p = c.payload as { key?: string; ctrl?: boolean; shift?: boolean; alt?: boolean; action?: string; description?: string };
        return {
          plugin_name: c.plugin_name,
          key:         p.key    ?? '',
          ctrl:        !!p.ctrl,
          shift:       !!p.shift,
          alt:         !!p.alt,
          action:      p.action ?? '',
          description: p.description ?? '',
        };
      })
  );
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Kbd           from '$lib/components/shared/ui/Kbd.svelte';

  let recordingAction = $state<string | null>(null);
  let _captureHandler: ((e: KeyboardEvent) => void) | null = null;

  function startRecording(action: string) {
    stopRecording();
    recordingAction = action;
    _captureHandler = captureKeybinding;
    window.addEventListener('keydown', _captureHandler, { capture: true });
  }

  function captureKeybinding(e: KeyboardEvent) {
    if (['Control', 'Shift', 'Alt', 'Meta', 'CapsLock'].includes(e.key)) return;
    e.preventDefault();
    e.stopImmediatePropagation();

    const action = recordingAction!;
    if (e.key === 'Escape') { stopRecording(); return; }

    const binding: Keybinding = {
      key: e.key,
      description: DEFAULT_KEYBINDINGS[action]?.description ?? '',
      group: DEFAULT_KEYBINDINGS[action]?.group ?? '',
    };
    if (e.ctrlKey || e.metaKey) binding.ctrl = true;
    if (e.shiftKey)              binding.shift = true;
    if (e.altKey)                binding.alt = true;

    keybindingsStore.setBinding(action, binding);
    stopRecording();
  }

  function stopRecording() {
    if (_captureHandler) {
      window.removeEventListener('keydown', _captureHandler, true);
      _captureHandler = null;
    }
    recordingAction = null;
  }

  /**
   * Adapt a PluginKeybinding (flat payload from arbor.keybinding.register)
   * to the standard Keybinding shape consumed by `<Kbd binding>`. Kept as a
   * local helper because PluginKeybinding has its own `action`/`plugin_name`
   * fields that don't belong on the generic Keybinding.
   */
  function pluginToBinding(kb: PluginKeybinding): Keybinding {
    return {
      key:   kb.key,
      ctrl:  kb.ctrl,
      shift: kb.shift,
      alt:   kb.alt,
      description: kb.description,
      group: 'Plugins',
    };
  }

  $effect(() => () => stopRecording());
</script>

<SectionHeader title="Keybindings" description="Click any shortcut to rebind it. Press Escape while recording to cancel." />

<div class="kb-toolbar">
  <div class="info-box kb-info-note">
    <Info size={12} />
    <span><kbd class="kb-key-inline">Escape</kbd> always closes panels/search and is not rebindable.</span>
  </div>
  <button class="btn-ghost-sm" onclick={() => keybindingsStore.resetAll()} use:tooltip={'Reset all keybindings to defaults'}>
    <RotateCcw size={11} /> Reset all
  </button>
</div>

{#each GROUP_ORDER as group}
  {@const entries = Object.entries(DEFAULT_KEYBINDINGS).filter(([, b]) => b.group === group)}
  <div class="kb-table">
    <div class="kb-group-header">{group}</div>
    {#each entries as [action, defaultBinding]}
      {@const binding = keybindingsStore.getBinding(action)}
      {@const customized = keybindingsStore.isCustomized(action)}
      {@const isRecording = recordingAction === action}
      <div class="kb-row" class:kb-recording-row={isRecording}>
        <span class="kb-action">{defaultBinding.description}</span>
        <div class="kb-right">
          <button
            class="kb-badge-btn"
            class:is-recording={isRecording}
            class:is-customized={customized}
            onclick={() => isRecording ? stopRecording() : startRecording(action)}
            use:tooltip={isRecording ? 'Press a key combination — Escape to cancel' : 'Click to rebind'}
          >
            {#if isRecording}
              <span class="kb-recording-label">Press a shortcut…</span>
            {:else}
              <kbd class="kb-key">{formatBinding(binding)}</kbd>
            {/if}
          </button>
          <div class="kb-reset-cell">
            {#if customized}
              <button
                class="kb-reset-btn"
                onclick={() => keybindingsStore.resetBinding(action)}
                use:tooltip={`Reset to default: ${formatBinding(defaultBinding)}`}
              >
                <RotateCcw size={10} />
              </button>
            {/if}
          </div>
        </div>
      </div>
    {/each}
  </div>
{/each}

{#if pluginKeybindings.length > 0}
  <div class="kb-table kb-table-plugins">
    <div class="kb-group-header kb-group-header-plugins">
      Plugins
      <span class="kb-readonly-tag">read-only · registered by plugins</span>
    </div>
    {#each pluginKeybindings as kb (kb.plugin_name + ':' + kb.action)}
      <div class="kb-row">
        <span class="kb-action">{kb.description || kb.action}</span>
        <div class="kb-right">
          <div class="kb-badge-btn kb-readonly" use:tooltip={`Registered by plugin: ${kb.plugin_name}`}>
            <Kbd binding={pluginToBinding(kb)} size="sm" />
          </div>
          <div class="kb-plugin-tag">{kb.plugin_name}</div>
          <div class="kb-reset-cell"></div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .kb-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: space-between;
    margin-bottom: -4px;
  }

  .kb-info-note {
    flex: 1;
    padding: 7px 12px;
    font-size: 11px;
  }

  .kb-table {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .kb-group-header {
    padding: 6px 14px;
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 10px;
    font-weight: 600;
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }

  .kb-row {
    display: flex;
    align-items: center;
    padding: 7px 14px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 12px;
    gap: 12px;
    transition: background var(--transition-fast);
  }
  .kb-row:last-child { border-bottom: none; }
  .kb-row:hover:not(.kb-recording-row) { background: rgba(255,255,255,0.02); }
  .kb-row.kb-recording-row { background: rgba(77,120,204,0.07); }

  .kb-action {
    flex: 1;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .kb-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .kb-badge-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    cursor: pointer;
    border-radius: var(--radius-sm);
    padding: 2px 4px;
    min-width: 80px;
    transition: all var(--transition-fast);
  }
  .kb-badge-btn:hover:not(.is-recording) { background: var(--bg-overlay); }
  .kb-badge-btn.is-recording { cursor: default; }

  kbd.kb-key {
    display: inline-block;
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    padding: 1px 7px;
    color: var(--text-primary);
    white-space: nowrap;
    transition: border-color var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
  }
  .kb-badge-btn:hover:not(.is-recording) kbd.kb-key {
    border-color: var(--accent);
    color: var(--accent);
  }
  .kb-badge-btn.is-customized kbd.kb-key {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-subtle);
  }

  .kb-key-inline {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    padding: 0 5px;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .kb-readonly {
    cursor: default;
    pointer-events: none;
    opacity: 0.75;
  }

  .kb-table-plugins {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 20%, var(--border));
    border-radius: var(--radius-md);
    margin-top: 4px;
  }

  .kb-group-header-plugins {
    color: color-mix(in srgb, var(--accent) 70%, var(--text-secondary));
    border-bottom-color: color-mix(in srgb, var(--accent) 20%, var(--border));
    letter-spacing: 0.04em;
  }

  .kb-readonly-tag {
    font-size: 10px;
    font-weight: 400;
    color: color-mix(in srgb, var(--accent) 50%, var(--text-disabled));
    margin-left: 8px;
    text-transform: none;
    letter-spacing: 0;
    font-family: var(--font-ui-sans);
    font-style: italic;
  }

  .kb-plugin-tag {
    font-size: 10px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm);
    padding: 0 5px;
    line-height: 18px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 120px;
  }

  .kb-recording-label {
    font-size: 11px;
    color: var(--accent);
    font-family: var(--font-ui-sans);
    font-style: italic;
    white-space: nowrap;
    animation: kb-pulse 1.1s ease-in-out infinite;
  }

  @keyframes kb-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.5; }
  }

  .kb-reset-cell {
    width: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .kb-reset-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-disabled);
    transition: all var(--transition-fast);
  }
  .kb-reset-btn:hover {
    color: var(--warning);
    background: rgba(226, 163, 53, 0.12);
  }
</style>
