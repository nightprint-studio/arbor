<script lang="ts">
  /**
   * FindWidget — the editor's find-and-replace bar, drawn by Arbor and driven by CodeMirror.
   *
   * CodeMirror owns the *search state* (the query, the match highlighting, `findNext`,
   * `replaceAll`); this owns only the surface, which replaces the stock panel — a row of unstyled
   * form controls that no amount of theming made look like part of the app. It is mounted into a
   * CodeMirror panel by `find-panel.ts`, which calls {@link refresh} on every editor update so the
   * counter tracks edits and caret moves.
   *
   * ## One row, two when replacing
   *
   * The options (case, whole word, regex) and the match counter live *inside* the find field, the
   * way IntelliJ's do, so the common case — find — is a single slim row. The replace row is folded
   * until asked for (`Alt+P` or the chevron).
   *
   * ## Typing is incremental
   *
   * Every change re-runs the query and selects the first match at or after the caret, so the
   * editor scrolls to the answer as you type instead of waiting for Enter.
   *
   * ## Keyboard
   *
   * Enter / Shift+Enter next / previous · Esc closes and returns focus to the code · Alt+C / Alt+W /
   * Alt+X toggle case / word / regex · Alt+P toggles replace · in the replace field Enter replaces
   * the current match and Ctrl+Enter replaces all.
   */
  import { ChevronRight, ChevronUp, ChevronDown, Search, X, Replace, ReplaceAll } from 'lucide-svelte';
  import {
    SearchQuery, setSearchQuery, getSearchQuery, findNext, findPrevious, replaceNext, replaceAll,
    closeSearchPanel,
  } from '@codemirror/search';
  import { EditorSelection } from '@codemirror/state';
  import type { EditorView } from '@codemirror/view';
  import Input from '../Input.svelte';
  import IconButton from '../IconButton.svelte';
  import ToggleButton from '../ToggleButton.svelte';

  /** The most matches counted — a document of a million `e`s must not stall the editor on each
   *  keystroke. Past it the counter reads `2000+`. */
  const COUNT_CAP = 2000;

  let {
    view,
    onFindInFiles,
  }: {
    view: EditorView;
    /** Hand the current query to a project-wide search. The button is absent without it. */
    onFindInFiles?: (query: string) => void;
  } = $props();

  // Seeded once from the query CodeMirror already holds: opening the panel with a selection has
  // already put the selected text there.
  // svelte-ignore state_referenced_locally
  const initial = getSearchQuery(view.state);
  let search = $state(initial.search);
  let replace = $state(initial.replace);
  let caseSensitive = $state(initial.caseSensitive);
  let wholeWord = $state(initial.wholeWord);
  let regexp = $state(initial.regexp);
  let replaceOpen = $state(false);

  let total = $state(0);
  let capped = $state(false);
  let index = $state(0);
  let invalid = $state(false);

  let root = $state<HTMLDivElement | undefined>();
  let findEl = $state<HTMLInputElement | undefined>();
  let replaceEl = $state<HTMLInputElement | undefined>();

  function queryOf(): SearchQuery {
    return new SearchQuery({ search, replace, caseSensitive, wholeWord, regexp });
  }

  /** Push the fields into CodeMirror's search state. Skipped when nothing differs, because the
   *  `refresh` that follows every dispatch would otherwise loop back into this. */
  function commit() {
    const q = queryOf();
    if (!q.eq(getSearchQuery(view.state))) view.dispatch({ effects: setSearchQuery.of(q) });
  }

  /** Select the first match at or after the caret (wrapping), so typing scrolls to the answer. */
  function jumpToFirst() {
    const q = getSearchQuery(view.state);
    if (!q.valid || !q.search) return;
    const from = view.state.selection.main.from;
    let hit = firstMatch(q, from, view.state.doc.length) ?? firstMatch(q, 0, from);
    if (!hit) return;
    view.dispatch({
      selection: EditorSelection.single(hit.from, hit.to),
      scrollIntoView: true,
    });
  }

  function firstMatch(q: SearchQuery, from: number, to: number) {
    const r = q.getCursor(view.state, from, to).next();
    return r.done ? null : r.value;
  }

  function edited() {
    commit();
    jumpToFirst();
    refresh();
  }

  /** Re-read the match count and the caret's position among them. Called by the panel on every
   *  editor update — and by this component after its own edits. */
  export function refresh() {
    const q = getSearchQuery(view.state);
    // The query changed from outside — `Ctrl+F` on a new selection while the bar is already open.
    // Adopt it, unless the user is typing here, in which case the fields are the truth.
    if (!root?.contains(document.activeElement) && !q.eq(queryOf())) {
      search = q.search; replace = q.replace;
      caseSensitive = q.caseSensitive; wholeWord = q.wholeWord; regexp = q.regexp;
      focusFind();
    }
    invalid = !!q.search && !q.valid;
    if (!q.search || !q.valid) { total = 0; index = 0; capped = false; return; }
    const sel = view.state.selection.main;
    let n = 0;
    let at = 0;
    const cur = q.getCursor(view.state);
    for (let r = cur.next(); !r.done; r = cur.next()) {
      n++;
      if (!at && r.value.from === sel.from && r.value.to === sel.to) at = n;
      if (n >= COUNT_CAP) break;
    }
    total = n;
    capped = n >= COUNT_CAP;
    index = at;
  }

  export function focusFind() {
    findEl?.focus();
    findEl?.select();
  }

  function close() {
    closeSearchPanel(view);
    view.focus();
  }

  function toggleReplace() {
    replaceOpen = !replaceOpen;
    if (replaceOpen) queueMicrotask(() => replaceEl?.focus());
  }

  function setOption(which: 'case' | 'word' | 're') {
    if (which === 'case') caseSensitive = !caseSensitive;
    else if (which === 'word') wholeWord = !wholeWord;
    else regexp = !regexp;
    edited();
  }

  function step(back: boolean) {
    commit();
    (back ? findPrevious : findNext)(view);
    refresh();
  }

  /** Keys shared by both fields. Returns true when it handled the event. */
  function commonKeys(e: KeyboardEvent): boolean {
    if (e.key === 'Escape') { e.preventDefault(); close(); return true; }
    if (e.altKey && !e.ctrlKey && !e.metaKey) {
      const k = e.key.toLowerCase();
      if (k === 'c') { e.preventDefault(); setOption('case'); return true; }
      if (k === 'w') { e.preventDefault(); setOption('word'); return true; }
      if (k === 'x') { e.preventDefault(); setOption('re'); return true; }
      // `p` for re**p**lace: `Ctrl+H` is the type hierarchy, `Ctrl+R` the recent-repos switcher and
      // `Ctrl+Shift+R` the screen recorder, and a chord that fires two things is worse than an
      // unfamiliar one.
      if (k === 'p') { e.preventDefault(); toggleReplace(); return true; }
    }
    return false;
  }

  function onFindKey(e: KeyboardEvent) {
    if (commonKeys(e)) return;
    if (e.key === 'Enter') { e.preventDefault(); step(e.shiftKey); }
  }

  function onReplaceKey(e: KeyboardEvent) {
    if (commonKeys(e)) return;
    if (e.key !== 'Enter') return;
    e.preventDefault();
    commit();
    if (e.ctrlKey || e.metaKey) replaceAll(view); else replaceNext(view);
    refresh();
  }

  const counter = $derived(
    invalid ? 'Invalid' : !search ? '' : total === 0 ? 'No results'
      : `${index || '–'}/${total}${capped ? '+' : ''}`,
  );
