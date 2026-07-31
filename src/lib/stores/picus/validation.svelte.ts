/**
 * Whether the database accepts the statements in the document in front of you.
 *
 * ## Why this replaced a pile of frontend heuristics
 *
 * The editor used to answer "no such table / no such column / ambiguous column"
 * itself, from a cached catalogue, in `sql-intel/diagnostics.ts`. That code had to
 * stay silent whenever it was unsure and was wrong whenever the schema had moved —
 * the two failure modes of reimplementing something the server already does exactly.
 * So the semantic checks are gone, and this asks the server: each statement is
 * *prepared* (parsed and described) but never run, and whatever it rejects comes back
 * placed at the server's own position.
 *
 * ## The same shape as `parse-faults`, and for the same reasons
 *
 * It is a round trip, so it follows the buffer debounced rather than answering
 * synchronously; and a call that could not be made is **never** a squiggle — a red
 * buffer because the backend was busy would be the worst possible answer. It parts
 * from `parse-faults` in one way: it carries a status, because the toolbar shows
 * whether the last check passed, is running, or could not be asked for at all.
 *
 * A singleton, following the active editor: only one buffer is on screen, and the
 * `for(text)` guard keeps a stale answer off any other.
 */

import { validateSql, type ValidationFinding } from '$lib/ipc/picus/db';
import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor/types';

/** How long the buffer must sit still before it is validated. Longer than the parse
 *  debounce: this is a round trip per statement, not a local scan. */
const DEBOUNCE_MS = 500;

/** What the toolbar shows. */
export type ValidationStatus = 'idle' | 'checking' | 'ok' | 'errors' | 'unavailable';

function createValidationStore() {
  /** The text the current findings describe — what their offsets index into. */
  let described = $state('');
  /** The connection the current findings were checked against — the same text on a
   *  different database is a different question. */
  let describedConn = '';
  let findings = $state<ValidationFinding[]>([]);
  let status = $state<ValidationStatus>('idle');

  /** Guards against an older check landing after a newer one. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function run(text: string, connectionId: string, mine: number) {
    try {
      const found = await validateSql(connectionId, text);
      if (mine !== seq) return;
      findings = found;
      described = text;
      describedConn = connectionId;
      status = found.length ? 'errors' : 'ok';
    } catch {
      // The connection failed mid-check. Not a validation result — clear the
      // squiggles and say so, rather than leaving a stale green tick or drawing a
      // red one under SQL that may be perfectly fine.
      if (mine !== seq) return;
      findings = [];
      status = 'unavailable';
    }
  }

  /** Stop any pending check and forget the current answer, without validating — so
   *  the next capable call re-checks rather than trusting a stale `described`. */
  function settle(next: ValidationStatus) {
    if (timer) clearTimeout(timer);
    seq++;
    findings = [];
    described = '';
    describedConn = '';
    status = next;
  }

  return {
    get status() {
      return status;
    },

    /** How many findings the last check produced — for the toolbar's badge. */
    get count() {
      return findings.length;
    },

    /**
     * The findings as the editor wants them — **only while they still describe the
     * buffer**. A stale offset underlines a character that has moved; the moment the
     * text differs from what was checked, this is empty.
     *
     * Warnings, not errors: a syntax fault ("this is not SQL") is the parser's job
     * and draws in red; this is the server's semantic verdict, a step milder.
     */
    for(text: string): EditorDiagnostic[] {
      if (text !== described) return [];
      return findings.map((f) => ({
        from: f.start,
        to: f.end,
        severity: 'warning' as const,
        message: f.message,
      }));
    },

    /**
     * The buffer (or its connection) changed. Debounced; a no-op when the same text
     * has already been validated.
     */
    follow(text: string, connectionId: string | undefined, capable: boolean) {
      // Nothing to validate against: no open connection, or an engine that cannot
      // prepare. Deliberately does NOT record `described`, so the same text is
      // checked the moment a capable connection appears.
      if (!capable || !connectionId) {
        settle('unavailable');
        return;
      }
      if (!text.trim()) {
        described = '';
        settle('idle');
        return;
      }
      // Already validated, against this same connection: nothing to do.
      if (text === described && connectionId === describedConn) return;
      if (timer) clearTimeout(timer);
      const mine = ++seq;
      status = 'checking';
      timer = setTimeout(() => void run(text, connectionId, mine), DEBOUNCE_MS);
    },

    /** The document went away. */
    clear() {
      described = '';
      settle('idle');
    },
  };
}

export const validationStore = createValidationStore();
