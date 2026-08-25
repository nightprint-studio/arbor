<script lang="ts">
  /**
   * Garrulus footer — the IntelliJ-style status strip.
   *
   * Left: the vault and how much is in it. Right: the path of the note in front
   * of you, then the shared feedback badges injected by the window.
   *
   * Scaffolding: the sync summary ("allineato · casa, 3 min fa") and the task
   * counter belong here too, and arrive with the domains that can answer them.
   * Nothing is shown as a placeholder — a footer that states a fact it invented
   * is worse than one that states fewer facts.
   */
  import { NotebookPen } from 'lucide-svelte';
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Display name of the open vault, or `null` when none is open. */
    vaultName?: string | null;
    /** Notes indexed at open. */
    noteCount?: number | null;
    /** Vault-relative path of the note on screen. */
    notePath?: string | null;
    /** Window-owned badges (toasts, progress) appended at the far right. */
    footerExtra?: Snippet;
  }

  let { vaultName = null, noteCount = null, notePath = null, footerExtra }: Props = $props();
</script>

<div class="gf">
  {#if vaultName}
    <span class="gf-item">{vaultName}</span>
    {#if noteCount !== null}
      <span class="gf-item gf-muted">
        <NotebookPen size={13} />
        {noteCount} {noteCount === 1 ? 'note' : 'notes'}
      </span>
    {/if}
  {:else}
    <span class="gf-item gf-muted">No vault open</span>
  {/if}

  <span class="gf-spacer"></span>

  {#if notePath}
    <span class="gf-item gf-muted gf-path" use:tooltip={notePath}>{notePath}</span>
  {/if}

  {#if footerExtra}{@render footerExtra()}{/if}
</div>

<style>
  .gf {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    flex-shrink: 0;
    padding: 0 8px;
    background: var(--bg-elevated);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }

  .gf-item {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 6px;
    height: 20px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }

  .gf-muted { color: var(--text-muted); }

  .gf-path {
    max-width: 46ch;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
    line-height: 20px;
  }

  .gf-spacer { flex: 1; min-width: 0; }
</style>
