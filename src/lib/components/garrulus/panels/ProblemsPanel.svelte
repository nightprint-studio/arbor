<script lang="ts">
  /**
   * Problems — what the link graph can tell about the state of the vault.
   *
   * `garrulus_problems` answers two questions and this panel keeps them apart,
   * because they are not the same kind of fact:
   *
   *  • **an unresolved `[[link]]` is not an error.** In this dialect writing a
   *    link to a note that does not exist yet is how a note gets created
   *    (`docs/garrulus-design.md` §5.1, and the backend's own doc comment says so
   *    in as many words). So these rows are invitations, coloured as a prompt
   *    rather than as damage, and when the host can create notes each one offers
   *    to make the missing one;
   *  • **an orphan is a note nothing points at.** Not wrong either — a daily note
   *    is an orphan by nature — but it is the thing a knowledge base rots into,
   *    and worth being able to see.
   *
   * **The panel owns its data**, like `ConflictsPanel`: it is one backend call
   * with no other consumer, and a store between the two could only ever be out of
   * date. It reads on mount and when asked; nothing here polls and nothing here
   * writes.
   */
  import { onMount, untrack } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { FilePlus2, Link2Off, Unlink } from 'lucide-svelte';
  import ChipBar, { type ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { onVaultChanged, problems as ipcProblems, type VaultProblems } from '$lib/ipc/garrulus';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { noteFolder, noteName } from './note-path';

  interface Props {
    /** Open a note. Absent while no editor is mounted — the rows then read, and
     *  offer no verb that goes nowhere. */
    onOpenNote?: (id: string) => void;
    /** Create the note a link is waiting for. Absent → the invitation is stated
     *  and not offered, rather than offered and inert. */
    onCreateNote?: (title: string, from: string) => void;
  }

  let { onOpenNote, onCreateNote }: Props = $props();

  type Kind = 'all' | 'links' | 'orphans';

  let report = $state<VaultProblems | null>(null);
  let error = $state<string | null>(null);
  let loaded = $state(false);
  let kind = $state<Kind>('all');
  let listEl = $state<HTMLDivElement | null>(null);

  /** Re-read the report. A read: safe whenever it might have moved. */
  export async function reload(): Promise<void> {
    try {
      report = await ipcProblems();
      error = null;
    } catch (e) {
      // A vault that cannot answer is a real state, not a healthy vault.
      error = String(e);
      report = null;
    } finally {
      loaded = true;
    }
  }

  /**
   * Re-read whenever the vault moves under the panel.
   *
   * The link graph is a function of the notes, so a pull that applied six notes
   * or an edit made in Obsidian changes this report and nothing else would say
   * so. `garrulus:vault-changed` is already debounced by the backend's watcher —
   * one event per burst — so this is one re-read per burst, not one per file.
   */
  onMount(() => {
    let off: UnlistenFn | null = null;
    let disposed = false;
    void reload();
    void onVaultChanged(() => { void reload(); })
      .then((fn) => { if (disposed) fn(); else off = fn; })
      .catch(() => { /* no dispatcher — the refresh action still works */ });
    return () => { disposed = true; off?.(); };
  });

  /** A different vault is a different graph. Reading the previous one's problems
   *  against the new one's notes would name notes that are not there.
   *
   *  Seeded with the root at mount so this does not fire a second read next to
   *  the one `onMount` already started. */
  let lastRoot: string | null = garrulusVaultStore.root;
  $effect(() => {
    const root = garrulusVaultStore.root;
    if (root === lastRoot) return;
    lastRoot = root;
    untrack(() => {
      loaded = false;
      report = null;
      void reload();
    });
  });

  const unresolved = $derived(report?.unresolved ?? []);
  const orphans = $derived(report?.orphans ?? []);
  const total = $derived(unresolved.length + orphans.length);

  const chips = $derived<ChipItem[]>([
    { id: 'all', label: 'Everything', count: total, tone: 'neutral' },
    {
      id: 'links',
      label: 'Links with no note',
      count: unresolved.length,
      tone: 'warning',
      tooltip: 'A [[link]] whose note does not exist yet',
    },
    {
      id: 'orphans',
      label: 'Nothing links here',
      count: orphans.length,
      tone: 'muted',
      tooltip: 'Notes no other note points at',
    },
  ]);

  const showLinks = $derived(kind === 'all' || kind === 'links');
  const showOrphans = $derived(kind === 'all' || kind === 'orphans');

  /** ↑/↓ walk the rows; Enter is the row's own button. Same treatment as the
   *  conflicts list — with twenty rows, tabbing to the eighth is not navigation. */
  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const rows = Array.from(listEl?.querySelectorAll<HTMLElement>('[data-problem-row]') ?? []);
    if (rows.length === 0) return;
    const active = document.activeElement as HTMLElement | null;
    const here = rows.findIndex((r) => r === active || r.contains(active));
    const next = e.key === 'ArrowDown'
      ? Math.min(here + 1, rows.length - 1)
      : Math.max(here - 1, 0);
    const target = rows[next];
    if (!target) return;
    e.preventDefault();
    target.focus();
    target.scrollIntoView({ block: 'nearest' });
  }
