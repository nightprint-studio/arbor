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
 * *precise* — which is only worth anything if you can check. The preview lists files; the ones
 * ticked are the ones applied.
 *
 * ## What is kept
 *
 * The recent queries and the templates the user saved live in the per-profile Bennu config
 * (`ssr_recent`, `ssr_saved`), by a read-modify-write like the regex store's — never `localStorage`.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { getBennuConfig, setBennuConfig, type BennuSsrQuery } from '$lib/ipc/bennu/config';
import {
  explainQuery, ssrApply, ssrCancel, ssrLanguages, ssrPreview, ssrSearch,
  type SsrDialect, type SsrExplained, type SsrHit, type SsrPreview, type SsrReport,
} from '$lib/ipc/bennu/ssr';
import { WHOLE_PROJECT, withScope, type SsrScope } from '$lib/components/bennu/ssr/ssr-model';

/** How long the query field sits still before it is read. Short: this is string work. */
const EXPLAIN_DEBOUNCE_MS = 120;
/** How many runs the history keeps. */
const RECENT_LIMIT = 15;

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
  error?: string | null;
}

export type KeptQuery = BennuSsrQuery;

const sameQuery = (a: KeptQuery, b: KeptQuery) =>
  a.query === b.query && a.replacement === b.replacement && a.dialect === b.dialect;

