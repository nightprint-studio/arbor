/**
 * What the grammar could not read in the document in front of you.
 *
 * ## Why this is not part of the live diagnostics
 *
 * `sql-intel/diagnostics.ts` answers *semantic* questions — is this table in the
 * catalogue, does this alias have that column, is this a write on a read-only
 * connection — and it answers them synchronously, from text and a schema this side
 * already has. "Is this SQL at all" is not one of those. Only the grammar can say
 * it, the grammar is in the backend, and asking it is a round trip.
 *
 * So it is a store rather than a function: it follows the buffer, debounced, and
 * the editor merges its answer with the synchronous one. The two stay separate
 * because they fail separately — the semantic rules are deliberately quiet when
 * they do not know, and a parse error is never a matter of not knowing.
 *
 * ## The gap this closes
 *
 * The syntax-tree panel has always shown these: it marks the node red and writes
 * `invented` on a token the parser had to supply. The editor beside it showed
 * nothing, so a statement the parser had already rejected — procedural code at the
 * top level, an unclosed paren — looked fine until it was run. One of the two
 * panels was right and it was the one nobody had open.
 */

import { parseFaults as fetchFaults, type ParseFault } from '$lib/ipc/picus/ast';
import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor/types';
import type { Dialect } from '$lib/types/picus';

/** How long the buffer must sit still before it is parsed. */
const DEBOUNCE_MS = 260;

function createParseFaultStore() {
  /** The text the current faults describe — what their offsets index into. */
  let described = $state('');
  let faults = $state<ParseFault[]>([]);

  /** Guards against an older parse landing after a newer one. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function reparse(text: string, dialect: Dialect | undefined, mine: number) {
    try {
      const found = await fetchFaults(text, dialect);
      if (mine !== seq) return;
      faults = found;
      described = text;
    } catch {
      // A parse that could not be asked for is not a parse error, and drawing one
      // would be the worst possible answer: a squiggle under correct SQL because
      // the backend was busy.
      if (mine !== seq) return;
      faults = [];
      described = text;
    }
  }

  return {
    /**
     * The faults as the editor wants them — **only while they still describe the
     * buffer**.
     *
     * A stale offset is worse than no offset: it underlines a character that has
     * moved, and the message beside it is about something that is no longer there.
     * So the moment the text differs from what was parsed, this is empty and stays
     * empty until the next answer lands, a fifth of a second later.
     */
    for(text: string): EditorDiagnostic[] {
      if (text !== described) return [];
      return faults.map((fault) => ({
        from: fault.start,
        to: fault.end,
        severity: 'error' as const,
        message: fault.message,
      }));
    },

    /** The buffer changed. Debounced; a no-op when the text is already described. */
    follow(text: string, dialect: Dialect | undefined) {
      if (text === described) return;
      if (timer) clearTimeout(timer);
      const mine = ++seq;
      timer = setTimeout(() => void reparse(text, dialect, mine), DEBOUNCE_MS);
    },

    /** The document went away. */
    clear() {
      if (timer) clearTimeout(timer);
      seq++;
      faults = [];
      described = '';
    },
  };
}

export const parseFaultStore = createParseFaultStore();