</script>

<div class="pp">
  <div class="pp-bar">
    <ChipBar items={chips} selected={kind} size="sm" onSelect={(id) => (kind = id as Kind)} />
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="pp-body"
    bind:this={listEl}
    role="group"
    aria-label="Vault problems"
    onkeydown={onKeyDown}
  >
    {#if !loaded}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the link graph…</span>
      </StateBlock>
    {:else if error}
      <StateBlock tone="error" label={error} />
    {:else if total === 0}
      <StateBlock
        tone="success"
        label="Every link lands somewhere, and every note is linked from somewhere."
      />
    {:else}
      {#if showLinks && unresolved.length > 0}
        <div class="pp-head">Links with no note yet — {unresolved.length}</div>
        {#each unresolved as link (`${link.from}→${link.target}${link.heading ?? ''}`)}
          <div class="pp-row">
            <span class="pp-sev warn"><Link2Off size={12} /></span>
            <button
              type="button"
              class="pp-main"
              data-problem-row
              disabled={!onOpenNote}
              title={onOpenNote
                ? `Open ${noteName(link.from)}, where the link is written`
                : 'No editor is attached to this panel yet'}
              onclick={() => onOpenNote?.(link.from)}
            >
              <span class="pp-text">
                Nothing is called <b>{link.target}</b>{#if link.heading}<span class="pp-dim"
                  >#{link.heading}</span>{/if}{#if link.embed}<span class="pp-dim"> — embedded</span>{/if}
              </span>
              <span class="pp-where">
                {noteFolder(link.from)}<b>{noteName(link.from)}</b>
              </span>
            </button>
            {#if onCreateNote}
              <button
                type="button"
                class="pp-act"
                use:tooltip={'Create the note this link is waiting for'}
                aria-label="Create {link.target}"
                onclick={() => onCreateNote(link.target, link.from)}
              >
                <FilePlus2 size={12} />
              </button>
            {/if}
          </div>
        {/each}
      {/if}

      {#if showOrphans && orphans.length > 0}
        <div class="pp-head">Nothing links here — {orphans.length}</div>
        {#each orphans as id (id)}
          <div class="pp-row">
            <span class="pp-sev dim"><Unlink size={12} /></span>
            <button
              type="button"
              class="pp-main"
              data-problem-row
              disabled={!onOpenNote}
              title={onOpenNote ? `Open ${noteName(id)}` : 'No editor is attached to this panel yet'}
              onclick={() => onOpenNote?.(id)}
            >
              <span class="pp-text"><b>{noteName(id)}</b></span>
              <span class="pp-where">{noteFolder(id) || 'vault root'}</span>
            </button>
          </div>
        {/each}
      {/if}
    {/if}
  </div>
</div>

<style>
  .pp {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-base);
  }

  .pp-bar {
    flex: none;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .pp-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    outline: none;
  }

  .pp-head {
    padding: 8px 12px 4px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .pp-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
  }
  .pp-row:hover { background: var(--bg-hover); }

  .pp-sev { display: flex; flex: none; }
  /* An unresolved link is a prompt, not damage — amber, never the error red the
     conflicts panel owns. */
  .pp-sev.warn { color: var(--warning); }
  .pp-sev.dim { color: var(--text-disabled); }

  .pp-main {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
    height: 24px;
    padding: 0;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .pp-main:disabled { cursor: default; }
  .pp-main:not(:disabled):hover .pp-text { color: var(--text-primary); }

  .pp-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pp-text b { color: var(--text-primary); font-weight: 600; }
  .pp-dim { color: var(--text-disabled); }

  .pp-where {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-muted);
  }
  .pp-where b { color: var(--text-secondary); font-weight: 400; }

  .pp-act {
    display: flex;
    align-items: center;
    flex: none;
    padding: 3px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .pp-act:hover { background: var(--bg-overlay); color: var(--text-primary); }
</style>
