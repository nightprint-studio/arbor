<script lang="ts">
  /**
   * ParkedModalsDock — floating panel anchored above the status bar,
   * listing all currently-parked modals. Toggled by the matching badge
   * in <StatusBar>; mounts only when `uiStore.parkedModalsOverlayOpen`
   * is true. Empty state renders an inline hint so the user understands
   * what the panel is for when they open it with nothing parked.
   *
   * Clicking an entry runs its `execute` action — the action knows how
   * to re-open the original modal (switching to its source tab if
   * necessary, opening the project from the registry if the tab was
   * closed). A per-entry `pending` flag drives the chip spinner so
   * async dispatch (IPC, tab open) is visible to the user.
   */
  import { X, Minimize2, Loader } from 'lucide-svelte';
  import { parkedModalsStore } from '$lib/stores/parked-modals.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // Ids currently mid-dispatch — drives the spinner + disables click.
  let pendingIds = $state<Set<string>>(new Set());

  async function restore(id: string) {
    if (pendingIds.has(id)) return;
    const entry = parkedModalsStore.entries.find(e => e.id === id);
    if (!entry) return;
    pendingIds = new Set([...pendingIds, id]);
    try {
      await entry.execute();
      // Action succeeded — the modal is back on screen, drop the chip
      // and close the panel.
      parkedModalsStore.unpark(id);
      uiStore.setParkedModalsOverlayOpen(false);
    } catch (err) {
      // The action failed (tab couldn't be reopened, project no longer
      // registered, …). Leave the chip in place so the user can retry
      // or dismiss it explicitly — auto-dropping would erase the only
      // breadcrumb pointing back to the workflow.
      const msg = err instanceof Error ? err.message : String(err);
      uiStore.showToast(`Couldn't restore dialog: ${msg}`, 'error');
    } finally {
      const next = new Set(pendingIds);
      next.delete(id);
      pendingIds = next;
    }
  }

  function dismiss(id: string) {
    parkedModalsStore.unpark(id);
  }
</script>

<button
  type="button"
  class="overlay-backdrop"
  aria-label="Close minimized dialogs panel"
  onclick={() => uiStore.setParkedModalsOverlayOpen(false)}
></button>

<div class="overlay-panel parked-overlay" role="dialog" aria-label="Minimized dialogs">
  <div class="overlay-header">
    <span class="overlay-title">
      <Minimize2 size={12} />
      Minimized dialogs
    </span>
    <button
      class="hdr-close"
      onclick={() => uiStore.setParkedModalsOverlayOpen(false)}
      aria-label="Close panel"
      use:tooltip={'Close'}
    >
      <X size={13} />
    </button>
  </div>

  {#if parkedModalsStore.count === 0}
    <div class="empty">
      <p>No minimized dialogs.</p>
      <p class="empty-hint">Use the <span class="kbd-hint">−</span> button in a dialog header to park it here.</p>
    </div>
  {:else}
    <ul class="entry-list">
      {#each parkedModalsStore.entries as entry (entry.id)}
        {@const isPending = pendingIds.has(entry.id)}
        <li class="entry" class:entry-pending={isPending}>
          <button
            type="button"
            class="entry-main"
            disabled={isPending}
            onclick={() => restore(entry.id)}
            use:tooltip={'Restore dialog'}
          >
            {#if isPending}
              <Loader size={13} class="spin" />
            {:else if entry.icon}
              {@const IconCmp = entry.icon}
              <IconCmp size={13} />
            {/if}
            <span class="entry-title">{entry.title}</span>
          </button>
          <button
            type="button"
            class="entry-close"
            disabled={isPending}
            onclick={() => dismiss(entry.id)}
            aria-label="Close dialog"
            use:tooltip={'Close'}
          >
            <X size={12} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .parked-overlay {
    width: 300px;
    max-height: 380px;
    background: var(--bg-base);
    border-color: var(--border);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.7);
  }

  .hdr-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-close:hover { background: var(--bg-elevated); color: var(--text-primary); }

  /* ── Empty state ─────────────────────────────────────────────────────── */
  .empty {
    padding: 18px 16px;
    color: var(--text-muted);
    font-size: 12px;
    text-align: center;
  }
  .empty p { margin: 0; }
  .empty-hint { margin-top: 6px !important; color: var(--text-disabled); font-size: 11px; }
  .kbd-hint {
    display: inline-block;
    padding: 0 5px;
    margin: 0 1px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-mono);
    font-size: 10px;
    line-height: 14px;
  }

  /* ── List ────────────────────────────────────────────────────────────── */
  .entry-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  .entry {
    display: flex;
    align-items: stretch;
    height: 28px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    transition: background var(--transition-fast);
  }
  .entry:hover { background: var(--bg-elevated); }
  .entry-pending { opacity: 0.7; }

  .entry-main {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 8px;
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: color var(--transition-fast);
  }
  .entry-main:hover:not(:disabled) { color: var(--text-primary); }
  .entry-main:disabled { cursor: progress; }

  .entry-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .entry-close:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-primary);
  }
  .entry-close:disabled { cursor: progress; }

  :global(.spin) { animation: spin 1s linear infinite; }
</style>
