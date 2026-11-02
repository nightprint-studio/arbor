/**
 * Full-text search for `SettingsShell` — opt-in, driven by the shell's `searchIndex` snippet.
 *
 * Searching only the nav labels is not enough for a settings dialog with dozens of pages: the thing
 * you remember is a sentence from a row ("Auto-fetch interval"), not the name of the page it lives
 * on. So the shell can index the *text* of every page and narrow the nav to the pages that contain
 * the query, then highlight the hits in the page showing (F3 / Shift+F3 walks them).
 *
 * The index is built lazily, on the first keystroke: it means mounting every page off-screen once
 * (their effects and IPC run), which is a cost worth paying only for someone who actually searches.
 */
import { tick, untrack } from 'svelte';
import {
  compileQuery, textMatches, injectHighlights, clearHighlights,
} from '$lib/utils/text-search';

/** Structural subset of the shell's nav types (kept local so this module does not import a component). */
export interface FtsNavItem { id: string; label: string; children?: FtsNavItem[] }
export interface FtsNavGroup { label: string; items: FtsNavItem[] }

export interface FullTextHost {
  enabled: () => boolean;
  groups: () => FtsNavGroup[];
  query: () => string;
  setQuery: (q: string) => void;
  regex: () => boolean;
  active: () => string;
  setActive: (id: string) => void;
  /** The element holding the page showing — where highlights are injected. */
  contentEl: () => HTMLElement | null;
  /** The off-screen container the pages are mounted in while the index is built. */
  indexEl: () => HTMLElement | null;
}

const HIGHLIGHT_CLASS = 'settings-match';

function flatten(items: FtsNavItem[]): FtsNavItem[] {
  return items.flatMap((i) => [i, ...flatten(i.children ?? [])]);
}

