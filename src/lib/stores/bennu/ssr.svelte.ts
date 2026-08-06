/**
 * Structural search & replace — the query, its results, and the replacement's two steps.
 *
 * ## Explain runs on every keystroke, search does not
 *
 * Reading a query is string work and touches no files, so it is debounced barely and the field
 * can say what is wrong *while you type it*. Running one walks the project, so it happens when
 * you ask — a search that fired on every character would make the panel unusable on the tree it
 * exists for.
 *
 * ## Replace is always two steps
 *
 * Preview, then apply what the preview showed. Never one: a structural replace rewrites places
 * you did not look at, and the whole reason to prefer it over a textual one is that it is
 * *precise* — which is only worth anything if you can check.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  explainQuery, ssrApply, ssrPreview, ssrSearch,
  type SsrExplained, type SsrHit, type SsrPreview, type SsrReport,
} from '$lib/ipc/bennu/ssr';

/** How long the query field sits still before it is read. Short: this is string work. */
const EXPLAIN_DEBOUNCE_MS = 120;

/** The BE payload: batches, then exactly one terminal event. */
interface Progress {
  id: string;
  hits?: SsrHit[];
  done?: boolean;
  report?: SsrReport;
  scanned?: number;
  parsed?: number;
  prefiltered?: boolean;
  capped?: boolean;
}

