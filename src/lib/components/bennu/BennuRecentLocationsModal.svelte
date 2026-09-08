<script lang="ts">
  /**
   * BennuRecentLocationsModal — IntelliJ's "Recent Locations" popup (Ctrl+Shift+E): the places
   * you have been, most recent first, with the line you were on.
   *
   * The reason it exists is that Back is a *stepper*: getting somewhere five jumps ago means
   * pressing it five times and reading five intermediate screens. This is the same history as a
   * list — you recognise the line and go straight there.
   *
   * Two kinds of row, because they answer different questions: places you **visited** (a jump
   * landed there) and places you **edited**. Both come from the same store, and the toggle keeps
   * only the edits — which is how you find the file you were changing before lunch.
   *
   * Type to filter (the shared fuzzy matcher, over the file name and the line's own text),
   * ↑/↓ to move, Enter to go, Esc to close.
   */
  import { History, Pencil } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { fuzzyMatch, segments, type MatchRange } from '$lib/utils/fuzzy';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuNavStore, type NavPlace, type RecentPlace } from '$lib/stores/bennu/nav-history.svelte';

  let {
    onClose,
    onPick,
  }: { onClose: () => void; onPick: (place: NavPlace) => void } = $props();

  let query = $state('');
  let active = $state(0);
  let editsOnly = $state(false);

  function fileName(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }

  /** The text of the line the place points at — only for a file whose buffer we already hold.
   *  A place in a file that is not open shows its name alone rather than making us read the disk
   *  to draw a popup. */
  function lineText(place: NavPlace): string {
    const source = projectStore.sourceOf(place.file);
    if (!source) return '';
    const line = source.split('\n')[place.line - 1];
    return line ? line.trim().slice(0, 120) : '';
  }

  interface Row {
    place: RecentPlace;
    text: string;
    ranges: MatchRange[];
  }

  const all = $derived.by<Row[]>(() =>
    bennuNavStore.recent
      .filter((p) => !editsOnly || p.kind === 'edit')
      .map((place) => ({ place, text: lineText(place), ranges: [] })),
  );

  /** Filtered rows. The query is matched against the file name first — that is what people type —
   *  and against the line's text as a fallback, so a remembered phrase finds the place too. */
  const rows = $derived.by<Row[]>(() => {
    const q = query.trim();
    if (!q) return all;
    const scored: { row: Row; score: number; at: number }[] = [];
    all.forEach((row, at) => {
      const name = fuzzyMatch(fileName(row.place.file), q);
      if (name) {
        scored.push({ row: { ...row, ranges: name.ranges }, score: name.score, at });
        return;
      }
      const text = row.text ? fuzzyMatch(row.text, q) : null;
      if (text) scored.push({ row, score: text.score - 1000, at });
    });
    // Ties keep recency, which is the order this list is for.
    scored.sort((a, b) => b.score - a.score || a.at - b.at);
    return scored.map((s) => s.row);
  });

  $effect(() => { void rows; active = 0; });

  function pick(row: Row | undefined) {
    if (!row) return;
    onClose();
    onPick({ file: row.place.file, line: row.place.line, col: row.place.col });
  }

  function onKeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); if (n) active = (active + 1) % n; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (n) active = (active - 1 + n) % n; }
    else if (e.key === 'Enter') { e.preventDefault(); pick(rows[active]); }
  }
</script>

<Modal {onClose} width="640px" height="520px" padBody={false} ariaLabel="Recent locations">
  {#snippet header()}
    <ModalHeader {onClose}>
      <History size={14} />
      <span class="modal-title">Recent locations</span>
      <span class="hdr-count">{rows.length}</span>
    </ModalHeader>
  {/snippet}

  {#snippet footer()}
    <div class="rl-foot">
      <span><Kbd keys={['↑', '↓']} size="sm" /> move</span>
      <span><Kbd keys={['Enter']} size="sm" /> go to</span>
      <span><Kbd keys={['Esc']} size="sm" /> close</span>
    </div>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rl" onkeydown={onKeydown}>
    <div class="search">
      <Input bind:value={query} placeholder="Filter…" autofocus ariaLabel="Filter recent locations" />
      <div class="edits">
        <Toggle bind:checked={editsOnly} size="sm" label="Edited only" />
      </div>
    </div>

    {#if all.length === 0}
      <EmptyState message={editsOnly ? 'Nothing edited yet.' : 'Nowhere yet — navigate, and this fills up.'} />
    {:else if rows.length === 0}
      <div class="state">No matches.</div>
    {:else}
      <div class="list" role="listbox" tabindex="-1" aria-label="Recent locations">
        {#each rows as row, i (row.place.file + ':' + row.place.at)}
          <button
            class="row"
            class:active={i === active}
            type="button"
            role="option"
            aria-selected={i === active}
            onmousemove={() => (active = i)}
            onclick={() => pick(row)}
          >
            <span class="r-icon" class:edit={row.place.kind === 'edit'}>
              {#if row.place.kind === 'edit'}<Pencil size={12} />{:else}<History size={12} />{/if}
            </span>
            <span class="r-main">
              <span class="r-file">
                {#each segments(fileName(row.place.file), row.ranges) as seg, si (si)}<span
                  class:hit={seg.hit}>{seg.text}</span>{/each}
                <span class="r-line">:{row.place.line}</span>
              </span>
              {#if row.text}<span class="r-text">{row.text}</span>{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</Modal>

<style>
  .rl { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .search {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; border-bottom: 1px solid var(--border);
  }
  .search :global(.input-wrap) { flex: 1; }
  .edits { font-size: 11px; color: var(--text-secondary); white-space: nowrap; }
  .list { flex: 1; overflow-y: auto; min-height: 0; padding: 4px 0; }
  .state { padding: 16px; text-align: center; color: var(--text-secondary); font-size: 12px; }
  .row {
    display: flex; align-items: flex-start; gap: 8px; width: 100%;
    padding: 5px 12px; border: 0; background: none; text-align: left; cursor: pointer;
    color: var(--text-primary); font-size: 12px;
  }
  .row.active { background: var(--bg-hover); }
  .r-icon { display: flex; padding-top: 2px; color: var(--text-disabled); }
  .r-icon.edit { color: var(--warning); }
  .r-main { display: flex; flex-direction: column; gap: 1px; min-width: 0; flex: 1; }
  .r-file { font-weight: 500; }
  .r-file .hit { color: var(--accent); font-weight: 600; }
  .r-line { color: var(--text-disabled); font-weight: 400; }
  .r-text {
    font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .rl-foot { display: flex; gap: 14px; font-size: 11px; color: var(--text-secondary); }
</style>
