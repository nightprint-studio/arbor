<script lang="ts">
  /**
   * Conflicts — the bottom-dock panel the sync button opens when it turns red.
   *
   * The design's central claim is that two PCs and one vault is Tuesday rather
   * than an edge case (`docs/garrulus-design.md` §4.4), and this panel is where
   * that claim is either kept or broken. Every conflict is shown in full — both
   * versions, side by side — with the three ways out on the same row.
   *
   * **The reassurance is the feature, so it is stated at the top and only once.**
   * Nothing was merged into any note, no merge marker was written anywhere, and
   * the other machine's version is sitting beside its note as an ordinary file:
   * that is what turns a conflict from an emergency into a decision, and it is
   * true of the whole list rather than of one row. Each card then names *its* side
   * file, which is the part that differs per note. Repeating the guarantee on
   * every card would make the one sentence that matters read like boilerplate.
   *
   * **The panel owns its data.** It asks `garrulus_conflicts` itself rather than
   * reading a store, because the list it renders is exactly one backend call and
   * has no other consumer; a store between the two would be a layer that can only
   * be out of date. It refreshes on `garrulus:sync-state`, which the backend emits
   * only when the state actually changed — conflicts appear and disappear with
   * that state and with nothing else, so it is both the cheapest and the only
   * correct trigger. Nothing here polls, and nothing here writes without a click,
   * which is the product rule the whole sync seam is built on (§4.2).
   */
  import { onMount } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { RefreshCw, ShieldCheck } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import ConflictCard from './ConflictCard.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import {
    conflicts as listConflicts,
    onSyncState,
    resolveConflict,
    type Conflict,
    type ConflictResolution,
  } from '$lib/ipc/garrulus';

  interface Props {
    /** Close the dock section. Omit → no close button (the host decides). */
    onClose?: () => void;
    /**
     * Skip the panel's own header — set by a host whose chrome already carries
     * one, which is what the dock's tab strip is. Without it the dock would draw
     * two title rows for one panel.
     */
    hideHeader?: boolean;
    /** Open a note in the editor — "merge by hand". Absent while no editor is
     *  mounted, and the button then says so instead of doing nothing. */
    onOpenNote?: (path: string) => void;
    /** One conflict was settled. The host re-probes the sync state and reloads
     *  whatever it shows of the note. */
    onResolved?: (path: string, resolution: ConflictResolution) => void;
  }

  let { onClose, hideHeader = false, onOpenNote, onResolved }: Props = $props();

  let items = $state<Conflict[]>([]);
  let error = $state<string | null>(null);
  let loaded = $state(false);
  /** The note whose resolution is in flight, so only its card goes inert. */
  let resolving = $state<string | null>(null);

  let listEl = $state<HTMLDivElement | undefined>();

  /** Re-read the conflict list. Exported so a host that owns the header — the
   *  dock — can put the refresh action there instead of duplicating one here. */
  export async function reload() {
    try {
      items = await listConflicts();
      error = null;
    } catch (e) {
      // A vault that cannot answer is a real state, not an empty list: saying
      // "no conflicts" when the backend is down is the one lie that would cost
      // text later.
      error = String(e);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    let off: UnlistenFn | null = null;
    let disposed = false;
    void reload();
    void onSyncState(() => { void reload(); })
      .then((fn) => { if (disposed) fn(); else off = fn; })
      .catch(() => { /* no dispatcher — the refresh button still works */ });
    return () => { disposed = true; off?.(); };
  });

  async function settle(conflict: Conflict, resolution: ConflictResolution) {
    if (resolving || !conflict.side_file) return;
    resolving = conflict.path;
    try {
      await resolveConflict(conflict.path, conflict.side_file, resolution);
      toastStore.show(
        resolution === 'mine'
          ? `Kept this machine’s version of ${conflict.path}.`
          : `Took the other version of ${conflict.path}.`,
        'success',
      );
      onResolved?.(conflict.path, resolution);
      await reload();
    } catch (e) {
      toastStore.show(`Could not resolve ${conflict.path}: ${e}`, 'error');
    } finally {
      resolving = null;
    }
  }

  /**
   * ↑/↓ jump between notes; Tab still walks the three actions inside one.
   *
   * With one conflict the tab order is enough. With ten it is not: reaching the
   * fourth note would mean tabbing through nine controls and every expandable
   * context block on the way.
   */
  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const cards = Array.from(listEl?.querySelectorAll<HTMLElement>('[data-conflict-card]') ?? []);
    if (cards.length === 0) return;
    const active = document.activeElement as HTMLElement | null;
    // `-1` (focus is not on any card) lands on the first one either way, which is
    // what an arrow key should do when the panel has just been shown.
    const here = cards.findIndex((c) => !!active && c.contains(active));
    const next = e.key === 'ArrowDown'
      ? Math.min(here + 1, cards.length - 1)
      : Math.max(here - 1, 0);
    const target = cards[next];
    const first = target?.querySelector<HTMLButtonElement>('button');
    if (!first) return;
    e.preventDefault();
    first.focus();
    target.scrollIntoView({ block: 'nearest' });
  }
</script>

<div class="cp">
  {#if !hideHeader}
    <BottomPanelHeader title="Conflicts" count={items.length} {onClose}>
      {#snippet actions()}
        <Button
          variant="icon"
          size="xs"
          ariaLabel="Re-read the conflict list"
          tooltip={{ content: 'Re-read the conflict list. Reads only — it settles nothing.' }}
          onclick={() => void reload()}
        >
          {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        </Button>
      {/snippet}
    </BottomPanelHeader>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="cp-body"
    bind:this={listEl}
    role="group"
    aria-label="Conflicting notes"
    onkeydown={onKeyDown}
  >
    {#if !loaded}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the conflict list…</span>
      </StateBlock>
    {:else if error}
      <StateBlock tone="error" label={error} />
    {:else if items.length === 0}
      <StateBlock
        tone="success"
        label="No conflicts. Both machines agree on every note."
      />
    {:else}
      <!-- Said once, for the whole list. See the header comment. -->
      <Alert variant="info" compact>
        <span class="cp-inline">
          <ShieldCheck size={13} />
          <span>
            <b>Nothing was written into your notes.</b> No merge markers, no half-merged
            text — every note below still holds the version this machine wrote, and the
            other machine's version is parked beside it in the vault as an ordinary note.
            The vault opens in Obsidian mid-conflict exactly as it does otherwise. Nothing
            changes until you choose here.
          </span>
        </span>
      </Alert>

      {#each items as conflict (conflict.path)}
        <ConflictCard
          {conflict}
          busy={resolving === conflict.path}
          onResolve={(resolution) => void settle(conflict, resolution)}
          {onOpenNote}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  .cp {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-base);
  }

  .cp-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }

  /* The body scrolls; nothing inside it shrinks to make room. Without this the
     banner and the cards are flex items that squash as the list grows, and a
     clipped diff is the one thing this panel cannot afford. */
  .cp-body > :global(*) { flex-shrink: 0; }

  .cp-inline { display: inline-flex; align-items: flex-start; gap: 6px; line-height: 1.5; }
  .cp-inline :global(svg) { margin-top: 2px; flex-shrink: 0; }
</style>