</script>

<div class="fw" role="search" aria-label="Find and replace" bind:this={root}>
  <div class="fw-row">
    <IconButton
      tooltip={replaceOpen ? 'Hide replace' : 'Show replace'}
      shortcut="Alt+P"
      size={20}
      ariaExpanded={replaceOpen}
      onclick={toggleReplace}
    >
      <span class="fw-chev" class:open={replaceOpen}><ChevronRight size={12} /></span>
    </IconButton>

    <div class="fw-field">
      <Input
        bind:element={findEl}
        value={search}
        size="sm"
        placeholder="Find"
        ariaLabel="Find"
        error={invalid ? 'Not a valid regular expression' : null}
        oninput={(v) => { search = v; edited(); }}
        onkeydown={onFindKey}
      >
        {#snippet iconEnd()}
          <span class="fw-opts">
            {#if counter}
              <span class="fw-count" class:none={total === 0 || invalid}>{counter}</span>
            {/if}
            <ToggleButton compact pressed={caseSensitive} label="Aa" title="Match case" shortcut="Alt+C" onclick={() => setOption('case')} />
            <ToggleButton compact pressed={wholeWord} label="ab" title="Whole word" shortcut="Alt+W" onclick={() => setOption('word')} />
            <ToggleButton compact pressed={regexp} label=".*" title="Regular expression" shortcut="Alt+X" onclick={() => setOption('re')} />
          </span>
        {/snippet}
      </Input>
    </div>

    <IconButton tooltip="Previous match" shortcut="Shift+Enter" size={26} onclick={() => step(true)}>
      <ChevronUp size={14} />
    </IconButton>
    <IconButton tooltip="Next match" shortcut="Enter" size={26} onclick={() => step(false)}>
      <ChevronDown size={14} />
    </IconButton>
    {#if onFindInFiles}
      <IconButton
        tooltip="Find in files"
        shortcut="Ctrl+Shift+F"
        size={26}
        onclick={() => { const q = search; close(); onFindInFiles(q); }}
      >
        <Search size={13} />
      </IconButton>
    {/if}
    <IconButton tooltip="Close" shortcut="Esc" size={26} onclick={close}><X size={13} /></IconButton>
  </div>

  {#if replaceOpen}
    <div class="fw-row">
      <span class="fw-indent"></span>
      <div class="fw-field">
        <Input
          bind:element={replaceEl}
          value={replace}
          size="sm"
          placeholder="Replace"
          ariaLabel="Replace with"
          oninput={(v) => { replace = v; commit(); }}
          onkeydown={onReplaceKey}
        />
      </div>
      <IconButton
        tooltip="Replace"
        shortcut="Enter"
        size={26}
        onclick={() => { commit(); replaceNext(view); refresh(); }}
      >
        <Replace size={14} />
      </IconButton>
      <IconButton
        tooltip="Replace all"
        shortcut="Ctrl+Enter"
        size={26}
        variant="accent"
        onclick={() => { commit(); replaceAll(view); refresh(); }}
      >
        <ReplaceAll size={14} />
      </IconButton>
    </div>
  {/if}
</div>

<style>
  .fw {
    display: flex; flex-direction: column; gap: 3px;
    width: 392px; box-sizing: border-box;
    padding: 4px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 6px 22px rgba(0, 0, 0, 0.38);
    font-family: var(--font-ui-sans);
  }
  .fw-row { display: flex; align-items: center; gap: 2px; }
  .fw-field { flex: 1; min-width: 0; }
  .fw-indent { width: 20px; flex-shrink: 0; }
  .fw-chev { display: inline-flex; transition: transform var(--transition-fast); }
  .fw-chev.open { transform: rotate(90deg); }
  .fw-opts { display: inline-flex; align-items: center; gap: 1px; }
  .fw-count {
    padding: 0 5px; font-size: var(--font-size-2xs); color: var(--text-muted);
    white-space: nowrap; font-variant-numeric: tabular-nums;
  }
  .fw-count.none { color: var(--danger); }
</style>