function createSsrStore() {
  let query = $state('');
  let replacement = $state('');
  /** Whether the replacement half is showing at all. Off by default: most queries are questions. */
  let replacing = $state(false);

  let explained = $state<SsrExplained | null>(null);
  let hits = $state<SsrHit[]>([]);
  let report = $state<SsrReport | null>(null);
  let searching = $state(false);
  let capped = $state(false);
  /** `(files admitted, files parsed, was the pre-filter usable)` — the panel's "why was that
   *  slow" line, and the only place the pre-filter is visible at all. */
  let scanned = $state(0);
  let parsed = $state(0);
  let prefiltered = $state(true);

  /**
   * How long the search took, in milliseconds.
   *
   * Worth showing because this is the one panel where the cost is *variable and explainable*: a
   * query with a literal to grep for parses a tenth of the project, one made only of holes
   * parses all of it. A number beside "4 800 files scanned, 380 parsed" turns the pre-filter
   * from an invisible mechanism into something you can aim at.
   *
   * It ticks while the scan runs and freezes on the terminal event, so a long scan says how long
   * it has been going rather than only how long it took.
   */
  let startedAt = $state(0);
  let elapsedMs = $state(0);
  let ticker: ReturnType<typeof setInterval> | null = null;

  function startClock() {
    stopClock();
    startedAt = Date.now();
    elapsedMs = 0;
    // Four times a second: fast enough to read as running, slow enough to cost nothing.
    ticker = setInterval(() => { elapsedMs = Date.now() - startedAt; }, 250);
  }

  function stopClock() {
    if (ticker) clearInterval(ticker);
    ticker = null;
    if (startedAt) elapsedMs = Date.now() - startedAt;
  }

  let preview = $state<SsrPreview | null>(null);
  let previewing = $state(false);
  let applyResult = $state<{ written: number; refused: { file: string; reason: string }[] } | null>(null);
  let error = $state<string | null>(null);

  /** Guards a search against an older one landing after it. */
  let seq = 0;
  /** Its own counter, NOT the search one: reading a query is a different round trip, and
   *  sharing a sequence would let a keystroke invalidate a scan already running. */
  let explainSeq = 0;
  let currentId = '';
  let explainTimer: ReturnType<typeof setTimeout> | null = null;
  let unlisten: UnlistenFn | null = null;

  async function attach(): Promise<UnlistenFn> {
    if (unlisten) return unlisten;
    unlisten = await listen<Progress>('arbor://bennu/ssr-progress', (e) => {
      const p = e.payload;
      if (p.id !== currentId) return; // a superseded search
      if (p.hits?.length) hits = [...hits, ...p.hits];
      if (p.done) {
        report = p.report ?? null;
        scanned = p.scanned ?? 0;
        parsed = p.parsed ?? 0;
        prefiltered = p.prefiltered ?? true;
        capped = p.capped ?? false;
        searching = false;
        stopClock();
      }
    });
    return unlisten;
  }

  return {
    get query() { return query; },
    get replacement() { return replacement; },
    get replacing() { return replacing; },
    get explained() { return explained; },
    get hits() { return hits; },
    get report() { return report; },
    get searching() { return searching; },
    get capped() { return capped; },
    get scanned() { return scanned; },
    get parsed() { return parsed; },
    get prefiltered() { return prefiltered; },
    get elapsedMs() { return elapsedMs; },
    get preview() { return preview; },
    get previewing() { return previewing; },
    get applyResult() { return applyResult; },
    get error() { return error; },

    /** Whether the query is one the backend agreed to read. */
    get valid() { return !!explained && !explained.error; },

    /** Attach the progress listener. Called once, from the panel's mount. */
    attach,

    setQuery(text: string) {
      query = text;
      // The results describe the OLD query the moment the text changes; keeping them under a
      // different query is how someone reads yesterday's answer as today's.
      hits = [];
      report = null;
      preview = null;
      applyResult = null;
      if (explainTimer) clearTimeout(explainTimer);
      explainTimer = setTimeout(() => void explain(), EXPLAIN_DEBOUNCE_MS);
    },

    setReplacement(text: string) {
      replacement = text;
      preview = null;
      applyResult = null;
    },

    setReplacing(yes: boolean) {
      replacing = yes;
      if (!yes) { replacement = ''; preview = null; applyResult = null; }
    },

    /** Run the query. */
    async search(root: string) {
      if (!root || !query.trim()) return;
      const id = `ssr-${++seq}`;
      currentId = id;
      hits = [];
      report = null;
      preview = null;
      applyResult = null;
      capped = false;
      error = null;
      searching = true;
      startClock();
      await attach();
      try {
        await ssrSearch(root, query, id);
      } catch (e) {
        if (id !== currentId) return;
        searching = false;
        stopClock();
        error = String(e);
      }
    },

    /** Build the before/after of every file the replacement would touch. */
    async buildPreview(root: string) {
      if (!root || !query.trim()) return;
      previewing = true;
      error = null;
      applyResult = null;
      try {
        preview = await ssrPreview(root, query, replacement);
      } catch (e) {
        preview = null;
        error = String(e);
      } finally {
        previewing = false;
      }
    },

    /** Write what the preview showed. A no-op without one — there is nothing to apply. */
    async apply(root: string) {
      const current = preview;
      if (!current || !current.files.length) return;
      try {
        const done = await ssrApply(
          root,
          current.files.map((f) => ({ file: f.file, digest: f.digest, after: f.after })),
        );
        applyResult = { written: done.written.length, refused: done.refused };
        // The plan described files that no longer look like that, whether it wrote them or not.
        preview = null;
        hits = [];
        report = null;
      } catch (e) {
        error = String(e);
      }
    },

    /** Seed the field — from a saved query, or from an example in the docs. */
    load(text: string) {
      this.setQuery(text);
    },

    /** Forget everything. Called when the project changes. */
    clear() {
      seq++;
      stopClock();
      startedAt = 0;
      elapsedMs = 0;
      currentId = '';
      query = '';
      replacement = '';
      replacing = false;
      explained = null;
      hits = [];
      report = null;
      preview = null;
      applyResult = null;
      searching = false;
      error = null;
    },
  };

  async function explain() {
    if (!query.trim()) { explained = null; return; }
    const mine = ++explainSeq;
    try {
      const answer = await explainQuery(query);
      if (mine !== explainSeq) return;
      explained = answer;
    } catch {
      // The backend is absent or still starting. The field stays quiet rather than showing a
      // transport error where a syntax message goes.
    }
  }
}

export const bennuSsrStore = createSsrStore();
