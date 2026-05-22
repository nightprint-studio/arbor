<!--
  StudioQueryBar — format-agnostic JSONPath search bar.

  Extracted from RonStudioModal.svelte (Phase 2B-2, step 1). Owns the
  full query lifecycle: input + ghost-autocomplete (Tab to accept),
  debounced run, history dropdown (per-format localStorage namespace),
  hit counter + ↑/↓ navigation inside the input, F3/Shift+F3 from
  outside via imperative `nav()` method, inline hit list collapses
  when the right sidebar pane is open.

  Tree integration is delegated to the parent via two callbacks:
  `getChildKeysForPath` (synchronous read of already-loaded subtree)
  and `ensureChildrenLoaded` (kick a lazy load when the ghost needs
  children we don't have yet). Jumping a hit to the tree is the
  parent's job through `onJumpToHit(path)`.

  Format-specific styling for kind badges and preview tokens flows
  through the `kindChip` snippet prop — RON renders its 13-kind
  palette, future formats (JSON/TOML/YAML/.properties) pass their own
  snippet. The bar itself stays kind-agnostic.

  Imperative API (via `bind:this`): `focus()`, `clear()`,
  `nav(delta)`, `getHitCount()`. Used by the parent's global F3/
  Ctrl+F handler so the bar can drive navigation even when focus is
  outside it.
-->
<script lang="ts" generics="TKind extends string">
  import { untrack } from 'svelte';
  import type { Snippet } from 'svelte';
  import {
    Search, Loader2, History, ChevronDown,
    AlertCircle, ArrowUp, ArrowDown, ArrowUpRight, ArrowRight,
    Link as LinkIcon, Trash2, PencilRuler,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import Dropdown, { type DropdownItem } from '../ui/Dropdown.svelte';
  import type {
    StudioBackend, StudioFormat, StudioQueryHit,
  } from '$lib/ipc/studio-format';

  // ── Props ────────────────────────────────────────────────────────

  interface Props {
    /** Identifies the format for storage namespacing + display. */
    formatId: StudioFormat;
    /** Pre-bound backend factory. Only `query(docId, expr)` is used. */
    backend: StudioBackend<TKind>;
    /** Active doc id — query short-circuits to `[]` when null. */
    docId: string | null;
    /** Parent's visibility gate (e.g. tree view + no parse error).
     *  When false the bar renders nothing AND active state resets. */
    visible: boolean;
    /** Input placeholder. Default is generic JSONPath syntax; format
     *  wrappers pass their own example flavored to their kind set. */
    placeholder?: string;
    /** localStorage key for the history list. Per-format namespace.
     *  RON keeps its legacy key for user-history continuity. */
    historyStorageKey?: string;

    /** Keys observed by the parent (e.g. tree expansion + hit results)
     *  fed back here for the global-mode ghost (`$..foo`). */
    knownKeys: Set<string>;
    /** Synchronous read of children for a path, when the parent has
     *  them materialised. Return `null` when the subtree hasn't been
     *  loaded yet — the bar then calls `ensureChildrenLoaded` so the
     *  ghost can resolve on the next keystroke. */
    getChildKeysForPath: (path: string[]) => string[] | null;
    /** Kick an async fetch of `path`'s children. Best-effort. */
    ensureChildrenLoaded: (path: string[]) => void;

    /** Jump a hit's path to the tree + select it. Owned by parent
     *  because tree state (`roots`, `expanded`, `byPid`, …) is
     *  parent state. */
    onJumpToHit: (path: string[]) => void;
    /** Whether the parent's dedicated query-results sidebar pane is
     *  open. Drives the inline list collapse + the `↗ N` toggle. */
    rightPaneOpen: boolean;
    /** Toggle the parent's query sidebar pane. */
    onToggleRightPane: () => void;
    /** Fires whenever the active state flips (empty ↔ non-empty
     *  query). Parent uses this for auto-open / auto-dismiss logic
     *  of the right sidebar pane. */
    onActiveChange?: (active: boolean) => void;
    /** Fires after a successful run that produced ≥1 hit. Parent
     *  may use this to feed `knownKeys` from the hit set. */
    onHits?: (hits: StudioQueryHit<TKind>[]) => void;

    /** Renders the badge chip for a hit's kind. RON-specific styling
     *  flows through here; the bar stays format-agnostic. */
    kindChip: Snippet<[TKind]>;
    /** Optional preview text renderer — default: plain text in a
     *  muted span. Use to color/syntax-highlight previews. */
    previewRender?: Snippet<[string]>;
    /** Extra buttons injected at the right edge of the toolbar.
     *  RON uses this for Expand-All / Collapse-All which operate on
     *  parent's tree state. */
    toolbarRight?: Snippet;

    /** F13 — Show the `[⚡ Edit]` button next to the hit counter.
     *  Wrapper passes `true` only when the backend's descriptor
     *  reports `supports_bulk_edit`. Disabled when there are no
     *  hits (or the query is empty). */
    bulkEditEnabled?: boolean;
    /** F13 — Fires when the user clicks `[⚡ Edit]`. Wrapper opens
     *  the format-agnostic `StudioBulkEditModal` with the current
     *  `query` + `queryHits`. The bar stays focused on read-only
     *  query state; the modal owns the action/value/preview UI. */
    onBulkEditRequest?: (query: string) => void;

    // ── $bindable state — parent may bind these for read access
    //    (e.g. a dedicated query-results sidebar pane mirrors them).
    //    The bar owns the state; bindings flow OUT to the parent.
    query?:         string;
    queryHits?:     StudioQueryHit<TKind>[];
    querying?:      boolean;
    queryError?:    string | null;
    currentHitIdx?: number;
  }

  let {
    formatId,
    backend,
    docId,
    visible,
    placeholder = 'Query — name (recursive), $.foo.bar, $.arr[0:5], $..[?@.field == "value"]…',
    historyStorageKey,
    knownKeys,
    getChildKeysForPath,
    ensureChildrenLoaded,
    onJumpToHit,
    rightPaneOpen,
    onToggleRightPane,
    onActiveChange,
    onHits,
    kindChip,
    previewRender,
    toolbarRight,
    bulkEditEnabled = false,
    onBulkEditRequest,
    query         = $bindable(''),
    queryHits     = $bindable<StudioQueryHit<TKind>[]>([]),
    querying      = $bindable(false),
    queryError    = $bindable<string | null>(null),
    currentHitIdx = $bindable(0),
  }: Props = $props();

  const HISTORY_KEY = $derived(
    historyStorageKey ?? `arbor:studio:${formatId}:query-history`
  );
  const HISTORY_CAP = 20;
  const QUERY_DEBOUNCE_MS = 220;

  // ── Internal state ───────────────────────────────────────────────
  // Bindable state lives on the props above. The few non-bound items:

  let queryInputEl: HTMLInputElement | undefined = $state();
  let queryTimer: ReturnType<typeof setTimeout> | null = null;
  // Horizontal scroll position of the input — mirrored onto the
  // highlight overlay so the colored tokens stay glued to the caret
  // when the typed query exceeds the visible row width.
  // `input` doesn't reliably fire `scroll` after every value change
  // (the caret-driven scroll happens AFTER the value is committed),
  // so we sync via rAF on every input/keydown in addition to the
  // native scroll event.
  let inputScrollLeft = $state(0);
  function syncScroll() {
    const el = queryInputEl;
    if (!el) return;
    requestAnimationFrame(() => {
      if (queryInputEl) inputScrollLeft = queryInputEl.scrollLeft;
    });
  }

  // ── JSONPath tokenizer (powers the syntax-highlight overlay) ─────
  // Splits the raw query into typed chunks so each role (root,
  // recurse, dot, field, bracket, index, slice, filter, current,
  // wildcard, operator, string, plain) gets its own color class.
  // Pure function — no Svelte state captured.

  type SqbTokenType =
    | 'root' | 'dot' | 'recurse' | 'field' | 'bracket' | 'index'
    | 'slice' | 'filter' | 'current' | 'wildcard' | 'operator'
    | 'string' | 'plain';
  // `depth` is meaningful only for `field` tokens — it carries the
  // 0-based segment index inside the path so the renderer can rotate
  // the field hue per level (so consecutive segments don't blur into
  // one solid color).
  type SqbToken = { text: string; type: SqbTokenType; depth?: number };

  // Distinct hues for cycling field colors. Lightness/saturation
  // tuned to read on both dark + light themes without relying on
  // any `--syntax-*` theme variable (the project supports custom
  // themes that may not declare them).
  const FIELD_HUES = [210, 160, 35, 290, 340, 110];
  function fieldColor(depth: number): string {
    const h = FIELD_HUES[depth % FIELD_HUES.length];
    return `hsl(${h}, 62%, 62%)`;
  }

  function tokenizePath(s: string): SqbToken[] {
    const out: SqbToken[] = [];
    let i = 0;
    let bracket = 0;
    // Segment counter — used as `depth` for path-level field tokens
    // so each level gets its own hue from `FIELD_HUES`.
    let segIdx = 0;
    while (i < s.length) {
      const c = s[i];

      if (c === '$' && bracket === 0) {
        out.push({ text: '$', type: 'root' }); i++; continue;
      }
      if (c === '.' && bracket === 0) {
        if (s[i + 1] === '.') { out.push({ text: '..', type: 'recurse' }); i += 2; }
        else                  { out.push({ text: '.',  type: 'dot'     }); i++;    }
        continue;
      }
      if (c === '[') { out.push({ text: '[', type: 'bracket' }); i++; bracket++; continue; }
      if (c === ']') { out.push({ text: ']', type: 'bracket' }); i++; if (bracket > 0) bracket--; continue; }

      if (bracket > 0) {
        // Inside a bracket expression — wider syntax: strings,
        // numbers, slice colon, filter ?, current @, wildcard *,
        // identifiers, comparison/logical operators.
        if (c === '"' || c === "'") {
          const quote = c;
          let j = i + 1;
          while (j < s.length && s[j] !== quote) {
            if (s[j] === '\\' && j + 1 < s.length) j += 2; else j++;
          }
          if (j < s.length) j++;
          out.push({ text: s.slice(i, j), type: 'string' }); i = j; continue;
        }
        if (c === '?') { out.push({ text: '?', type: 'filter'   }); i++; continue; }
        if (c === '@') { out.push({ text: '@', type: 'current'  }); i++; continue; }
        if (c === '*') { out.push({ text: '*', type: 'wildcard' }); i++; continue; }
        if (c === ':') { out.push({ text: ':', type: 'slice'    }); i++; continue; }
        if (c === '.') { out.push({ text: '.', type: 'dot'      }); i++; continue; }
        if (/[0-9]/.test(c) || (c === '-' && /[0-9]/.test(s[i + 1] ?? ''))) {
          let j = i + 1; while (j < s.length && /[0-9]/.test(s[j])) j++;
          out.push({ text: s.slice(i, j), type: 'index' }); i = j; continue;
        }
        if ('=!<>&|'.includes(c)) {
          let j = i + 1;
          if (s[j] === '=' || (c === '&' && s[j] === '&') || (c === '|' && s[j] === '|')) j++;
          out.push({ text: s.slice(i, j), type: 'operator' }); i = j; continue;
        }
        if (/[A-Za-z_]/.test(c)) {
          let j = i + 1; while (j < s.length && /[\w-]/.test(s[j])) j++;
          // Inside brackets the field belongs to a filter sub-path
          // (e.g. @.foo) — color it as a fresh sub-segment using
          // the running counter so it still gets cycled.
          out.push({ text: s.slice(i, j), type: 'field', depth: segIdx++ }); i = j; continue;
        }
        out.push({ text: c, type: 'plain' }); i++; continue;
      }

      // Outside brackets — bare identifier or wildcard. Bump segIdx
      // for every field so successive `.a.b.c` segments rotate
      // through `FIELD_HUES`.
      if (/[A-Za-z_]/.test(c)) {
        let j = i + 1; while (j < s.length && /[\w-]/.test(s[j])) j++;
        out.push({ text: s.slice(i, j), type: 'field', depth: segIdx++ }); i = j; continue;
      }
      if (c === '*') { out.push({ text: '*', type: 'wildcard' }); i++; continue; }
      out.push({ text: c, type: 'plain' }); i++;
    }
    return out;
  }

  const highlightTokens = $derived(tokenizePath(query));

  // Tokenize a hit path for the inline hit list. The path arrives as
  // string segments — we synthesise the `$` root + dots so the same
  // color rules apply as in the input.
  function tokenizeHitPath(segments: string[]): SqbToken[] {
    if (segments.length === 0) return [{ text: '$', type: 'root' }];
    const out: SqbToken[] = [{ text: '$', type: 'root' }];
    let depth = 0;
    for (const seg of segments) {
      if (/^\d+$/.test(seg)) {
        out.push({ text: '[',  type: 'bracket' });
        out.push({ text: seg,  type: 'index'   });
        out.push({ text: ']',  type: 'bracket' });
      } else {
        out.push({ text: '.',  type: 'dot'   });
        out.push({ text: seg,  type: 'field', depth: depth++ });
      }
    }
    return out;
  }

  // History — lazy-loaded on first toolbar interaction OK but the
  // dropdown reads on mount; load synchronously here.
  let recentQueries = $state<string[]>(untrack(() => loadHistory(HISTORY_KEY)));

  function loadHistory(key: string): string[] {
    try {
      const raw = localStorage.getItem(key);
      if (!raw) return [];
      const arr = JSON.parse(raw);
      return Array.isArray(arr)
        ? arr.filter((s: unknown): s is string => typeof s === 'string').slice(0, HISTORY_CAP)
        : [];
    } catch { return []; }
  }
  function pushHistory(q: string) {
    const t = q.trim();
    if (!t) return;
    const next = [t, ...recentQueries.filter(x => x !== t)].slice(0, HISTORY_CAP);
    recentQueries = next;
    try { localStorage.setItem(HISTORY_KEY, JSON.stringify(next)); } catch { /* quota */ }
  }
  function clearHistory() {
    recentQueries = [];
    try { localStorage.removeItem(HISTORY_KEY); } catch { /* ignore */ }
  }

  // Re-load history when the storage key changes (format switch). Rare
  // but cheap; keeps the dropdown coherent.
  $effect(() => {
    const key = HISTORY_KEY;
    untrack(() => {
      recentQueries = loadHistory(key);
    });
  });

  // ── Ghost autocomplete — same context model as JSON Studio. ──────
  // `$..foo` / `foo` pull from `knownKeys`; `$.parent.foo` pulls from
  // the children of the resolved parent (we ask the parent to lazy-
  // load it via `ensureChildrenLoaded` when not yet materialised).

  type GhostCtx =
    | { kind: 'global'; tail: string }
    | { kind: 'parent'; path: string[]; tail: string };

  function parseGhostCtx(s: string): GhostCtx | null {
    const t = s.trim();
    if (!t) return null;
    if (!t.startsWith('$') && !t.startsWith('.') && !t.startsWith('[')) {
      return /^[A-Za-z_][\w-]*$/.test(t) ? { kind: 'global', tail: t } : null;
    }
    const body = t.startsWith('$') ? t.slice(1) : t;
    if (body.startsWith('..')) {
      const m = body.match(/[.](\w*)$/);
      return { kind: 'global', tail: m ? m[1] : '' };
    }
    if (body.includes('[')) return null;
    const parts = body.split('.');
    if (parts[0] !== '') return null;
    const segments = parts.slice(1);
    if (segments.length === 0) return null;
    const tail = segments[segments.length - 1];
    const path = segments.slice(0, -1);
    return { kind: 'parent', path, tail };
  }

  function maybePreloadGhostParent(s: string) {
    const ctx = parseGhostCtx(s);
    if (!ctx || ctx.kind !== 'parent') return;
    // Synchronous read — if not loaded, parent kicks the fetch and
    // the ghost will resolve on the next keystroke.
    const keys = getChildKeysForPath(ctx.path);
    if (keys === null) ensureChildrenLoaded(ctx.path);
  }

  const ghostSuffix = $derived.by<string>(() => {
    const ctx = parseGhostCtx(query);
    if (!ctx) return '';
    let candidates: string[];
    if (ctx.kind === 'parent') {
      const keys = getChildKeysForPath(ctx.path);
      if (!keys) return '';
      candidates = keys.filter(k => !/^\d+$/.test(k));
    } else {
      candidates = Array.from(knownKeys);
    }
    const tail = ctx.tail;
    let best: string | null = null;
    for (const k of candidates) {
      if (k.length <= tail.length) continue;
      if (tail && !k.startsWith(tail)) continue;
      if (best === null || k.length < best.length) best = k;
    }
    return best ? best.slice(tail.length) : '';
  });

  function acceptGhost() {
    if (!ghostSuffix) return;
    query = query + ghostSuffix;
    maybePreloadGhostParent(query + '.');
    void runQuery();
  }

  // ── Query lifecycle ──────────────────────────────────────────────

  function scheduleQuery() {
    if (queryTimer) clearTimeout(queryTimer);
    maybePreloadGhostParent(query);
    const q = query.trim();
    if (!q) {
      queryHits  = [];
      queryError = null;
      querying   = false;
      onActiveChange?.(false);
      return;
    }
    onActiveChange?.(true);
    queryTimer = setTimeout(() => void runQuery(), QUERY_DEBOUNCE_MS);
  }

  async function runQuery() {
    if (!docId) return;
    const q = query.trim();
    if (!q) { queryHits = []; queryError = null; return; }
    querying = true;
    try {
      const hits = await backend.query(docId, q);
      queryHits  = hits;
      queryError = null;
      if (hits.length > 0) {
        pushHistory(q);
        onHits?.(hits);
      }
    } catch (e) {
      queryError = String(e);
      queryHits  = [];
    } finally {
      querying = false;
    }
  }

  // Reset hit cursor whenever a fresh result set arrives so the first
  // hit is the one ↑/↓ navigates from.
  $effect(() => {
    void queryHits;
    currentHitIdx = 0;
  });

  // Reset everything when the active doc changes — leftover search
  // state shouldn't apply to an unrelated document.
  $effect(() => {
    void docId;
    untrack(() => clearQueryInternal());
  });

  function clearQueryInternal() {
    query = '';
    queryHits = [];
    queryError = null;
    currentHitIdx = 0;
    if (queryTimer) { clearTimeout(queryTimer); queryTimer = null; }
    onActiveChange?.(false);
  }

  function navigateHitsInternal(delta: number) {
    if (queryHits.length === 0) return;
    const n = queryHits.length;
    currentHitIdx = ((currentHitIdx + delta) % n + n) % n;
    onJumpToHit(queryHits[currentHitIdx].path);
  }

  async function copyHitPath(hit: StudioQueryHit<TKind>) {
    const p = hit.path.length === 0 ? '$' : '$.' + hit.path.join('.');
    await copyToClipboard(p);
  }

  // History dropdown items — rebuilt on every recent-list change.
  const historyItems = $derived.by<DropdownItem[]>(() => {
    if (recentQueries.length === 0) {
      return [{
        kind: 'item', id: 'empty', label: 'No recent queries',
        disabled: true, onclick: () => {},
      }];
    }
    const items: DropdownItem[] = recentQueries.map((q, i) => ({
      kind:    'item',
      id:      `q${i}`,
      label:   q,
      onclick: () => { query = q; void runQuery(); queryInputEl?.focus(); },
    }));
    items.push({ kind: 'separator' });
    items.push({
      kind: 'item', id: 'clear', label: 'Clear history',
      icon: Trash2, onclick: () => clearHistory(),
    });
    return items;
  });

  // ── Imperative API (via bind:this) ───────────────────────────────

  export function focus() { queryInputEl?.focus(); }
  export function clear() { clearQueryInternal(); }
  export function nav(delta: number) { navigateHitsInternal(delta); }
  export function getHitCount(): number { return queryHits.length; }
  export function getQuery(): string { return query; }

  // ── Derived display flags ────────────────────────────────────────

  const hasResults  = $derived(query.trim().length > 0 && queryHits.length > 0);
  const queryActive = $derived(query.trim().length > 0);
</script>

{#if visible}
  <div class="sqb-block">
    <div class="sqb-row">
      <Search size={12} class="sqb-icon" />
      <div class="sqb-ghost-wrap">
        <input
          type="text"
          class="sqb-input"
          {placeholder}
          bind:this={queryInputEl}
          bind:value={query}
          oninput={() => { scheduleQuery(); syncScroll(); }}
          onscroll={() => { inputScrollLeft = queryInputEl?.scrollLeft ?? 0; }}
          onkeyup={syncScroll}
          onclick={syncScroll}
          onfocus={syncScroll}
          onkeydown={(e) => {
            if (e.key === 'Tab' && ghostSuffix) { e.preventDefault(); acceptGhost(); }
            else if (e.key === 'Enter') void runQuery();
            else if (e.key === 'ArrowDown' && queryHits.length > 0) { e.preventDefault(); navigateHitsInternal(1); }
            else if (e.key === 'ArrowUp'   && queryHits.length > 0) { e.preventDefault(); navigateHitsInternal(-1); }
            else if (e.key === 'Escape' && query)                   { e.preventDefault(); clearQueryInternal(); }
          }}
          autocomplete="off"
          spellcheck="false"
        />
        <!-- Highlight + ghost overlay. The input above is rendered
             with `color: transparent` so the colored tokens here are
             what the user actually sees; selection + caret stay
             visible via the input's caret-color and ::selection rule.
             Inner div carries the scroll-sync transform so long
             queries track the caret. `white-space: pre` preserves
             spaces exactly. -->
        <div class="sqb-hl-overlay" aria-hidden="true">
          <div class="sqb-hl-inner" style="transform: translateX({-inputScrollLeft}px)">
            {#each highlightTokens as tok, i (i + ':' + tok.text)}<span class="sqb-tok sqb-tok-{tok.type}" style={tok.type === 'field' ? `color:${fieldColor(tok.depth ?? 0)}` : ''}>{tok.text}</span>{/each}{#if ghostSuffix}<span class="sqb-ghost-suffix">{ghostSuffix}</span>{/if}
          </div>
        </div>
      </div>
      {#if ghostSuffix}
        <kbd class="sqb-tab-hint" use:tooltip={'Complete with ' + (query + ghostSuffix)}>Tab</kbd>
      {/if}
      {#if querying}
        <Loader2 size={12} class="sqb-spinner" />
      {:else if query}
        <button class="sqb-clear" onclick={clearQueryInternal} use:tooltip={'Clear query (Esc)'} aria-label="Clear query">×</button>
      {/if}
      {#if queryHits.length > 0}
        <!-- Inline reference to the dedicated sidebar.  Clicking opens
             (or closes) the Query Results sidebar; doubles as a status
             pill showing the live hit count without taking the user
             away from the input. -->
        <button
          type="button"
          class="sqb-sidebar-ref"
          class:sqb-sidebar-ref-active={rightPaneOpen}
          onclick={onToggleRightPane}
          use:tooltip={rightPaneOpen
            ? 'Hide query results sidebar'
            : 'Open query results sidebar'}
        >
          <span>{queryHits.length}{queryHits.length >= 500 ? '+' : ''}</span>
          <ArrowUpRight size={11} />
        </button>
      {/if}
    </div>

    <!-- Toolbar: History on the left, hit-count + nav in the middle
         (only when there are results), parent-injected extras on the
         right (Expand/Collapse for RON). -->
    <div class="sqb-toolbar">
      <Dropdown items={historyItems} width="420px" position="fixed">
        {#snippet trigger({ toggle })}
          <button
            type="button"
            class="sqb-tool-btn"
            onclick={toggle}
            use:tooltip={'Recent queries'}
            aria-label="Recent queries"
          >
            <History size={12} />
            <span>History</span>
            <ChevronDown size={10} />
          </button>
        {/snippet}
      </Dropdown>

      {#if hasResults}
        <span class="sqb-toolbar-sep"></span>
        <span class="sqb-count">
          <span class="sqb-count-current">{currentHitIdx + 1}</span>
          <span class="sqb-count-sep">/</span>
          <span>{queryHits.length}</span>
          <span class="sqb-count-label">hit{queryHits.length === 1 ? '' : 's'}{queryHits.length >= 500 ? ' (capped)' : ''}</span>
        </span>
        <div class="sqb-nav">
          <button
            type="button"
            class="sqb-nav-btn"
            onclick={() => navigateHitsInternal(-1)}
            use:tooltip={{ content: 'Previous hit', shortcut: 'Shift+F3' }}
            aria-label="Previous hit"
          ><ArrowUp size={11} /></button>
          <button
            type="button"
            class="sqb-nav-btn"
            onclick={() => navigateHitsInternal(1)}
            use:tooltip={{ content: 'Next hit', shortcut: 'F3' }}
            aria-label="Next hit"
          ><ArrowDown size={11} /></button>
        </div>
      {/if}

      {#if bulkEditEnabled}
        <span class="sqb-toolbar-sep"></span>
        <button
          type="button"
          class="sqb-tool-btn sqb-bulk-edit-btn"
          onclick={() => onBulkEditRequest?.(query.trim())}
          disabled={queryHits.length === 0}
          use:tooltip={queryHits.length === 0
            ? 'Run a query that matches at least one node to enable bulk edit'
            : `Bulk edit ${queryHits.length} ${queryHits.length === 1 ? 'match' : 'matches'} (Set / Delete with mini-expr)`}
          aria-label="Bulk edit hits"
        >
          <PencilRuler size={12} />
          <span>Transform</span>
        </button>
      {/if}

      <span class="sqb-tool-spacer"></span>

      {#if toolbarRight}
        {@render toolbarRight()}
      {/if}
    </div>

    {#if queryError}
      <div class="sqb-error">
        <AlertCircle size={11} /> {queryError}
      </div>
    {:else if hasResults && !rightPaneOpen}
      <!-- Inline hit list — hidden when the dedicated Query Results
           sidebar is open so the same data isn't shown twice. The
           inline "↗ N" pill in the toolbar above carries the count +
           jumps to the sidebar when it's closed. -->
      <div class="sqb-list" role="listbox" aria-label="Query results">
        {#each queryHits as hit, i (hit.path.join('\x00'))}
          <div
            class="sqb-hit"
            class:active={i === currentHitIdx}
            role="option"
            aria-selected={i === currentHitIdx}
            tabindex="0"
            onclick={() => { currentHitIdx = i; onJumpToHit(hit.path); }}
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                currentHitIdx = i;
                onJumpToHit(hit.path);
              }
            }}
          >
            {@render kindChip(hit.kind)}
            <span class="sqb-hit-path">{#each tokenizeHitPath(hit.path) as tok, j (j + ':' + tok.text)}<span class="sqb-tok sqb-tok-{tok.type}" style={tok.type === 'field' ? `color:${fieldColor(tok.depth ?? 0)}` : ''}>{tok.text}</span>{/each}</span>
            {#if hit.variant_tag}
              <span class="sqb-hit-tag">{hit.variant_tag}</span>
            {/if}
            {#if hit.preview}
              <span class="sqb-hit-sep">·</span>
              {#if previewRender}
                {@render previewRender(hit.preview)}
              {:else}
                <span class="sqb-hit-preview">{hit.preview}</span>
              {/if}
            {/if}
            <span class="sqb-hit-spacer"></span>
            <button
              type="button"
              class="sqb-hit-action"
              onclick={(e) => { e.stopPropagation(); void copyHitPath(hit); }}
              use:tooltip={'Copy path'}
              aria-label="Copy path"
            ><LinkIcon size={10} /></button>
            <ArrowRight size={11} class="sqb-hit-arrow" />
          </div>
        {/each}
      </div>
    {:else if queryActive && !querying}
      <div class="sqb-empty">No matches.</div>
    {/if}
  </div>
{/if}

<style>
  .sqb-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border);
  }
  .sqb-row {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 8px;
    transition: border-color var(--transition-fast);
  }
  .sqb-row:focus-within {
    border-color: var(--accent);
  }
  .sqb-ghost-wrap { position: relative; flex: 1; min-width: 0; display: flex; align-items: center; }
  .sqb-input {
    width: 100%;
    background: transparent;
    border: none;
    /* Text itself is hidden — the colored tokens in
       `.sqb-hl-overlay` are what the user reads. Caret + selection
       remain visible via `caret-color` and the `::selection` rule
       below, so editing feels identical to a normal input. */
    color: transparent;
    caret-color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    /* Explicit line-height so the overlay can match it exactly.
       Without this the input falls back to the UA default
       (~normal) and the tokens (which inherit from the wrap) land
       a couple of pixels above and at a slightly different size. */
    line-height: 18px;
    outline: none;
    padding: 0;
    position: relative;
    z-index: 1;
  }
  .sqb-input::placeholder { color: var(--text-disabled); }
  .sqb-input::selection {
    color: var(--text-primary);
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  /* Highlight + ghost overlay sits BEHIND the input. Flex+baseline
     keeps the tokens on the same row as the caret regardless of the
     row's vertical chrome. Font properties mirror `.sqb-input`
     exactly so token widths match the underlying text to the pixel.
     `overflow: hidden` clips when the typed query exceeds the row;
     the inner `translateX(-scrollLeft)` keeps it glued to the caret. */
  .sqb-hl-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
    display: flex;
    align-items: center;
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 18px;
    white-space: pre;
    z-index: 0;
  }
  .sqb-hl-inner { white-space: pre; }
  .sqb-ghost-suffix { color: var(--text-disabled); opacity: 0.7; }

  /* Token palette — leans on the existing `--syntax-*` design
     tokens (same ones PrismJS uses) so it tracks theme changes.
     Hard fallbacks match the JetBrains-style defaults declared in
     `app.css`. */
  .sqb-tok { font-family: inherit; font-size: inherit; line-height: inherit; }
  /* Field tokens get their color inline via `fieldColor(depth)` so
     successive `.a.b.c` segments rotate through a theme-independent
     palette. The other token classes use hard fallback hex values
     because the project supports custom themes that may not
     declare the `--syntax-*` design tokens. */
  .sqb-tok-root     { color: #d19a66; font-weight: 600; }
  .sqb-tok-recurse  { color: #c678dd; font-weight: 600; }
  .sqb-tok-dot      { color: var(--text-muted, #7a7d85); }
  .sqb-tok-bracket  { color: var(--text-muted, #7a7d85); }
  .sqb-tok-index    { color: #d19a66; }
  .sqb-tok-slice    { color: #c678dd; }
  .sqb-tok-filter   { color: #c678dd; font-weight: 600; }
  .sqb-tok-current  { color: #e5c07b; }
  .sqb-tok-wildcard { color: #e06c75; font-weight: 600; }
  .sqb-tok-operator { color: #56b6c2; }
  .sqb-tok-string   { color: #98c379; }
  .sqb-tok-plain    { color: var(--text-primary); }
  .sqb-tab-hint {
    font-family: var(--font-code);
    font-size: 9.5px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--bg-base);
    color: var(--text-secondary);
    border: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sqb-clear {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 16px;
    line-height: 1;
    padding: 0 2px;
    flex-shrink: 0;
    transition: color var(--transition-fast), background var(--transition-fast);
    border-radius: 4px;
  }
  .sqb-clear:hover { color: var(--text-primary); background: var(--bg-base); }
  .sqb-spinner {
    flex-shrink: 0;
    color: var(--accent);
    animation: sqb-spin 1s linear infinite;
  }
  @keyframes sqb-spin { to { transform: rotate(360deg); } }

  .sqb-sidebar-ref {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-family: var(--font-code);
    font-size: 10.5px;
    padding: 1px 6px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-base));
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    cursor: pointer;
    transition: background var(--transition-fast);
    flex-shrink: 0;
  }
  .sqb-sidebar-ref:hover,
  .sqb-sidebar-ref-active {
    background: color-mix(in srgb, var(--accent) 22%, var(--bg-base));
  }

  .sqb-toolbar { display: flex; align-items: center; gap: 4px; }
  .sqb-toolbar-sep {
    width: 1px;
    height: 14px;
    background: var(--border);
    margin: 0 4px;
  }
  .sqb-tool-spacer { flex: 1; }
  .sqb-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 11px;
    padding: 3px 7px;
    border-radius: 4px;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .sqb-tool-btn:hover {
    color: var(--text-primary);
    background: var(--bg-overlay);
  }
  .sqb-tool-btn:disabled {
    color: var(--text-disabled);
    cursor: not-allowed;
    background: transparent;
  }
  .sqb-bulk-edit-btn :global(svg) {
    color: var(--accent);
  }
  .sqb-bulk-edit-btn:disabled :global(svg) {
    color: var(--text-disabled);
  }
  .sqb-count {
    display: inline-flex;
    align-items: baseline;
    gap: 2px;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
  }
  .sqb-count-current { font-weight: 600; color: var(--accent); font-family: var(--font-code); }
  .sqb-count-sep { color: var(--text-disabled); }
  .sqb-count-label { margin-left: 4px; }
  .sqb-nav { display: inline-flex; gap: 2px; }
  .sqb-nav-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 3px 5px;
    border-radius: 4px;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .sqb-nav-btn:hover {
    color: var(--text-primary);
    background: var(--bg-overlay);
  }

  .sqb-error {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--error, #e06c75);
    background: color-mix(in srgb, var(--error, #e06c75) 12%, transparent);
    padding: 4px 8px;
    border-radius: 4px;
  }
  .sqb-empty {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
    padding: 2px 4px;
  }

  .sqb-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-overlay);
    padding: 4px;
  }
  .sqb-hit {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    cursor: pointer;
    font-family: var(--font-code);
    font-size: 11px;
    min-width: 0;
    outline: none;
  }
  .sqb-hit > .sqb-hit-sep,
  .sqb-hit > .sqb-hit-action,
  .sqb-hit > :global(.sqb-hit-arrow) { flex-shrink: 0; }
  .sqb-hit-path    { flex: 0 1 auto; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sqb-hit-tag     {
    font-family: var(--font-code);
    font-size: 10px;
    padding: 0 4px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    flex-shrink: 0;
  }
  .sqb-hit-sep     { color: var(--text-disabled); }
  .sqb-hit-preview { color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 0 1 auto; min-width: 0; }
  .sqb-hit-spacer  { flex: 1 1 6px; min-width: 6px; }
  .sqb-hit:hover { background: color-mix(in srgb, var(--text-primary) 4%, transparent); }
  .sqb-hit:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  .sqb-hit.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .sqb-hit.active :global(.sqb-hit-arrow) { color: var(--accent); }
  .sqb-hit-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    padding: 2px 4px;
    border-radius: 3px;
    opacity: 0;
    transition: opacity var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  }
  .sqb-hit:hover .sqb-hit-action,
  .sqb-hit.active .sqb-hit-action { opacity: 1; }
  .sqb-hit-action:hover { background: var(--bg-base); color: var(--text-primary); }
</style>
