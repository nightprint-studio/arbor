<script lang="ts">
  /**
   * GroveEditor — the CodeMirror 6 host for one `.grove` buffer. Format-specific
   * sibling of the shared `StudioTextPane`, but grove-coupled: it wires the live
   * engine stores (diagnostics → lint, active-haps → highlight) and Tree-sitter
   * go-to-declaration directly, so the surrounding TabbedEditor stays thin.
   *
   * Controlled `value`: external writes (tab switch, cross-file open) flow in via
   * the prop; internal edits flow out via `oninput` (the parent debounces the
   * re-eval). Imperative API (focus / scrollToLineCol / scrollToOffset /
   * gotoSymbol / getValue) is exposed via `bind:this`.
   */
  import { onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { setDiagnostics } from '@codemirror/lint';
  import { openSearchPanel } from '@codemirror/search';

  import { createGroveExtensions, setActiveHaps, toActiveHapMarks, toCmDiagnostics, getGroveTree }
    from './grove-cm';
  import type { GroveIntelSource } from './grove-intel';
  import { extractSymbols } from './grove-lang';
  import { buildControlEdits, type ControlEdit } from './grove-edit';
  import { diagnosticsStore, activeHapsStore } from '../stores/engine.svelte';
  import { referenceStore } from '../stores/reference.svelte';
  import { soundsStore } from '../stores/sounds.svelte';

  // Autocomplete + hover read the DSL catalogue live from the store (snapshotted
  // at call time — the store loads asynchronously, so completions light up once
  // it lands without re-mounting the editor). Instruments come from the live
  // sound registry (for `inst("…")` value completion).
  const intel: GroveIntelSource = {
    entries: () => referenceStore.entries,
    byName: (name) => referenceStore.byName(name),
    instruments: () => soundsStore.instruments,
  };

  let {
    value,
    readOnly = false,
    oninput,
    onfocus,
    /** Live caret position (1-based line/col) — drives the footer Ln/Col. */
    oncaret,
    /** The word resolved to an imported binding — host opens the source file. */
    onCrossFileGoto,
  }: {
    value: string;
    readOnly?: boolean;
    oninput?: (text: string) => void;
    onfocus?: () => void;
    oncaret?: (line: number, col: number) => void;
    onCrossFileGoto?: (word: string, importPath: string) => void;
  } = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let suppressEmit = false;
  let lastEmitted: string | null = null;

  // ── Go-to-declaration (Ctrl/Cmd+Click) ──────────────────────────────────────
  function handleGoto(word: string, v: EditorView) {
    const tree = getGroveTree(v);
    if (!tree) return;
    const { defs, imports } = extractSymbols(tree);
    const local = defs.get(word);
    if (local) { scrollToOffset(local.offset, true); return; }
    const importPath = imports.get(word);
    if (importPath) onCrossFileGoto?.(word, importPath);
  }

  function mount(target: HTMLDivElement) {
    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged && !suppressEmit) {
        const text = u.state.doc.toString();
        lastEmitted = text;
        oninput?.(text);
      }
      if (u.focusChanged && u.view.hasFocus) onfocus?.();
      if (oncaret && (u.selectionSet || u.docChanged)) {
        const head = u.state.selection.main.head;
        const line = u.state.doc.lineAt(head);
        oncaret(line.number, head - line.from + 1);
      }
    });

    const state = EditorState.create({
      doc: value,
      extensions: [
        createGroveExtensions({ readOnly, onGoto: handleGoto, intel }),
        updateListener,
      ],
    });
    view = new EditorView({ state, parent: target });
    pushDiagnostics();
    pushActiveHaps();
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });
  onDestroy(() => { view?.destroy(); view = undefined; });

  // ── value (controlled) → editor ─────────────────────────────────────────────
  $effect(() => {
    const next = value;
    if (!view) return;
    if (next === lastEmitted) return;
    const current = view.state.doc.toString();
    if (current === next) return;
    suppressEmit = true;
    try {
      view.dispatch({ changes: { from: 0, to: current.length, insert: next } });
    } finally { suppressEmit = false; }
  });

  // ── diagnostics store → lint markers ────────────────────────────────────────
  function pushDiagnostics() {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch(setDiagnostics(view.state, toCmDiagnostics(diagnosticsStore.errors, src)));
  }
  $effect(() => { void diagnosticsStore.errors; pushDiagnostics(); });

  // ── active-haps store → live highlight ──────────────────────────────────────
  function pushActiveHaps() {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch({ effects: setActiveHaps.of(toActiveHapMarks(activeHapsStore.haps, src)) });
  }
  $effect(() => { void activeHapsStore.haps; pushActiveHaps(); });

  // ── Imperative API ──────────────────────────────────────────────────────────
  export function focus() { view?.focus(); }

  /** Open CodeMirror's search panel and focus its query field (routed here from
   *  the GroveShell's Ctrl+F when the editor pane has focus). */
  export function openSearch() {
    if (!view) return;
    openSearchPanel(view);
  }

  export function scrollToOffset(offset: number, select = false) {
    if (!view) return;
    const len = view.state.doc.length;
    const pos = Math.max(0, Math.min(offset, len));
    view.dispatch({
      selection: select ? { anchor: pos, head: pos } : { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    view.focus();
  }

  export function scrollToLineCol(line: number, col = 1) {
    if (!view) return;
    const doc = view.state.doc;
    const ln = Math.max(1, Math.min(line, doc.lines));
    const lineInfo = doc.line(ln);
    const pos = Math.min(lineInfo.from + Math.max(0, col - 1), lineInfo.to);
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    view.focus();
  }

  /** Jump to a local declaration by name (used after a cross-file open). Returns
   *  false when the tree isn't ready yet or the symbol isn't a local decl. */
  export function gotoSymbol(name: string): boolean {
    if (!view) return false;
    const tree = getGroveTree(view);
    if (!tree) return false;
    const sym = extractSymbols(tree).defs.get(name);
    if (!sym) return false;
    scrollToOffset(sym.offset, true);
    return true;
  }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }

  /** Commit mixer/inspector control values into the source as literals (one
   *  undoable transaction; the resulting edit re-evals via `oninput`). Resolves
   *  spans against the live tree — returns `treeReady: false` when the grammar
   *  hasn't loaded yet (the caller retries), and lists any controls skipped
   *  because their argument is calculated (not a literal). */
  export function commitControls(
    index: number,
    edits: ControlEdit[],
  ): { treeReady: boolean; applied: number; skipped: string[] } {
    if (!view) return { treeReady: false, applied: 0, skipped: [] };
    const tree = getGroveTree(view);
    if (!tree) return { treeReady: false, applied: 0, skipped: [] };
    const src = view.state.doc.toString();
    const { changes, skipped } = buildControlEdits(tree, src, index, edits);
    if (changes.length) view.dispatch({ changes });
    return { treeReady: true, applied: changes.length, skipped };
  }
</script>

<div class="grv-editor" bind:this={hostEl}></div>

<style>
  .grv-editor {
    flex: 1;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .grv-editor :global(.cm-editor) { height: 100%; }
</style>
