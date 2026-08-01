<script lang="ts">
  /**
   * Find-in-vault, as a view rather than as a dialog.
   *
   * `docs/garrulus-design.md` §12.14 settles two things about this surface: it is
   * first class, and Picus's find-in-file is the anti-pattern rather than the
   * model. What that rules out, concretely:
   *
   *  • **it does not run on every keystroke.** The query is a sentence with
   *    structure in it; searching half of one is how a box ends up showing the
   *    results of `type:b`. Enter searches, and the summary line says when the
   *    box has moved on from what is under it;
   *  • **the filters are in the query, not beside it.** `type:bug stato:aperto`
   *    is typed where the words are typed and rendered as chips there — one
   *    sentence, one field (see `SearchQueryBox`);
   *  • **the result is a note, not a line.** Matches are grouped under the note
   *    that holds them, with a sticky header, and the preview answers "is this
   *    the one" without opening anything.
   *
   * Everything is a read. The view never writes and never syncs; the only backend
   * calls it makes are `garrulus_search` behind Enter, and `garrulus_read_note`
   * behind the selection.
   *
   * Mount it in the centre column. It carries no chrome of its own beyond its own
   * header row, so a host can put it where the editor goes.
   */
  import { Search } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import SearchQueryBox from './SearchQueryBox.svelte';
  import SearchHitGroup from './SearchHitGroup.svelte';
  import SearchPreview from './SearchPreview.svelte';

  interface Props {
    /** Open a note in the editor. Absent while no editor is mounted — the rows
     *  then select and preview, and offer no verb that goes nowhere. */
    onOpenNote?: (id: string) => void;
    /** Focus the query box when the view appears. */
    autofocus?: boolean;
  }

  let { onOpenNote, autofocus = true }: Props = $props();

  let box = $state<{ focus: () => void } | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  /** Which notes are folded. Session-shaped and view-local: it describes what is
   *  on screen right now and means nothing once the query changes. */
  let collapsed = $state<Record<string, boolean>>({});

  /**
   * Results belong to the vault they were found in.
   *
   * Closing a vault or switching to another one leaves the old hits addressing
   * notes that are no longer there, which is the one state where every row lies.
   * `untrack`-free because only the plain variable is compared: the effect
   * depends on the vault root and on nothing it writes.
   */
  let lastRoot: string | null = null;
  $effect(() => {
    const root = garrulusVaultStore.root;
    if (root === lastRoot) return;
    lastRoot = root;
    garrulusSearchStore.clear();
    collapsed = {};
  });

  const summary = $derived.by(() => {
    if (!garrulusSearchStore.hasRun) return null;
    const notes = garrulusSearchStore.hits.length;
    const ms = garrulusSearchStore.elapsedMs;
    const noun = notes === 1 ? 'note' : 'notes';
    const marks = garrulusSearchStore.highlighted;
    const highlights = marks > 0 ? ` · ${marks} highlighted` : '';
    return `${notes} ${noun}${highlights}${ms === null ? '' : ` · ${ms} ms`}`;
  });

  /** Put the keyboard on a result and keep it in view. */
  function focusRow(id: string | null) {
    if (!id) return;
    const group = listEl?.querySelector<HTMLElement>(`[data-hit-group="${CSS.escape(id)}"]`);
    group?.querySelector<HTMLButtonElement>('[data-hit-row]')?.focus();
    group?.scrollIntoView({ block: 'nearest' });
  }

  function step(delta: number) {
    focusRow(garrulusSearchStore.step(delta));
  }

  /**
   * The list's keyboard: ↑/↓ walk the notes, Enter opens the one selected, Esc
   * goes back to the box.
   *
   * ←/→ fold and unfold, and are handled by the group itself so they act on the
   * row the focus is actually on.
   */
  function onListKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      step(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      step(-1);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      box?.focus();
    }
  }
</script>

