<script lang="ts">
  import { GitCompare } from 'lucide-svelte';
  import { explorerStore } from '$lib/stores/explorer.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import GlobalShortcutCapture from '$lib/components/shared/internal/GlobalShortcutCapture.svelte';

  // Git awareness is a plain local toggle (setter is a no-op when unchanged).
  let gitAwareness = $state(explorerStore.gitAwareness);
  $effect(() => { explorerStore.setGitAwareness(gitAwareness); });

  // The global shortcut goes through async setters (the backend register can
  // fail on a taken combo); read straight from the store and toast on error.
  async function toggleShortcut(on: boolean) {
    try { await explorerStore.setGlobalShortcut(on); }
    catch (e) { uiStore.showToast(`Shortcut: ${e}`, 'error'); }
  }
  async function rebind(accel: string) {
    try { await explorerStore.setGlobalShortcutAccel(accel); }
    catch (e) { uiStore.showToast(`Shortcut: ${e}`, 'error'); }
  }
</script>

<SectionHeader
  title="File Explorer"
  description="Host-level switches for the built-in file explorer. Display preferences live inside the explorer itself — open its address bar and type arbor://settings, or press Ctrl+, while it's focused." />

<div class="card">
  <FormRow
    label="Git awareness"
    description="Show git status overlays, repo-root markers, the Changes panel and branch switching while browsing. Off by default — when off, the explorer issues no git checks, so plain file browsing stays fast.">
    <Toggle bind:checked={gitAwareness} />
  </FormRow>

  <FormRow
    label="Global shortcut"
    description="Register a system-wide hotkey that opens the dedicated explorer window even when Arbor isn't focused. Off by default. Click the chord to rebind it.">
    <GlobalShortcutCapture accel={explorerStore.globalShortcutAccel} disabled={!explorerStore.globalShortcut} onChange={rebind} />
    <Toggle checked={explorerStore.globalShortcut} onchange={toggleShortcut} />
  </FormRow>
</div>

<div class="ex-note">
  <GitCompare size={13} />
  <span>Tip: with git awareness on, right-click a folder for stage / discard / switch-branch, and use the Changes panel to review staged and unstaged files.</span>
</div>

<style>
  .ex-note {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 14px;
    padding: 10px 12px;
    border: 1px solid var(--border-subtle, var(--border));
    border-radius: var(--radius-md);
    background: var(--bg-subtle, var(--bg-elevated));
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.45;
  }
  .ex-note > :global(svg) { flex-shrink: 0; margin-top: 1px; color: var(--accent); }
</style>
