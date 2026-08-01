/**
 * The vault search: the query being built, and the results of the last one run.
 *
 * A store rather than component state because the query has four ways in — the
 * search view's own box, `Ctrl+Shift+F`, the palette, and a row clicked in the
 * Tags or Types sidebar section — and a query owned by the view would be a query
 * the sidebar cannot reach. It is the same argument the ui store makes for the
 * overlays: more than one door, so the state cannot live behind one of them.
 *
 * **Nothing here runs on its own.** `run()` is called from Enter, from a button,
 * or from a sidebar row that was clicked; there is no debounce and no `$effect`
 * anywhere in this file. Search is a read and could not damage a vault, but a
 * query is also *the user's sentence*, and firing it half-typed is how a search
 * box ends up showing the results of `type:b`. `docs/garrulus-design.md` §12.14
 * is explicit that the find-in-file that re-runs on every keystroke is the
 * anti-pattern this view exists to not be.
 *
 * **The query crosses the seam as one string.** The chips are a rendering of it,
 * not a parse of it: `garrulus-index` owns the grammar and is asked the question
 * exactly as it was typed (see `components/garrulus/search/query-tokens.ts`).
 */

import {
  search as ipcSearch,
  type Hit,
} from '$lib/ipc/garrulus';
import {
  buildQuery,
  parseQuery,
  sameToken,
  type QueryToken,
} from '$lib/components/garrulus/search/query-tokens';

