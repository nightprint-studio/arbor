<script lang="ts">
  /**
   * The preview beside the vault-search results: the selected note's source, with the
   * query highlighted in it.
   *
   * A results list answers "which notes", and the reason to keep reading is
   * "is this the one" — which a 160-character excerpt cannot settle. The preview
   * is what turns walking the list with the arrow keys into deciding, without
   * opening and closing four notes to find the right one.
   *
   * **It shows the source, not a rendering.** Rendering markdown is the editor's
   * job; a second, lesser renderer written here would be a second thing to keep in
   * step with the format. The source with the matches marked is honest and is what
   * a `Ctrl+F` in any editor shows. The frame is the shared `FindPreview` in its
   * plain-text mode.
   *
   * Reads only, and only what is selected — one `garrulus_read_note`, debounced so
   * holding the Down arrow through forty results does not ask for forty notes.
   */
  import { ExternalLink } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FindPreview from '$lib/components/shared/ui/find/FindPreview.svelte';
  import { readNote, type Hit } from '$lib/ipc/garrulus';
  import { noteName } from '../panels/note-path';
  import { findTermRanges, highlightCharSegments } from './highlight';

  interface Props {
    /** The highlighted result, or nothing selected yet. */
    hit?: Hit;
    /** The words of the query that produced the results — what gets marked. */
    terms: string[];
    /** Open the selected note for real. Absent → no open affordance is offered. */
    onOpen?: (id: string) => void;
  }

  let { hit, terms, onOpen }: Props = $props();

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

  const id = $derived(hit?.id ?? null);

  const shown = $derived(source === null ? '' : source.slice(0, PREVIEW_BUDGET));
  const truncated = $derived(source !== null && source.length > PREVIEW_BUDGET);
  const segments = $derived(
    shown ? highlightCharSegments(shown, findTermRanges(shown, terms)) : [],
  );

  const note = $derived.by<string | null>(() => {
    if (!hit) return 'Pick a result to read it here without opening it.';
    if (loading && source === null) return `Reading ${noteName(hit.id)}…`;
    return error;
  });

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
        .then((read) => {
          if (run !== seq) return;
          source = read.text;
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

<FindPreview
  label={hit ? hit.id : ''}
  {note}
  {segments}
  footnote={truncated
    ? `Only the first ${PREVIEW_BUDGET.toLocaleString()} characters are shown here — open the note to read the rest.`
    : null}
>
  {#snippet actions()}
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
  {/snippet}
</FindPreview>
