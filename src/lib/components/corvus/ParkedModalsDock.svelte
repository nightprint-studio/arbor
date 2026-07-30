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
  import { X, Minimize2, Loader, AppWindow } from 'lucide-svelte';
  import {
    parkedModalsStore,
    resolveParkedAccent,
    type ParkedModalEntry,
  } from '$lib/stores/parked-modals.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // Ids currently mid-dispatch — drives the spinner + disables click.
  let pendingIds = $state<Set<string>>(new Set());

  // Roving keyboard focus across the entry rows (↑/↓ move, Enter restores via
  // the button itself). Index is clamped whenever the list changes.
  let focusIdx = $state(0);
  let listEl = $state<HTMLUListElement | null>(null);

  const entries = $derived(parkedModalsStore.entries);

  // Keep the roving-focus index in range as entries are dismissed/added, so a
  // row is always tabbable (tabindex=0) and Tab never skips the whole list.
  $effect(() => {
    if (entries.length === 0) return;
    if (focusIdx > entries.length - 1) focusIdx = entries.length - 1;
    else if (focusIdx < 0) focusIdx = 0;
  });

  function accentVarFor(entry: ParkedModalEntry): string {
    const accent = resolveParkedAccent(entry);
    // `danger` maps to the `--error` token; the others match 1:1.
    return accent === 'danger' ? 'error' : accent;
  }

  function focusRow(idx: number) {
    if (!listEl || entries.length === 0) return;
    const clamped = Math.max(0, Math.min(idx, entries.length - 1));
    focusIdx = clamped;
    const rows = listEl.querySelectorAll<HTMLButtonElement>('.entry-main');
    rows[clamped]?.focus();
  }

  function onListKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      focusRow(focusIdx + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      focusRow(focusIdx - 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      focusRow(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      focusRow(entries.length - 1);
    }
  }

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
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul class="entry-list" bind:this={listEl} onkeydown={onListKeydown}>
      {#each entries as entry, i (entry.id)}
        {@const isPending = pendingIds.has(entry.id)}
        {@const IconCmp = entry.icon ?? AppWindow}
        <li
          class="entry"
          class:entry-pending={isPending}
          style="--chip-accent: var(--{accentVarFor(entry)});"
        >
          <button
            type="button"
            class="entry-main"
            disabled={isPending}
            tabindex={i === focusIdx ? 0 : -1}
            onfocus={() => (focusIdx = i)}
            onclick={() => restore(entry.id)}
            use:tooltip={'Restore dialog'}
          >
            <span class="entry-chip" class:pending={isPending}>
              {#if isPending}
                <Loader size={14} class="spin" />
              {:else}
                <IconCmp size={14} />
              {/if}
            </span>
            <span class="entry-text">
              <span class="entry-title">{entry.title}</span>
              {#if entry.subtitle}
                <span class="entry-subtitle">{entry.subtitle}</span>
              {/if}
            </span>
          </button>
          <button
            type="button"
            class="close-btn entry-close"
            disabled={isPending}
            tabindex={i === focusIdx ? 0 : -1}
            onclick={() => dismiss(entry.id)}
            aria-label="Discard minimized dialog"
            use:tooltip={'Discard'}
          ></button>
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
    font-size: var(--font-size-sm);
    text-align: center;
  }
  .empty p { margin: 0; }
  .empty-hint { margin-top: 6px !important; color: var(--text-disabled); font-size: var(--font-size-xs); }
  .kbd-hint {
    display: inline-block;
    padding: 0 5px;
    margin: 0 1px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-mono);
    font-size: var(--font-size-2xs);
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
    gap: 2px;
    min-height: 40px;
    padding-right: 6px;
    border-radius: var(--radius-md);
    /* Accent-tinted left strip + faint fill so each type reads as its own
       colour band; the accent is resolved per-entry via --chip-accent. */
    border-left: 2px solid color-mix(in srgb, var(--chip-accent) 65%, transparent);
    background: color-mix(in srgb, var(--chip-accent) 7%, transparent);
    transition: background var(--transition-fast);
  }
  .entry:hover {
    background: color-mix(in srgb, var(--chip-accent) 14%, transparent);
  }
  .entry-pending { opacity: 0.75; }

  .entry-main {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px 0 8px;
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-md);
  }
  .entry-main:disabled { cursor: progress; }
  .entry-main:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  /* Prominent, accent-tinted icon chip. */
  .entry-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--chip-accent) 20%, transparent);
    color: var(--chip-accent);
  }
  .entry-chip.pending { color: var(--text-muted); }

  .entry-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }
  .entry-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    line-height: 1.3;
  }
  .entry-subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
    line-height: 1.2;
  }

  /* Discard button — reuses the shared .close-btn (mac dot / windows X
     via [data-window-controls]); align it to the row and never let it grow. */
  .entry-close {
    align-self: center;
  }
  .entry-close:disabled { cursor: progress; opacity: 0.5; }

  :global(.spin) { animation: spin 1s linear infinite; }
</style>