<div class="sv">
  <div class="sv-top">
    <SearchQueryBox
      bind:this={box}
      {autofocus}
      onLeaveDown={() => step(0)}
    />

    <div class="sv-meta">
      <Button
        variant="secondary"
        size="xs"
        disabled={!garrulusSearchStore.query.trim() || garrulusSearchStore.running}
        tooltip={{ content: 'Search the vault', shortcut: 'Enter' }}
        onclick={() => void garrulusSearchStore.run()}
      >
        {#snippet iconStart()}<Search size={12} />{/snippet}
        Search
      </Button>

      {#if garrulusSearchStore.running}
        <span class="sv-busy"><Spinner size={12} /> Searching…</span>
      {:else if summary}
        <span class="sv-summary">{summary}</span>
      {/if}

      {#if garrulusSearchStore.stale && !garrulusSearchStore.running}
        <span class="sv-stale">the query has changed — press Enter to run it</span>
      {/if}

      <span class="sv-grow"></span>
      <span class="sv-hint">
        <Kbd keys={['↑']} size="sm" /><Kbd keys={['↓']} size="sm" /> results
        {#if onOpenNote}· <Kbd keys={['Enter']} size="sm" /> open{/if}
      </span>
    </div>
  </div>

  <div class="sv-body">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="sv-hits"
      bind:this={listEl}
      role="group"
      tabindex="-1"
      aria-label="Search results"
      onkeydown={onListKeyDown}
    >
      {#if garrulusSearchStore.error}
        <StateBlock tone="error" label={garrulusSearchStore.error} />
      {:else if !garrulusSearchStore.hasRun}
        <StateBlock tone="neutral">
          <span class="sv-blurb">
            Words find the notes that contain them. <code>type:bug</code>,
            <code>stato:aperto</code>, <code>#urgente</code> and
            <code>sort:-title</code> narrow and order them, and mix freely with the
            words. Anything that is not a filter is simply searched for.
          </span>
        </StateBlock>
      {:else if garrulusSearchStore.hits.length === 0}
        <StateBlock
          tone="info"
          label="No note matches that. A filter on a field no note carries excludes every note."
        />
      {:else}
        {#each garrulusSearchStore.hits as hit (hit.id)}
          <SearchHitGroup
            {hit}
            active={garrulusSearchStore.selected === hit.id}
            collapsed={collapsed[hit.id] ?? false}
            onToggle={() => (collapsed = { ...collapsed, [hit.id]: !collapsed[hit.id] })}
            onSelect={() => garrulusSearchStore.select(hit.id)}
            onOpen={onOpenNote ? () => onOpenNote(hit.id) : undefined}
          />
        {/each}
      {/if}
    </div>

    <div class="sv-preview">
      <SearchPreview onOpen={onOpenNote} />
    </div>
  </div>
</div>

<style>
  .sv {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
    background: var(--bg-base);
  }

  .sv-top {
    flex: none;
    padding: 10px 12px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .sv-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 8px;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .sv-grow { flex: 1; }
  .sv-summary { color: var(--text-secondary); }
  .sv-busy { display: inline-flex; align-items: center; gap: 5px; }
  /* Not an error: the box simply says something the results do not answer yet. */
  .sv-stale { color: var(--warning); }
  .sv-hint { display: inline-flex; align-items: center; gap: 3px; }

  .sv-body {
    flex: 1;
    min-height: 0;
    display: flex;
    min-width: 0;
  }

  .sv-hits {
    flex: 1;
    min-width: 0;
    overflow: auto;
    border-right: 1px solid var(--border-subtle);
    outline: none;
  }

  .sv-preview {
    width: 42%;
    min-width: 260px;
    max-width: 620px;
    min-height: 0;
    display: flex;
  }
  .sv-preview > :global(*) { flex: 1; min-width: 0; }

  /* Below this the two columns stop being two columns; the preview goes under
     the results rather than squeezing both into nothing. */
  @media (max-width: 900px) {
    .sv-body { flex-direction: column; }
    .sv-hits { border-right: none; border-bottom: 1px solid var(--border-subtle); }
    .sv-preview { width: auto; max-width: none; flex: 1; }
  }

  .sv-blurb {
    max-width: 62ch;
    line-height: 1.6;
    text-align: center;
  }
  .sv-blurb code {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--accent);
  }
</style>
