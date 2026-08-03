<script lang="ts">
  /**
   * BennuTerminalView — the Terminal section of the bottom dock.
   *
   * Reuses Corvus's generic {@link TerminalInstance} (xterm.js over the *platform*
   * terminal IPC — `terminal_create` / `terminal_write` / … route to the shared
   * shell layer, not to corvus-be, so the same PTY works from any window) and the
   * corvus `terminalStore` (a module singleton, but each window is its own JS
   * context → Bennu gets an independent instance, no cross-window collision).
   *
   * Only the two Corvus-window couplings from `TerminalPanel` are swapped out: the
   * cwd comes from Bennu's project store, and failures toast through Bennu's
   * feedback host. It behaves identically to Corvus's terminal otherwise.
   *
   * The panel owns its header (title + "New terminal" + close) and renders the
   * session tab strip (when >1) above the terminal bodies.
   */
  import { Plus, TerminalSquare } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import TerminalInstance from '$lib/components/corvus/terminal/TerminalInstance.svelte';
  import { terminalCreate, terminalClose } from '$lib/ipc/corvus/terminal';
  import { terminalStore } from '$lib/stores/corvus/terminal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  let creating = $state(false);

  function currentCwd(): string | undefined {
    return projectStore.project?.root ?? undefined;
  }

  async function openTerminal(shellId?: string) {
    if (creating) return;
    creating = true;
    try {
      const info = await terminalCreate({ shell: shellId, cwd: currentCwd() });
      terminalStore.addTab(info.id, info.shell, info.cwd);
    } catch (err) {
      toastStore.show(`Failed to open terminal: ${err}`, 'error');
    } finally {
      creating = false;
    }
  }

  async function closeTab(id: string) {
    terminalStore.removeTab(id);
    try { await terminalClose(id); } catch { /* already gone */ }
  }

  const items = $derived<TabItem[]>(
    terminalStore.tabs.map((t) => ({
      id: t.id, label: t.title, title: `${t.title} — ${t.cwd}`, closable: true, data: t,
    })),
  );
</script>

<div class="tv">
  <BottomPanelHeader title="Terminal" onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<TerminalSquare size={13} />{/snippet}
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'New terminal'}
        aria-label="New terminal"
        disabled={creating}
        onclick={() => void openTerminal()}
      >
        <Plus size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  {#if terminalStore.tabs.length === 0}
    <div class="tv-empty">
      <TerminalSquare size={26} />
      <p>No terminal open</p>
      <button class="tv-open" onclick={() => openTerminal()} disabled={creating}>
        <Plus size={12} /> New Terminal
      </button>
    </div>
  {:else}
    {#if terminalStore.tabs.length > 1}
      <div class="tv-tabs">
        <Tabs
          {items}
          value={terminalStore.activeId}
          variant="panel"
          size="sm"
          closable
          ariaLabel="Terminal sessions"
          onSelect={(id) => terminalStore.setActive(id)}
          onClose={(id) => closeTab(id)}
        />
      </div>
    {/if}
    <div class="tv-body">
      {#each terminalStore.tabs as tab (tab.id)}
        <TerminalInstance id={tab.id} active={tab.id === terminalStore.activeId} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .tv {
    display: flex; flex-direction: column;
    height: 100%; width: 100%; min-height: 0;
    background: var(--terminal-bg, var(--bg-base));
    overflow: hidden;
  }
  .tv-tabs {
    display: flex; align-items: stretch;
    height: 28px; min-height: 28px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .tv-tabs :global(.tabs) { flex: 1; min-width: 0; }
  .tv-body { flex: 1; min-height: 0; position: relative; overflow: hidden; }

  .tv-empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%; gap: 10px;
    color: var(--text-disabled); font-size: var(--font-size-sm);
  }
  .tv-open {
    display: flex; align-items: center; gap: 5px;
    padding: 6px 14px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-secondary);
    font-size: var(--font-size-sm); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tv-open:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .tv-open:disabled { opacity: 0.5; cursor: default; }
</style>