function createGarrulusSearchStore() {
  /** The structured filters, as chips. */
  let tokens = $state<QueryToken[]>([]);
  /** The free-text tail, which is what the input actually holds. */
  let text = $state('');

  let hits = $state<Hit[]>([]);
  let running = $state(false);
  let error = $state<string | null>(null);
  /** How long the last search took, round trip included — the number the summary
   *  line shows, and the only honest way to say a vault got slow. */
  let elapsedMs = $state<number | null>(null);
  /** The query the results on screen belong to. `null` means none has been run,
   *  which is a different state from "ran and found nothing". */
  let ranQuery = $state<string | null>(null);
  /** The note whose preview is showing — a `Hit.id`. */
  let selected = $state<string | null>(null);

  /** Discriminates the in-flight search, so a slow one that was superseded can
   *  never overwrite the newer one's results. */
  let seq = 0;

  const query = $derived(buildQuery(tokens, text));

  /** The query has moved on from the results on screen. */
  const stale = $derived(ranQuery !== null && ranQuery !== query.trim());

  const hasRun = $derived(ranQuery !== null);

  /**
   * How many terms are highlighted across the excerpts.
   *
   * Deliberately not called "occurrences": the backend cuts one excerpt per note
   * around the first match and highlights each distinct term once inside it, so
   * this counts highlighted runs on screen and nothing more. The summary line
   * says so in as many words rather than implying a vault-wide total the index
   * never reported.
   */
  const highlighted = $derived(
    hits.reduce((n, h) => n + (h.snippet?.ranges.length ?? 0), 0),
  );

  const selectedHit = $derived(hits.find((h) => h.id === selected) ?? null);

  function clearResults(): void {
    seq++;
    hits = [];
    error = null;
    elapsedMs = null;
    ranQuery = null;
    selected = null;
    running = false;
  }

  /**
   * Run the query as it stands. Called from Enter and from clicks, never from an
   * effect.
   *
   * An empty query is not sent: the backend answers it with an empty list by
   * contract, and asking anyway would replace "type something" with "nothing
   * found", which are not the same sentence.
   *
   * A named function rather than a method so the two callers below reach it
   * without `this` — a store whose methods break when destructured is a trap.
   */
  async function run(): Promise<void> {
    const q = query.trim();
    if (!q) {
      clearResults();
      return;
    }

    const id = ++seq;
    running = true;
    error = null;
    const started = performance.now();

    try {
      const result = await ipcSearch(q);
      if (id !== seq) return;
      hits = result;
      elapsedMs = Math.round(performance.now() - started);
      ranQuery = q;
      selected = result[0]?.id ?? null;
    } catch (e) {
      // A vault that cannot answer is a real state. Saying "no results" when the
      // backend is down is the one lie that would cost text later.
      if (id !== seq) return;
      error = String(e);
      hits = [];
      elapsedMs = null;
      ranQuery = q;
      selected = null;
    } finally {
      if (id === seq) running = false;
    }
  }

  return {
    get tokens() { return tokens; },
    get text() { return text; },
    get query() { return query; },
    get hits() { return hits; },
    get running() { return running; },
    get error() { return error; },
    get elapsedMs() { return elapsedMs; },
    get hasRun() { return hasRun; },
    /** The query the results on screen answer — what the preview highlights,
     *  which is not necessarily what the box currently holds. */
    get ranQuery() { return ranQuery; },
    get stale() { return stale; },
    get highlighted() { return highlighted; },
    get selected() { return selected; },
    get selectedHit() { return selectedHit; },

    /**
     * What the input holds.
     *
     * A token becomes a chip when it is *finished* — that is, when the text ends
     * in whitespace. Converting eagerly would turn `type:b` into a chip on the
     * way to `type:bug`, and the user would be editing a chip they never meant
     * to make.
     */
    setText(value: string): void {
      if (!/\s$/.test(value)) {
        text = value;
        return;
      }
      const parsed = parseQuery(value);
      for (const t of parsed.tokens) {
        if (!tokens.some((existing) => sameToken(existing, t))) tokens = [...tokens, t];
      }
      // The trailing space survives so the caret keeps its distance from the
      // chip that was just made.
      text = parsed.text ? `${parsed.text} ` : '';
    },

    /** Add a chip, or remove it when it is already there — a Tags or Types row
     *  is a toggle, not an accumulator of duplicates. */
    toggleToken(token: QueryToken): void {
      const at = tokens.findIndex((t) => sameToken(t, token));
      tokens = at === -1 ? [...tokens, token] : tokens.filter((_, i) => i !== at);
    },

    /** Whether a chip is currently part of the query — what the sidebar rows
     *  render their selected state from. */
    has(token: QueryToken): boolean {
      return tokens.some((t) => sameToken(t, token));
    },

    /**
     * Toggle a chip and search again — one call, because a sidebar row that
     * changed the query without running it would leave the results contradicting
     * the filters shown above them.
     */
    async toggleAndRun(token: QueryToken): Promise<void> {
      const at = tokens.findIndex((t) => sameToken(t, token));
      tokens = at === -1 ? [...tokens, token] : tokens.filter((_, i) => i !== at);
      await run();
    },

    removeToken(index: number): void {
      tokens = tokens.filter((_, i) => i !== index);
    },

    /** Drop the last chip — Backspace in an empty input. */
    dropLastToken(): void {
      tokens = tokens.slice(0, -1);
    },

    /** Empty the box and the results together: a cleared query with the previous
     *  results still under it is the one state that reads as a lie. */
    clear(): void {
      tokens = [];
      text = '';
      clearResults();
    },

    clearResults,

    select(id: string | null): void {
      selected = id;
    },

    /** Move the preview one result up or down; `-1`/`+1`. Returns the newly
     *  selected id so a caller can scroll it into view. */
    step(delta: number): string | null {
      if (hits.length === 0) return null;
      const here = hits.findIndex((h) => h.id === selected);
      // `-1` lands on the first result going either way, which is what an arrow
      // key should do when nothing is selected yet.
      const next = here === -1
        ? 0
        : Math.min(hits.length - 1, Math.max(0, here + delta));
      selected = hits[next].id;
      return selected;
    },

    run,

    /**
     * Replace the query with `raw` and run it — the entry point for everything
     * outside the search box: the palette, a tag row, a type row.
     */
    async searchFor(raw: string): Promise<void> {
      const parsed = parseQuery(raw);
      tokens = parsed.tokens;
      text = parsed.text;
      await run();
    },
  };
}

export const garrulusSearchStore = createGarrulusSearchStore();
