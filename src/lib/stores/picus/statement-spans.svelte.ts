/**
 * Where each statement begins and ends in the document in front of you — **as the
 * backend's parser sees it**.
 *
 * ## Why this exists
 *
 * The same question was being answered twice, once per side. The Run path asked
 * `picus-parse` (`sqlStatements`), while completion, hover, ghost text, the
 * semantic diagnostics and the bind-slot scanner split the buffer themselves, on
 * top-level `;`. `sql-intel/tokens.ts` was honest about the limit of doing it that
 * way — an Oracle `CREATE PROCEDURE … BEGIN … END;` contains semicolons of its own
 * and came apart into fragments — but honesty about a limit is not the same as not
 * having it, and the two answers could disagree about which statement the caret is
 * in. That is the shape of the bug: completion describes one statement and
 * <kbd>Ctrl</kbd>+<kbd>Enter</kbd> sends another.
 *
 * The rule this restores is the one the rest of the SQL intelligence already
 * follows: **the rule goes where the rule is authoritative, the lookup happens
 * where the data already is.** Statement boundaries are a parse, the parser is in
 * the backend, so they are asked for. Tokens and the catalogue stay on this side,
 * where they are already free.
 *
 * ## What it does not do
 *
 * It answers boundaries only. Tokenizing stays local and synchronous — it has to
 * be, it runs on every keystroke — so `scanSql` still lexes the buffer itself and
 * merely groups the result by these spans instead of by its own `;` rule.
 *
 * And it is allowed not to know. Before the first answer lands, after a failure, or
 * while the text is newer than the reply, {@link StatementSpanStore.for} returns
 * `null` and the caller falls back to the local split. A round trip must never be
 * the difference between completion working and not working.
 */

import { sqlStatements, type StatementSpan } from '$lib/ipc/picus/db';
import type { Dialect } from '$lib/types/picus';

/**
 * How long the buffer must sit still before it is asked about.
 *
 * A little under the parse-fault debounce: both land on the same backend parser,
 * and having the spans arrive first means the diagnostics that follow are already
 * grouped the way the parser groups them.
 */
const DEBOUNCE_MS = 200;

function createStatementSpanStore() {
  /** The text the current spans describe — what their offsets index into. */
  let describedText = $state('');
  /**
   * …and the dialect they were read in. The two engines disagree about what ends a
   * statement, so the same text is a different question on each.
   *
   * A plain `let`: only {@link follow}'s no-op guard reads it, nothing renders it,
   * and making it reactive would put a write into the dependency set of whatever is
   * reading the spans.
   */
  let describedDialect: Dialect | undefined;
  let spans = $state<StatementSpan[] | null>(null);

  /** Guards against an older answer landing after a newer one. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function reread(text: string, dialect: Dialect | undefined, mine: number) {
    try {
      const found = await sqlStatements(text, dialect ?? 'postgres');
      if (mine !== seq) return;
      spans = found;
    } catch {
      // `picus-be` down, or the parse refused. Not an answer, and not a reason to
      // draw one: the caller falls back to the local split, which is what it used
      // to do all the time.
      if (mine !== seq) return;
      spans = null;
    }
    // Recorded either way, so a buffer nobody is typing in is asked about once
    // rather than on every read.
    describedText = text;
    describedDialect = dialect;
  }

  return {
    /**
     * The spans for this exact text, or `null` when there are none to be had yet.
     *
     * `null` is a real answer and the caller must handle it: stale boundaries are
     * worse than local ones, because every offset after an edit points at text that
     * has moved.
     *
     * Keyed on the **text alone**, deliberately, while {@link follow} is keyed on the
     * dialect as well. One document has one driver (`DocumentBridge`) and therefore
     * one dialect in play at a time, so there is nothing here to disambiguate — but
     * the callers do not all name that dialect the same way. `sqlDiagnostics` falls
     * back to `oracle` for an unclassified file while the language descriptor falls
     * back to `postgres`, so a key that included it would hand the backend's answer
     * to one of them and the local split to the other: the two opinions this exists
     * to collapse, reintroduced by a cache key. Asking again on a dialect *change* is
     * `follow`'s job, and it does it.
     */
    for(text: string): StatementSpan[] | null {
      if (text !== describedText) return null;
      return spans;
    },

    /** The buffer changed. Debounced; a no-op when this text is already described. */
    follow(text: string, dialect: Dialect | undefined) {
      if (text === describedText && dialect === describedDialect) return;
      if (timer) clearTimeout(timer);
      const mine = ++seq;
      timer = setTimeout(() => void reread(text, dialect, mine), DEBOUNCE_MS);
    },

    /** The document went away. */
    clear() {
      if (timer) clearTimeout(timer);
      seq++;
      spans = null;
      describedText = '';
      describedDialect = undefined;
    },
  };
}

export const statementSpanStore = createStatementSpanStore();
