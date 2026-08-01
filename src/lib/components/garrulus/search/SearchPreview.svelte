<script lang="ts">
  /**
   * The preview beside the results: the selected note's source, with the query
   * highlighted in it.
   *
   * A results list answers "which notes", and the reason to keep reading is
   * "is this the one" — which a 160-character excerpt cannot settle. The preview
   * is what turns walking the list with the arrow keys into deciding, without
   * opening and closing four notes to find the right one.
   *
   * **It shows the source, not a rendering.** Rendering markdown is the editor's
   * job and the editor is being made shared and product-agnostic
   * (`docs/garrulus-design.md` §12.8); a second, lesser renderer written here to
   * fill the gap would be a second thing to keep in step with the format. The
   * source with the matches marked is honest, is what a `Ctrl+F` in any editor
   * shows, and costs nothing to replace when the real one lands.
   *
   * Reads only, and only what is selected — one `garrulus_read_note`, debounced so
   * holding the Down arrow through forty results does not ask for forty notes.
   */
  import { ExternalLink, FileText } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { readNote } from '$lib/ipc/garrulus';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { noteFolder, noteName } from '../panels/note-path';
  import { findTermRanges, highlightCharSegments, queryTerms } from './highlight';
  import { parseQuery } from './query-tokens';

  interface Props {
    /** Open the selected note for real. Absent → no open affordance is offered. */
    onOpen?: (id: string) => void;
  }

  let { onOpen }: Props = $props();

  /**
   * How much of a note the preview will render.
   *
   * A preview is a glance; a 400 KB note pasted into the DOM to answer "is this
   * the one" would cost a visible pause for information nobody reads. Past this
   * the pane says it was cut.
   */
  const PREVIEW_BUDGET = 40_000;

  /** How long a selection has to hold still before its note is read. Long enough
   *  that arrowing through the list is one read at the end, short enough that a
   *  deliberate click feels immediate. */
  const READ_DELAY_MS = 90;

  let source = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /**
   * The id `source` belongs to — the guard that keeps re-selecting the same
   * result from re-reading it.
   *
   * A plain variable rather than `$state`: nothing renders it, and a piece of
   * state an effect both reads and writes is a loop waiting for the first
   * condition that stops converging.
   */
  let loadedFor: string | null = null;

  const hit = $derived(garrulusSearchStore.selectedHit);
  const id = $derived(hit?.id ?? null);

  const terms = $derived(
    queryTerms(parseQuery(garrulusSearchStore.ranQuery ?? '').text),
  );

  const shown = $derived(source === null ? '' : source.slice(0, PREVIEW_BUDGET));
  const truncated = $derived(source !== null && source.length > PREVIEW_BUDGET);
  const parts = $derived(
    shown ? highlightCharSegments(shown, findTermRanges(shown, terms)) : [],
  );

  let seq = 0;

  $effect(() => {
    const target = id;
    if (!target) {
      source = null;
      error = null;
      loading = false;
      loadedFor = null;
      return;
    }
    if (target === loadedFor) return;

    const run = ++seq;
    loading = true;
    const timer = setTimeout(() => {
      readNote(target)
        .then((note) => {
          if (run !== seq) return;
          source = note.text;
          loadedFor = target;
          error = null;
        })
        .catch((e) => {
          if (run !== seq) return;
          // A note whose id is a frontmatter `uid` rather than a path cannot be
          // read by that id, and neither can one deleted since the search ran.
          // Both are worth saying out loud: the result is real, the file is not
          // where the id says.
          source = null;
          loadedFor = target;
          error = String(e);
        })
        .finally(() => {
          if (run === seq) loading = false;
        });
    }, READ_DELAY_MS);

    return () => {
      clearTimeout(timer);
      seq++;
    };
  });
</script>

<div class="pv">
  <div class="pv-head">
    <span class="pv-icon"><FileText size={13} /></span>
    <span class="pv-title">{hit ? hit.title || noteName(hit.id) : 'Preview'}</span>
    {#if hit && noteFolder(hit.id)}<span class="pv-folder">{noteFolder(hit.id)}</span>{/if}
    <span class="pv-grow"></span>
    {#if hit && onOpen}
      <Button
        variant="icon"
        size="xs"
        ariaLabel="Open this note"
        tooltip={{ content: 'Open this note', shortcut: 'Enter' }}
        onclick={() => onOpen(hit.id)}
      >
        {#snippet iconStart()}<ExternalLink size={13} />{/snippet}
      </Button>
    {/if}
  </div>

  <div class="pv-body">
    {#if !hit}
      <StateBlock
        tone="neutral"
        label="Pick a result to read it here without opening it."
      />
    {:else if loading && source === null}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading {noteName(hit.id)}…</span>
      </StateBlock>
    {:else if error}
      <StateBlock tone="error" label={error} />
    {:else}
      <pre class="pv-text">{#each parts as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</pre>
      {#if truncated}
        <p class="pv-cut">Only the first {PREVIEW_BUDGET.toLocaleString()} characters are
          shown here — open the note to read the rest.</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .pv {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    height: 100%;
  }

  .pv-head {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    flex: none;
    padding: 0 6px 0 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .pv-icon { display: flex; color: var(--text-muted); flex-shrink: 0; }
  .pv-title {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pv-folder {
    font-size: var(--font-size-2xs);
    font-family: var(--font-code);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .pv-grow { flex: 1; }

  .pv-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .pv-text {
    margin: 0;
    padding: 10px 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.65;
    color: var(--text-secondary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .pv-cut {
    margin: 0;
    padding: 0 12px 12px;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }

  mark {
    padding: 0 1px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--warning) 32%, transparent);
    color: var(--text-primary);
  }
</style>
