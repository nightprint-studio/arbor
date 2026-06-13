<script lang="ts">
  /**
   * NemusEditor — the CodeMirror 6 host for one `.nemus` buffer. Format-specific
   * sibling of the shared `StudioTextPane`, but nemus-coupled: it wires the live
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

  import { createNemusExtensions, setActiveHaps, toActiveHapMarks, toCmDiagnostics, getNemusTree }
    from './nemus-cm';
  import { nemusFormat } from '$lib/ipc/nemus';
  import type { NemusIntelSource } from './nemus-intel';
  import { extractSymbols, identifierAt, identifierUsages, tracksReferencing, stringArgCallAt, declBodyRangeForSelection, withFileDeps, type NemusSymbol } from './nemus-lang';
  import { symbolHighlightStore } from '../stores/symbol-highlight.svelte';
  import { editorSelectionStore } from '../stores/editor-selection.svelte';
  import { buildControlEdits, type ControlEdit, type EditChange } from './nemus-edit';
  import { renamePlan, extractTarget, extractLetPlan, inlinePlan, freshName } from './nemus-refactor';
  import { collectIntentions, changeScalePlan, type IntentionItem } from './nemus-intentions';
  import { scalesStore } from '../stores/scales.svelte';
  import type { UsageItem, UsageAnchor } from '../stores/usages.svelte';
  import { diagnosticsStore, activeHapsStore, nemusEngine } from '../stores/engine.svelte';
  import { referenceStore } from '../stores/reference.svelte';
  import { soundsStore } from '../stores/sounds.svelte';
  import { previewStore } from '../stores/preview.svelte';
  import { scratchStore } from '../stores/scratch.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { Play, FlaskConical } from 'lucide-svelte';

  // Autocomplete + hover read the DSL catalogue live from the store (snapshotted
  // at call time — the store loads asynchronously, so completions light up once
  // it lands without re-mounting the editor). Instruments come from the live
  // sound registry (for `inst("…")` value completion).
  const intel: NemusIntelSource = {
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

  // ── Symbol-under-caret → arrangement lane highlight ──────────────────────────
  // The occurrence plugin reports the identifier at the caret; resolve which
  // arrangement tracks reference it (from the live tree) and publish for the DAW.
  function handleSymbol(name: string | null, v: EditorView) {
    if (!name) { symbolHighlightStore.clear(); return; }
    const tree = getNemusTree(v);
    symbolHighlightStore.set(name, tree ? tracksReferencing(tree, name) : []);
  }

  // ── Selection → arrangement region highlight ─────────────────────────────────
  // Publish the source regions the selection maps to; the DAW boxes the haps whose
  // span overlaps any of them. Selecting a variable / function name resolves to its
  // declaration body range too, so every note it produces lights up (their spans
  // point into the definition, not the use site). Cycles (`<…>`, cat, every, …) are
  // inline, so their literals already fall inside the selection.
  function handleSelection(range: { from: number; to: number } | null) {
    if (!range) { editorSelectionStore.set([]); return; }
    const ranges = [range];
    const tree = view ? getNemusTree(view) : null;
    if (tree) {
      const declRange = declBodyRangeForSelection(tree, range.from, range.to);
      if (declRange) ranges.push(declRange);
    }
    editorSelectionStore.set(ranges);
  }

  // ── Right-click → play / send the selection ──────────────────────────────────
  let ctx = $state<{ x: number; y: number; text: string } | null>(null);
  function onContextMenu(e: MouseEvent) {
    if (!view) return;
    const sel = view.state.selection.main;
    if (sel.empty) return; // no selection → leave the native menu
    e.preventDefault();
    ctx = { x: e.clientX, y: e.clientY, text: view.state.doc.sliceString(sel.from, sel.to) };
  }
  const ctxItems: MenuItem[] = [
    { id: 'play',    label: 'Play selection',     icon: Play },
    { id: 'scratch', label: 'Send to Scratch',    icon: FlaskConical },
  ];
  async function onCtxSelect(id: string) {
    const text = ctx?.text;
    const src = view?.state.doc.toString() ?? '';
    ctx = null;
    if (!text) return;
    // Resolve the selection against the file's preamble so a bare variable (or any
    // expression using file-level bindings) actually plays, not silently nothing.
    if (id === 'play') void nemusEngine.playSnippet(await withFileDeps(src, text), projectStore.project?.path);
    else if (id === 'scratch') { scratchStore.load(text); nemusStore.showBottom('scratch'); }
  }

  // ── Go-to-declaration (Ctrl/Cmd+Click) ──────────────────────────────────────
  function handleGoto(word: string, v: EditorView) {
    const tree = getNemusTree(v);
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
        createNemusExtensions({
          readOnly,
          onGoto: handleGoto,
          onPreview: (name) => previewStore.showByName(name),
          onSymbol: handleSymbol,
          onSelection: handleSelection,
          intel,
        }),
        updateListener,
      ],
    });
    view = new EditorView({ state, parent: target });
    pushDiagnostics();
    pushActiveHaps();
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });
  onDestroy(() => {
    view?.destroy();
    view = undefined;
    symbolHighlightStore.clear(); // drop the arrangement lane highlight with the editor
    editorSelectionStore.clear(); // drop the selection→region boxes with the editor
  });

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
   *  the NemusShell's Ctrl+F when the editor pane has focus). */
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
    const tree = getNemusTree(view);
    if (!tree) return false;
    const sym = extractSymbols(tree).defs.get(name);
    if (!sym) return false;
    scrollToOffset(sym.offset, true);
    return true;
  }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }

  /** Reformat the buffer through the canonical pretty-printer (backend `emit`).
   *  The whole-document rewrite is one undoable transaction that flows out via
   *  `oninput` (so the debounced re-eval runs); the caret is re-anchored to the
   *  start of its former line (formatting reflows columns). Resolves `{ ok:false,
   *  error }` when the source has a syntax error — the buffer is left untouched so
   *  no content is lost (the lint markers already point at the offending span). */
  export async function formatDocument(): Promise<{ ok: boolean; error?: string }> {
    if (!view) return { ok: false };
    const src = view.state.doc.toString();
    let formatted: string;
    try {
      formatted = await nemusFormat(src);
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
    if (!view) return { ok: true };            // editor torn down mid-await
    if (formatted === src) return { ok: true }; // already canonical
    const lineNo = view.state.doc.lineAt(view.state.selection.main.head).number;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: formatted } });
    const doc = view.state.doc;
    const pos = doc.line(Math.min(lineNo, doc.lines)).from;
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    view.focus();
    return { ok: true };
  }

  /** Find every usage of the identifier under the caret (declaration + refs),
   *  resolved against the live tree. Always returns (when the editor exists) so
   *  the popover can give feedback: `name` is null when the caret isn't on a name
   *  (the popover then shows a hint). Returns null only when there's no view at
   *  all. `anchor` is the caret's viewport coords (to position the popover). */
  export function findUsages(): { name: string | null; items: UsageItem[]; anchor: UsageAnchor | null } | null {
    if (!view) return null;
    const head = view.state.selection.main.head;
    const c = view.coordsAtPos(head);
    const anchor: UsageAnchor | null = c ? { x: c.left, y: c.bottom } : null;
    const tree = getNemusTree(view);
    // Boundary-tolerant: the caret often sits at the RIGHT edge of a word (after a
    // double-click selection or typing the name), where `descendantForIndex(head)`
    // returns the next token. Fall back to the char just before the caret so
    // `bass|` resolves to `bass`.
    const name = tree
      ? (identifierAt(tree, head) ?? (head > 0 ? identifierAt(tree, head - 1) : null))
      : null;
    if (!tree || !name) return { name: null, items: [], anchor };
    const doc = view.state.doc;
    const items: UsageItem[] = identifierUsages(tree, name).map((r) => {
      const line = doc.lineAt(r.from);
      return { offset: r.from, line: line.number, col: r.from - line.from + 1, preview: line.text.trim() };
    });
    return { name, items, anchor };
  }

  /** The active file's symbol table (tracks · fn · let · import) in source order,
   *  for the file-structure picker (Ctrl+F12). Empty until the grammar/tree is
   *  ready. Reuses the same `extractSymbols` walk the Outline panel feeds — no drift. */
  export function getStructure(): NemusSymbol[] {
    if (!view) return [];
    const tree = getNemusTree(view);
    if (!tree) return [];
    return extractSymbols(tree).outline;
  }

  // ── Structural refactors (rename · extract → let · inline) ───────────────────
  // The pure planners live in `nemus-refactor`; these resolve the live tree +
  // selection, dispatch the change set as one undoable transaction, and return a
  // result the host turns into a toast (or keeps the rename/extract input open on
  // an error). `Outcome` is the shared shape.

  type RefactorOutcome = { ok: boolean; error?: string; note?: string };

  function anchorAt(pos: number): UsageAnchor | null {
    const c = view?.coordsAtPos(pos);
    return c ? { x: c.left, y: c.bottom } : null;
  }

  /** The user symbol under the caret to rename, or null (not on a name you
   *  defined). Restricted to declared symbols so a builtin can't be renamed. */
  export function prepareRename(): { name: string; anchor: UsageAnchor | null } | null {
    if (!view) return null;
    const tree = getNemusTree(view);
    if (!tree) return null;
    const head = view.state.selection.main.head;
    const name = identifierAt(tree, head) ?? (head > 0 ? identifierAt(tree, head - 1) : null);
    if (!name) return null;
    const { defs, imports } = extractSymbols(tree);
    if (!defs.has(name) && !imports.has(name)) return null;
    return { name, anchor: anchorAt(head) };
  }

  export function applyRename(oldName: string, newName: string): RefactorOutcome {
    if (!view) return { ok: false };
    const tree = getNemusTree(view);
    if (!tree) return { ok: false, error: 'Editor not ready' };
    const plan = renamePlan(tree, oldName, newName);
    if (plan.error) return { ok: false, error: plan.error };
    if (plan.changes.length) view.dispatch({ changes: plan.changes });
    view.focus();
    return { ok: true, note: plan.note };
  }

  /** The current selection if it cleanly spans a host expression (so it can be
   *  extracted), plus a suggested fresh name + caret anchor. Null otherwise. */
  export function prepareExtract(): { from: number; to: number; suggested: string; anchor: UsageAnchor | null } | null {
    if (!view) return null;
    const tree = getNemusTree(view);
    if (!tree) return null;
    const sel = view.state.selection.main;
    if (sel.empty) return null;
    const t = extractTarget(tree, view.state.doc.toString(), sel.from, sel.to);
    if (!t) return null;
    return { from: t.from, to: t.to, suggested: freshName(tree, 'phrase'), anchor: anchorAt(t.to) };
  }

  export function applyExtract(from: number, to: number, name: string): RefactorOutcome {
    if (!view) return { ok: false };
    const tree = getNemusTree(view);
    if (!tree) return { ok: false, error: 'Editor not ready' };
    const plan = extractLetPlan(tree, view.state.doc.toString(), from, to, name);
    if (plan.error) return { ok: false, error: plan.error };
    if (plan.changes.length) view.dispatch({ changes: plan.changes });
    view.focus();
    return { ok: true, note: plan.note };
  }

  /** Inline the `let` under the caret into its uses (and delete the declaration). */
  export function applyInline(): RefactorOutcome {
    if (!view) return { ok: false };
    const tree = getNemusTree(view);
    if (!tree) return { ok: false, error: 'Editor not ready' };
    const head = view.state.selection.main.head;
    const name = identifierAt(tree, head) ?? (head > 0 ? identifierAt(tree, head - 1) : null);
    if (!name) return { ok: false, error: 'Place the caret on a let name' };
    const plan = inlinePlan(tree, view.state.doc.toString(), name);
    if (plan.error) return { ok: false, error: plan.error };
    if (plan.changes.length) view.dispatch({ changes: plan.changes });
    view.focus();
    return { ok: true, note: plan.note };
  }

  // ── Intentions (Alt+Enter quick-fixes) ───────────────────────────────────────

  /** The context actions available at the caret / selection (rename · inline ·
   *  extract · fix unresolved instrument · transpose notes), plus a caret anchor
   *  for the popup. Empty list when nothing applies. */
  export function getIntentions(): { items: IntentionItem[]; anchor: UsageAnchor | null } | null {
    if (!view) return null;
    const tree = getNemusTree(view);
    if (!tree) return { items: [], anchor: null };
    const sel = view.state.selection.main;
    const items = collectIntentions({
      tree,
      src: view.state.doc.toString(),
      head: sel.head, from: sel.from, to: sel.to,
      instruments: soundsStore.instruments.map((i) => i.name),
      scales: scalesStore.modes,
    });
    return { items, anchor: anchorAt(sel.head) };
  }

  /** Apply an "edit" intention's change set (one undoable transaction). */
  export function applyIntentionEdits(edits: EditChange[]): void {
    if (!view || !edits.length) return;
    const sorted = [...edits].sort((a, b) => a.from - b.from || a.to - b.to);
    view.dispatch({ changes: sorted });
    view.focus();
  }

  // ── Change scale (Alt+Enter on a `.scale("…")`) ───────────────────────────────

  /** The current scale spec when the caret sits in a `.scale("…")` string (so the
   *  host can open an input prefilled with it), or null. */
  export function prepareChangeScale(): { spec: string; anchor: UsageAnchor | null } | null {
    if (!view) return null;
    const tree = getNemusTree(view);
    if (!tree) return null;
    const head = view.state.selection.main.head;
    const sa = stringArgCallAt(tree, head);
    if (!sa || sa.fn !== 'scale') return null;
    return { spec: view.state.doc.toString().slice(sa.from, sa.to), anchor: anchorAt(head) };
  }

  /** Change the scale at the caret to `newSpec`, re-spelling its notes to keep
   *  their degree (one undoable transaction). */
  export function applyChangeScale(newSpec: string): RefactorOutcome {
    if (!view) return { ok: false };
    const tree = getNemusTree(view);
    if (!tree) return { ok: false, error: 'Editor not ready' };
    const plan = changeScalePlan(tree, view.state.doc.toString(), view.state.selection.main.head, newSpec, scalesStore.modes);
    if (plan.error) return { ok: false, error: plan.error };
    if (plan.changes.length) view.dispatch({ changes: plan.changes });
    view.focus();
    return { ok: true, note: plan.note };
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
    const tree = getNemusTree(view);
    if (!tree) return { treeReady: false, applied: 0, skipped: [] };
    const src = view.state.doc.toString();
    const { changes, skipped } = buildControlEdits(tree, src, index, edits);
    if (changes.length) view.dispatch({ changes });
    return { treeReady: true, applied: changes.length, skipped };
  }
</script>

<div class="grv-editor" bind:this={hostEl} oncontextmenu={onContextMenu}></div>

{#if ctx}
  <ContextMenu items={ctxItems} x={ctx.x} y={ctx.y} onSelect={onCtxSelect} onClose={() => (ctx = null)} />
{/if}

<style>
  .grv-editor {
    flex: 1;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .grv-editor :global(.cm-editor) { height: 100%; }
</style>
