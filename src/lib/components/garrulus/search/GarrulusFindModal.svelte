<script lang="ts">
  /**
   * Find-in-vault (Ctrl+Shift+F) — the same shell as Bennu's "Find in project", fed with notes.
   *
   * The presentation and the keyboard are the shared `FindResults`; this modal only maps the
   * search store's results onto it. `docs/garrulus-design.md` §12.14 still decides what that
   * mapping may not do:
   *
   *  • **it does not run on every keystroke.** The query is a sentence with structure in it;
   *    searching half of one is how a box ends up showing the results of `type:b`. Enter
   *    searches (the query box owns that key), and once results are on screen Enter opens the
   *    highlighted note instead — the summary says when the box has moved on from what is under it;
   *  • **the filters are in the query, not beside it.** `type:bug stato:aperto` is typed where
   *    the words are typed and rendered as chips there (`SearchQueryBox`, passed as the
   *    widget's `queryField`);
   *  • **the result is a note, not a line.** Hits are grouped under the note that holds them
   *    (one excerpt each — the index cuts one per note), and the preview answers "is this the
   *    one" without opening anything.
   *
   * Everything is a read: `garrulus_search` behind Enter, `garrulus_read_note` behind the
   * selection. The Tags and Types sections filter the same store and bring this modal forward.
   */
  import FindResults from '$lib/components/shared/ui/find/FindResults.svelte';
  import type {
    FindGroup,
    FindHitItem,
    FindNotice,
    FindSegment,
  } from '$lib/components/shared/ui/find/find-types';
  import FindFooter from '$lib/components/shared/internal/FindFooter.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { noteFolder, noteName } from '../panels/note-path';
  import { highlightSegments, queryTerms } from './highlight';
  import { parseQuery } from './query-tokens';
  import SearchQueryBox from './SearchQueryBox.svelte';
  import GarrulusFindPreview from './GarrulusFindPreview.svelte';

  interface Props {
    onClose: () => void;
    /** Open a note in the editor (a `Hit.id`). */
    onOpenNote: (id: string) => void;
  }

  let { onClose, onOpenNote }: Props = $props();

  /** The highlighted result — an index into the hits, one hit row per note. */
  let sel = $state(0);

  const hits = $derived(garrulusSearchStore.hits);
  const current = $derived(hits[sel]);

  // A new result set starts at its first note.
  $effect(() => {
    void garrulusSearchStore.hits;
    sel = 0;
  });

  /** What a row shows when the note matched on its title or a filter and has no excerpt. */
  const NO_EXCERPT = 'matched by its title or filters';

  /**
   * One line per row. Newlines and tabs become a single space each — one-for-one, because the
   * match ranges are UTF-8 byte offsets into this text and a collapse would shift them.
   */
  function oneLine(text: string): string {
    return text.replace(/[\r\n\t]/g, ' ');
  }

  const groups = $derived<FindGroup[]>(
    hits.map((hit) => ({
      key: hit.id,
      label: hit.title || noteName(hit.id),
      detail: noteFolder(hit.id),
      tooltip: hit.id,
      hits: [{ key: hit.id, text: hit.snippet ? oneLine(hit.snippet.text) : NO_EXCERPT }],
    })),
  );

  const byId = $derived(new Map(hits.map((h) => [h.id, h])));

  function segmentsOf(item: FindHitItem): FindSegment[] {
    const ranges = byId.get(item.key)?.snippet?.ranges;
    return ranges ? highlightSegments(item.text, ranges) : [{ text: item.text, hit: false }];
  }

  const terms = $derived(queryTerms(parseQuery(garrulusSearchStore.ranQuery ?? '').text));

  const notice = $derived<FindNotice | null>(
    !garrulusVaultStore.isOpen
      ? { message: 'Open a vault to search it.' }
      : garrulusSearchStore.error
        ? { message: garrulusSearchStore.error }
        : !garrulusSearchStore.hasRun
          ? {
              message: 'Search the vault.',
              description:
                'Words find the notes that contain them. type:bug, stato:aperto, #urgente and ' +
                'sort:-title narrow and order them, and mix freely with the words. Enter searches.',
            }
          : null,
  );

  const empty: FindNotice = {
    message: 'No note matches that.',
    description: 'A filter on a field no note carries excludes every note.',
  };

  const summary = $derived.by(() => {
    if (!garrulusSearchStore.hasRun || garrulusSearchStore.error) return '';
    const notes = hits.length;
    const marks = garrulusSearchStore.highlighted;
    const ms = garrulusSearchStore.elapsedMs;
    return (
      `${notes} ${notes === 1 ? 'note' : 'notes'}` +
      (marks > 0 ? ` · ${marks} highlighted` : '') +
      (ms === null ? '' : ` · ${ms} ms`)
    );
  });

  /** The box has moved on from the results under it. */
  const showStale = $derived(garrulusSearchStore.stale && !garrulusSearchStore.running);

  function open(index: number) {
    const hit = hits[index];
    if (!hit) return;
    onOpenNote(hit.id);
    onClose();
  }
</script>

<Modal {onClose} width="1000px" height="620px" padBody={false} ariaLabel="Search the vault">
  <FindResults
    {groups}
    bind:selected={sel}
    loading={garrulusSearchStore.running}
    {notice}
    {empty}
    {summary}
    {segmentsOf}
    onOpen={open}
    controls={showStale ? controls : undefined}
    {queryField}
    {preview}
    {footer}
  />
</Modal>

{#snippet controls()}
  {#if showStale}
    <span class="gf-stale">the query has changed — press Enter to run it</span>
  {/if}
{/snippet}

{#snippet queryField()}
  <SearchQueryBox autofocus />
{/snippet}

{#snippet preview()}
  <GarrulusFindPreview hit={current} {terms} onOpen={(id) => open(hits.findIndex((h) => h.id === id))} />
{/snippet}

{#snippet footer()}
  <FindFooter />
{/snippet}

<style>
  /* Not an error: the box simply says something the results do not answer yet. */
  .gf-stale { color: var(--warning); }
</style>