function cssEscape(s: string): string {
  return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/(["\\])/g, '\\$1');
}

/** Skipping pre/code in the index keeps it consistent with the highlight injector, so a page can
 *  never be listed as a match while showing zero `<mark>` tags in its body. */
function extractText(root: HTMLElement): string {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      let p = node.parentElement;
      while (p && p !== root) {
        const t = p.tagName;
        if (t === 'PRE' || t === 'CODE' || t === 'SCRIPT' || t === 'STYLE') {
          return NodeFilter.FILTER_REJECT;
        }
        p = p.parentElement;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const parts: string[] = [];
  let n: Node | null;
  while ((n = walker.nextNode())) parts.push(n.nodeValue ?? '');
  return parts.join(' ');
}

export function createFullTextSearch(host: FullTextHost) {
  let matchingIds = $state<Set<string>>(new Set());
  let labelMatchIds = $state<Set<string>>(new Set());
  let contentMarks = $state<HTMLElement[]>([]);
  let currentMarkIdx = $state(0);
  /** True while the pages are mounted off-screen for indexing; the shell renders them meanwhile. */
  let extracting = $state(false);

  const searchActive = $derived(host.enabled() && host.query().trim().length > 0);
  const compiledQuery = $derived.by(() => compileQuery(host.query().trim(), { regex: host.regex() }));
  const regexInvalid = $derived(
    host.regex() && host.query().trim().length > 0 && compiledQuery === null,
  );

  const allIds = () => host.groups().flatMap((g) => flatten(g.items).map((i) => i.id));

  // Per-page text, built once per dialog and reused across queries.
  const textCache = new Map<string, string>();
  let cacheBuilding: Promise<void> | null = null;

  async function ensureCache(): Promise<void> {
    if (textCache.size > 0) return;
    if (cacheBuilding) return cacheBuilding;
    cacheBuilding = (async () => {
      extracting = true;
      await tick();
      await tick();
      const root = host.indexEl();
      if (root) {
        for (const id of allIds()) {
          const el = root.querySelector<HTMLElement>(`[data-section="${cssEscape(id)}"]`);
          if (el) textCache.set(id, extractText(el));
        }
      }
      extracting = false;
      cacheBuilding = null;
    })();
    return cacheBuilding;
  }

  async function applyContentHighlights() {
    await tick();
    await tick();
    const el = host.contentEl();
    if (!el || !el.isConnected) return;
    clearHighlights(el, HIGHLIGHT_CLASS);
    contentMarks = [];
    currentMarkIdx = 0;
    const re = compiledQuery;
    if (!re || !searchActive) return;
    const marks = injectHighlights(el, re, { className: HIGHLIGHT_CLASS });
    contentMarks = marks;
    if (marks.length > 0) {
      marks[0].classList.add('current');
      marks[0].scrollIntoView({ block: 'center', behavior: 'auto' });
    }
  }

  async function doSearch() {
    if (!searchActive) {
      matchingIds = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    await ensureCache();
    const re = compiledQuery;
    if (!re) {
      matchingIds = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    const matches = new Set<string>();
    const labels = new Set<string>();
    for (const g of host.groups()) {
      const groupLabelHit = textMatches(g.label, re);
      for (const item of flatten(g.items)) {
        const labelHit = textMatches(item.label, re) || groupLabelHit;
        if (labelHit) labels.add(item.id);
        if (labelHit || textMatches(textCache.get(item.id) ?? '', re)) matches.add(item.id);
      }
    }
    matchingIds = matches;
    labelMatchIds = labels;

    if (!matches.has(host.active())) {
      const first = allIds().find((id) => matches.has(id));
      if (first) host.setActive(first);
    }

    await applyContentHighlights();
  }

  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  /** Debounced: the input fires per keystroke, the walk over every page's text should not. */
  function onInput() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(doSearch, 120);
  }

  function clear() {
    host.setQuery('');
    matchingIds = new Set();
    labelMatchIds = new Set();
    const el = host.contentEl();
    if (el) {
      clearHighlights(el, HIGHLIGHT_CLASS);
      contentMarks = [];
      currentMarkIdx = 0;
    }
  }

  function gotoMark(idx: number) {
    if (contentMarks.length === 0) return;
    const wrapped = ((idx % contentMarks.length) + contentMarks.length) % contentMarks.length;
    contentMarks.forEach((m) => m.classList.remove('current'));
    const target = contentMarks[wrapped];
    target.classList.add('current');
    target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    currentMarkIdx = wrapped;
  }

  /** No hit left in the page showing → step to the next page that has one. */
  function jumpSection(dir: 1 | -1) {
    const order = allIds().filter((id) => matchingIds.has(id));
    if (order.length === 0) return;
    const i = order.indexOf(host.active());
    if (i === -1) { host.setActive(order[0]); return; }
    host.setActive(order[(i + dir + order.length) % order.length]);
  }

  function next() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx + 1);
    else jumpSection(+1);
  }
  function prev() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx - 1);
    else jumpSection(-1);
  }

  // Toggling regex mode changes what the same text means, so it re-runs the search at once.
  $effect(() => {
    if (!host.enabled()) return;
    host.regex();
    if (searchActive) untrack(() => { void doSearch(); });
  });

  // Highlights live in the page's DOM, which is replaced on every page switch.
  $effect(() => {
    if (!host.enabled()) return;
    host.active();
    untrack(() => { void applyContentHighlights(); });
  });

  $effect(() => {
    if (!host.enabled()) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'F3') {
        e.preventDefault();
        if (e.shiftKey) prev(); else next();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  return {
    get searchActive() { return searchActive; },
    get compiledQuery() { return compiledQuery; },
    get regexInvalid() { return regexInvalid; },
    get matchingIds() { return matchingIds; },
    get labelMatchIds() { return labelMatchIds; },
    get markCount() { return contentMarks.length; },
    get currentMark() { return currentMarkIdx; },
    get extracting() { return extracting; },
    onInput, clear, next, prev,
  };
}
