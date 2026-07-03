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

  import type { LanguageDescriptor, EditorDiagnostic, EditorViewSnapshot } from './types';
  import { createCodeEditorExtensions, refTextAt } from './extensions';
  import { makeByteToU16 } from './highlight';

  let {
    value,
    language,
    readOnly = false,
    diagnostics = [],
    rulerColumn,
    initialState,
    oninput,
    oncaret,
    onViewState,
    onfocus,
    onGoto,
  }: {
    value: string;
    language: LanguageDescriptor;
    readOnly?: boolean;
    /** Diagnostics in UTF-8 byte offsets — mapped to CM lint spans against the buffer. */
    diagnostics?: EditorDiagnostic[];
    /** Draw a vertical margin guide at this 1-based column (IntelliJ-style). Omit for none. */
    rulerColumn?: number;
    /** Cursor + scroll to restore at mount (e.g. the tab's last-known position). */
    initialState?: EditorViewSnapshot;
    oninput?: (text: string) => void;
    /** Live caret position (1-based line/col) — drives a host footer Ln/Col. */
    oncaret?: (line: number, col: number) => void;
    /** Cursor + scroll changed — the host can persist it for a later {@link initialState}. */
    onViewState?: (s: EditorViewSnapshot) => void;
    onfocus?: () => void;
    /** Ctrl/Cmd+Click on an identifier the descriptor didn't resolve locally — the word
     *  plus the clicked position as a UTF-8 byte offset (for a BE go-to-declaration). */
    onGoto?: (word: string, view: EditorView, byteOffset: number) => void;
  } = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let suppressEmit = false;
  let lastEmitted: string | null = null;
  // Scroll-listener teardown (emits `onViewState` so the host can persist scroll too).
  let detachScroll: (() => void) | null = null;
  let scrollRaf = 0;

  /** Report the current cursor + scroll to the host (for per-tab restore). */
  function emitViewState() {
    if (!view || !onViewState) return;
    const sel = view.state.selection.main;
    onViewState({ anchor: sel.anchor, head: sel.head, scrollTop: view.scrollDOM.scrollTop });
  }

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
      out.push({ from, to, severity: e.severity, message: e.message, actions: e.actions });
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
    const { extensions } = createCodeEditorExtensions(language, { readOnly, onGoto, rulerColumn });

    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged && !suppressEmit) {
        const text = u.state.doc.toString();
        lastEmitted = text;
        oninput?.(text);
      }
      if (u.focusChanged && u.view.hasFocus) onfocus?.();
      if (u.selectionSet || u.docChanged) {
        if (oncaret) {
          const head = u.state.selection.main.head;
          const line = u.state.doc.lineAt(head);
          oncaret(line.number, head - line.from + 1);
        }
        emitViewState();
      }
    });

    const state = EditorState.create({
      doc: value,
      extensions: [extensions, updateListener],
    });
    view = new EditorView({ state, parent: target });
    pushDiagnostics();

    // Restore the host-provided cursor + scroll (per-tab position). The scroll is set
    // after a frame so the layout the offset refers to exists.
    if (initialState) {
      const len = view.state.doc.length;
      const anchor = Math.min(Math.max(0, initialState.anchor), len);
      const head = Math.min(Math.max(0, initialState.head), len);
      view.dispatch({ selection: { anchor, head } });
      const top = initialState.scrollTop;
      requestAnimationFrame(() => { if (view) view.scrollDOM.scrollTop = top; });
    }

    // Persist scroll changes too (selection changes come through the update listener).
    const onScroll = () => {
      if (scrollRaf) return;
      scrollRaf = requestAnimationFrame(() => { scrollRaf = 0; emitViewState(); });
    };
    view.scrollDOM.addEventListener('scroll', onScroll, { passive: true });
    detachScroll = () => view?.scrollDOM.removeEventListener('scroll', onScroll);
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });
  onDestroy(() => {
    if (scrollRaf) cancelAnimationFrame(scrollRaf);
    detachScroll?.();
    view?.destroy();
    view = undefined;
  });

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

  /** The caret head as a **UTF-8 byte offset** (what byte-offset backends want, e.g.
   *  a rename / find-usages query). CodeMirror positions are UTF-16 code units, so we
   *  measure the encoded length of the text before the head. 0 when unmounted. */
  export function caretByteOffset(): number {
    if (!view) return 0;
    const head = view.state.selection.main.head;
    return new TextEncoder().encode(view.state.doc.sliceString(0, head)).length;
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

  /** The caret's viewport coordinates (bottom-left of the primary selection head),
   *  for anchoring a caret-attached popup (intentions / usages). Null when the
   *  editor isn't mounted or the position is off-screen. Mirrors merula's
   *  `anchorAt`. */
  export function coordsAtCaret(): { x: number; y: number } | null {
    if (!view) return null;
    const c = view.coordsAtPos(view.state.selection.main.head);
    return c ? { x: c.left, y: c.bottom } : null;
  }

  /** The identifier under (or just before) the caret, or null. Boundary-tolerant:
   *  the caret often sits at a word's right edge, so we scan both directions from
   *  the head. Used to label context actions (e.g. "Add import for <word>"). */
  export function wordAtCaret(): string | null {
    if (!view) return null;
    const doc = view.state.doc;
    const head = view.state.selection.main.head;
    const line = doc.lineAt(head);
    const text = line.text;
    const rel = head - line.from;
    const isWord = (ch: string) => /[A-Za-z0-9_$]/.test(ch);
    // Expand left/right from the caret (checking the char before too, so `foo|`
    // resolves to `foo`).
    let start = rel;
    let end = rel;
    while (start > 0 && isWord(text[start - 1])) start--;
    while (end < text.length && isWord(text[end])) end++;
    const word = text.slice(start, end);
    return word.length ? word : null;
  }

  /** The reference-like token at the caret — a string-literal's contents (a JSP
   *  `action="…"` value) or a path/identifier run (`/do/Category/viewTree`), or null.
   *  Powers a host go-to-definition triggered by keyboard (vs the Ctrl+Click seam,
   *  which resolves the same token at the click position via `onGoto`). */
  export function refAtCaret(): string | null {
    if (!view) return null;
    return refTextAt(view.state.doc, view.state.selection.main.head);
  }

  /** Copy the current selection to the clipboard (no-op when nothing is selected). */
  export function copySelection() {
    if (!view) return;
    const s = view.state.selection.main;
    const text = view.state.sliceDoc(s.from, s.to);
    if (text) void navigator.clipboard.writeText(text).catch(() => {});
  }

  /** Cut the current selection to the clipboard (no-op when nothing is selected). */
  export function cutSelection() {
    if (!view) return;
    const s = view.state.selection.main;
    const text = view.state.sliceDoc(s.from, s.to);
    if (!text) return;
    void navigator.clipboard.writeText(text).catch(() => {});
    view.dispatch({ changes: { from: s.from, to: s.to, insert: '' } });
    view.focus();
  }

  /** Paste clipboard text at the caret (replacing any selection). Best-effort — a
   *  blocked clipboard read is swallowed. */
  export async function pasteClipboard() {
    if (!view) return;
    let text = '';
    try { text = await navigator.clipboard.readText(); } catch { return; }
    if (!text) return;
    const s = view.state.selection.main;
    view.dispatch({
      changes: { from: s.from, to: s.to, insert: text },
      selection: { anchor: s.from + text.length },
    });
    view.focus();
  }

  /** Insert `text` at the caret (replacing any selection), leaving the caret right
   *  after the inserted text. Used by generator flows (Alt+Insert → Generate).
   *  Mirrors merula's `insertAtCursor`. */
  export function insertAtCursor(text: string) {
    if (!view || !text) return;
    const sel = view.state.selection.main;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: text },
      selection: { anchor: sel.from + text.length },
    });
    view.focus();
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
