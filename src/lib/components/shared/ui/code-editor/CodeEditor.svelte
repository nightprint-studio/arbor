<script lang="ts">
  /**
   * CodeEditor — the generic, app-agnostic CodeMirror 6 host for one buffer.
   *
   * Generalised from merula's `MerulaEditor`, but with NO product/engine imports:
   * it is parametrised by a {@link LanguageDescriptor} (syntax highlight, go-to-decl)
   * and driven entirely through props. Controlled `value`: external writes (tab
   * switch, cross-file open) flow in via the prop; internal edits flow out via
   * `oninput`. Imperative API (focus / getValue / scrollToLineCol / scrollToOffset /
   * openSearch / setDiagnostics) is exposed via `bind:this`.
   *
   * Diagnostics arrive as {@link EditorDiagnostic}[] in **UTF-8 byte offsets**; they
   * are mapped onto CodeMirror's UTF-16 lint spans against the live buffer.
   */
  import { onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { setDiagnostics as cmSetDiagnostics, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
  import { openSearchPanel } from '@codemirror/search';

  import type { LanguageDescriptor, EditorDiagnostic } from './types';
  import { createCodeEditorExtensions } from './extensions';
  import { makeByteToU16 } from './highlight';

  let {
    value,
    language,
    readOnly = false,
    diagnostics = [],
    oninput,
    oncaret,
    onfocus,
    onGoto,
  }: {
    value: string;
    language: LanguageDescriptor;
    readOnly?: boolean;
    /** Diagnostics in UTF-8 byte offsets — mapped to CM lint spans against the buffer. */
    diagnostics?: EditorDiagnostic[];
    oninput?: (text: string) => void;
    /** Live caret position (1-based line/col) — drives a host footer Ln/Col. */
    oncaret?: (line: number, col: number) => void;
    onfocus?: () => void;
    /** Ctrl/Cmd+Click on an identifier the descriptor didn't resolve locally. */
    onGoto?: (word: string, view: EditorView) => void;
  } = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let suppressEmit = false;
  let lastEmitted: string | null = null;

  // ── Byte-span diagnostics → CM lint markers ───────────────────────────────────
  function toCmDiagnostics(errors: EditorDiagnostic[], src: string): CmDiagnostic[] {
    const b2u = makeByteToU16(src);
    const len = src.length;
    const out: CmDiagnostic[] = [];
    for (const e of errors) {
      let from = b2u(e.from);
      let to = b2u(e.to);
      from = Math.max(0, Math.min(from, len));
      to = Math.max(from, Math.min(to, len));
      if (to === from) to = Math.min(len, from + 1); // give the marker some width
      out.push({ from, to, severity: e.severity, message: e.message });
    }
    return out;
  }

  function pushDiagnostics() {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch(cmSetDiagnostics(view.state, toCmDiagnostics(diagnostics, src)));
  }
  // Re-push whenever the diagnostics prop changes.
  $effect(() => { void diagnostics; pushDiagnostics(); });

  function mount(target: HTMLDivElement) {
    const { extensions } = createCodeEditorExtensions(language, { readOnly, onGoto });

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
      extensions: [extensions, updateListener],
    });
    view = new EditorView({ state, parent: target });
    pushDiagnostics();
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });
  onDestroy(() => { view?.destroy(); view = undefined; });

  // ── value (controlled) → editor ───────────────────────────────────────────────
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

  // ── Imperative API ────────────────────────────────────────────────────────────
  export function focus() { view?.focus(); }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }

  /** Open CodeMirror's search panel + focus its query field (routed here from the
   *  host's Ctrl+F when the editor pane has focus). */
  export function openSearch() {
    if (view) openSearchPanel(view);
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

  /** Imperatively replace the diagnostics (byte spans → lint), e.g. after a fresh
   *  async lint run when the host isn't binding the `diagnostics` prop. */
  export function setDiagnostics(errors: EditorDiagnostic[]) {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch(cmSetDiagnostics(view.state, toCmDiagnostics(errors, src)));
  }
</script>

<!-- CodeMirror mount host: the editable surface and all keyboard interaction live in
     CM inside this node. -->
<div class="code-editor" bind:this={hostEl}></div>

<style>
  .code-editor {
    flex: 1;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .code-editor :global(.cm-editor) { height: 100%; }
</style>