function createSsrStore() {
  let query = $state('');
  /**
   * Which language the query is written in.
   *
   * A choice, never a guess. `<s:property value="$x$"/>` and `log.debug($x$)` are both plausible
   * text, and a wrong guess does not fail loudly — it compiles a pattern that matches nothing,
   * which reads as "the project contains none of this". So the panel asks.
   */
  let dialect = $state<SsrDialect>('java');
  let replacement = $state('');
  /** Whether the replacement half is showing at all. Off by default: most queries are questions. */
  let replacing = $state(false);
  let scope = $state<SsrScope>(WHOLE_PROJECT);

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
  /** The last search was stopped by the user: the list is what had arrived, not the whole answer. */
  let cancelled = $state(false);

  /** Whether the project holds pages. `null` until asked — until then only Java is offered. */
  let hasPages = $state<boolean | null>(null);
  let languagesFor = '';

  /**
   * How long the search took, in milliseconds. It ticks while the scan runs and freezes on the
   * terminal event, so a long scan says how long it has been going rather than only how long it took.
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
  /** The previewed files to write — all of them until the user unticks some. */
  let applyFiles = $state<Set<string>>(new Set());
  let applyResult = $state<{ written: number; refused: { file: string; reason: string }[] } | null>(null);
  let error = $state<string | null>(null);

  let saved = $state<KeptQuery[]>([]);
  let recent = $state<KeptQuery[]>([]);

  /** Guards a search against an older one landing after it. */
  let seq = 0;
  /** Its own counter, NOT the search one: reading a query is a different round trip, and
   *  sharing a sequence would let a keystroke invalidate a scan already running. */
  let explainSeq = 0;
  let currentId = '';
  let explainTimer: ReturnType<typeof setTimeout> | null = null;
  let unlisten: UnlistenFn | null = null;

  async function persist(patch: { ssr_saved?: KeptQuery[]; ssr_recent?: KeptQuery[] }) {
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({ ...cur, ...patch });
    } catch {
      /* BE absent — the choice holds for this session */
    }
  }

  function forgetResults() {
    hits = [];
    report = null;
    preview = null;
    applyResult = null;
    cancelled = false;
  }

  function scheduleExplain() {
    if (explainTimer) clearTimeout(explainTimer);
    explainTimer = setTimeout(() => void explain(), EXPLAIN_DEBOUNCE_MS);
  }

  async function explain() {
    if (!query.trim()) { explained = null; return; }
    const mine = ++explainSeq;
    try {
      const answer = await explainQuery(withScope(query, scope), dialect);
      if (mine !== explainSeq) return;
      explained = answer;
    } catch {
      // The backend is absent or still starting. The field stays quiet rather than showing a
      // transport error where a syntax message goes.
    }
  }

  function remember(entry: KeptQuery) {
    recent = [entry, ...recent.filter((r) => !sameQuery(r, entry))].slice(0, RECENT_LIMIT);
    void persist({ ssr_recent: recent });
  }

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
        // A walk that refused the query says so. Without this the panel showed "nothing in this
        // project matches that" — which is a statement about the project, for a problem in the
        // pattern.
        if (p.error) {
          error = p.error;
          report = null;
        }
        searching = false;
        stopClock();
      }
    });
    return unlisten;
  }

  return {
    get query() { return query; },
    get dialect() { return dialect; },
    get replacement() { return replacement; },
    get replacing() { return replacing; },
    get scope() { return scope; },
    get explained() { return explained; },
    get hits() { return hits; },
    get report() { return report; },
    get searching() { return searching; },
    get cancelled() { return cancelled; },
    get capped() { return capped; },
    get scanned() { return scanned; },
    get parsed() { return parsed; },
    get prefiltered() { return prefiltered; },
    get elapsedMs() { return elapsedMs; },
    get preview() { return preview; },
    get previewing() { return previewing; },
    get applyFiles() { return applyFiles; },
    get applyResult() { return applyResult; },
    get error() { return error; },
    get saved() { return saved; },
    get recent() { return recent; },
    /** Whether the project has pages: `null` while unknown. Java is always there. */
    get hasPages() { return hasPages; },

    /** Whether the query is one the backend agreed to read. */
    get valid() { return !!explained && !explained.error; },

    /** Attach the progress listener. Called once, from the panel's mount. */
    attach,

    setQuery(text: string) {
      query = text;
      // The results describe the OLD query the moment the text changes; keeping them under a
      // different query is how someone reads yesterday's answer as today's.
      forgetResults();
      scheduleExplain();
    },

    /** Switch language. The results answered the OTHER question, so they go — and the query is
     *  re-read, because what compiles under one grammar rarely compiles under the other. */
    setDialect(next: SsrDialect) {
      if (next === dialect) return;
      dialect = next;
      forgetResults();
      scheduleExplain();
    },

    setScope(next: SsrScope) {
      scope = next;
      forgetResults();
      scheduleExplain();
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

    /** Ask what the project holds, once per root; a failure leaves Java as the only language. */
    async loadLanguages(root: string) {
      if (!root || languagesFor === root) return;
      languagesFor = root;
      try {
        const found = await ssrLanguages(root);
        if (languagesFor !== root) return;
        hasPages = found.jsp;
        // A query left in a page language in a project that has none would search nothing.
        if (!found.jsp && dialect !== 'java') this.setDialect('java');
      } catch {
        if (languagesFor === root) hasPages = null;
      }
    },

    /** Run the query. */
    async search(root: string) {
      if (!root || !query.trim()) return;
      const id = `ssr-${++seq}`;
      currentId = id;
      forgetResults();
      capped = false;
      error = null;
      searching = true;
      startClock();
      remember({ title: '', query, replacement, dialect });
      await attach();
      try {
        await ssrSearch(root, withScope(query, scope), id, dialect);
      } catch (e) {
        if (id !== currentId) return;
        searching = false;
        stopClock();
        error = String(e);
      }
    },

    /** Stop the running search. What had arrived stays, marked as partial. */
    cancel() {
      if (!searching) return;
      const id = currentId;
      currentId = '';
      searching = false;
      cancelled = true;
      stopClock();
      void ssrCancel(id).catch(() => { /* an older backend: the walk finishes unseen */ });
    },

    /** Build the before/after of every file the replacement would touch. */
    async buildPreview(root: string) {
      if (!root || !query.trim()) return;
      previewing = true;
      error = null;
      applyResult = null;
      try {
        const built = await ssrPreview(root, withScope(query, scope), replacement, dialect);
        preview = built;
        applyFiles = new Set(built.files.map((f) => f.file));
      } catch (e) {
        preview = null;
        error = String(e);
      } finally {
        previewing = false;
      }
    },

    toggleApplyFile(file: string, on?: boolean) {
      const next = new Set(applyFiles);
      const want = on ?? !next.has(file);
      if (want) next.add(file); else next.delete(file);
      applyFiles = next;
    },

    setAllApplyFiles(on: boolean) {
      applyFiles = on ? new Set(preview?.files.map((f) => f.file) ?? []) : new Set();
    },

    /** Write the ticked files of the preview. A no-op without one — there is nothing to apply. */
    async apply(root: string) {
      const current = preview;
      if (!current) return;
      const chosen = current.files.filter((f) => applyFiles.has(f.file));
      if (!chosen.length) return;
      try {
        const done = await ssrApply(root, chosen.map((f) => ({ file: f.file, digest: f.digest, after: f.after })));
        applyResult = { written: done.written.length, refused: done.refused };
        // The plan described files that no longer look like that, whether it wrote them or not.
        preview = null;
        hits = [];
        report = null;
      } catch (e) {
        error = String(e);
      }
    },

    /** Seed the field — from a saved query, a recent one or an example. */
    load(text: string, nextDialect?: SsrDialect, nextReplacement?: string) {
      if (nextDialect && nextDialect !== dialect) dialect = nextDialect;
      if (nextReplacement !== undefined) {
        replacement = nextReplacement;
        if (nextReplacement) replacing = true;
      }
      this.setQuery(text);
    },

    /** Keep the current query under `title`, replacing a template of the same name. */
    saveTemplate(title: string) {
      const name = title.trim();
      if (!name || !query.trim()) return;
      saved = [{ title: name, query, replacement, dialect }, ...saved.filter((s) => s.title !== name)];
      void persist({ ssr_saved: saved });
    },

    removeTemplate(title: string) {
      saved = saved.filter((s) => s.title !== title);
      void persist({ ssr_saved: saved });
    },

    clearRecent() {
      recent = [];
      void persist({ ssr_recent: [] });
    },

    async loadConfig() {
      try {
        const cfg = await getBennuConfig();
        saved = cfg.ssr_saved ?? [];
        recent = cfg.ssr_recent ?? [];
      } catch {
        /* keep what is there — BE absent / not ready */
      }
    },

    /** Put the store in a given state without a backend — for the component gallery only. */
    seed(s: {
      query: string; replacement?: string; hits?: SsrHit[]; report?: SsrReport | null; explained?: SsrExplained | null;
      preview?: SsrPreview | null; scanned?: number; parsed?: number; elapsedMs?: number; hasPages?: boolean;
    }) {
      query = s.query;
      replacement = s.replacement ?? '';
      replacing = !!s.replacement || !!s.preview;
      hits = s.hits ?? [];
      report = s.report ?? null;
      explained = s.explained ?? null;
      preview = s.preview ?? null;
      applyFiles = new Set(s.preview?.files.map((f) => f.file) ?? []);
      scanned = s.scanned ?? 0;
      parsed = s.parsed ?? 0;
      elapsedMs = s.elapsedMs ?? 0;
      hasPages = s.hasPages ?? null;
      languagesFor = 'seeded';
    },

    /** Forget everything. Called when the project changes. */
    clear() {
      seq++;
      stopClock();
      startedAt = 0;
      elapsedMs = 0;
      currentId = '';
      query = '';
      dialect = 'java';
      replacement = '';
      replacing = false;
      scope = WHOLE_PROJECT;
      explained = null;
      hits = [];
      report = null;
      preview = null;
      applyFiles = new Set();
      applyResult = null;
      searching = false;
      cancelled = false;
      error = null;
      hasPages = null;
      languagesFor = '';
    },
  };
}

export const bennuSsrStore = createSsrStore();
