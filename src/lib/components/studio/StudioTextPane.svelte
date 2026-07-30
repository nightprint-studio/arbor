<!--
  StudioTextPane — CodeMirror 6 host for every Studio modal's text view.

  Format-agnostic shell: the parent passes a `language` id and a controlled
  `value`. The component:
    - mounts a CM6 EditorView on mount, tears it down on destroy
    - syncs `value` → editor on external change (without echoing back)
    - fires `oninput` per docChange (the parent debounces / pushes to host)
    - exposes a focus / scroll / find imperative API via `bind:this`

  In future this same component will replace the read-only third pane of
  ConflictResolver — only `readOnly` + the language will change.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import type {
    StudioLanguage,
    StudioDiagnostic,
    StudioCompletionItem,
    StudioSnippetItem,
  } from '$lib/utils/studio-codemirror';

  export interface StudioTextPaneProps {
    /** Current text content. Treated as the source of truth from the parent's
     *  perspective: external writes (e.g. after a tree mutation) flow in via
     *  this prop; internal edits flow out via `oninput`. */
    value: string;
    /** Format id used to pick the language extension (RON stream parser,
     *  JSON/TOML/YAML/.properties added in their respective phases). */
    language: StudioLanguage;
    /** Disable typing. Used by the future ConflictResolver pane. */
    readOnly?: boolean;
    /** Fires per text change made inside the editor. NOT debounced — the
     *  parent owns the cadence (e.g. RonStudioModal debounces at 180ms). */
    oninput?: (text: string) => void;
    /** Fires on a pure selection change (cursor move / range select) that is
     *  not itself an edit — `oninput` already covers edits. Reports the main
     *  selection range + its text. */
    onselect?: (sel: { from: number; to: number; text: string }) => void;
    /** Optional callback when the editor gains focus. */
    onfocus?: () => void;
    /** Optional footer (char/line count, parse status, encoding pill). */
    footer?: Snippet;
    /** Show the line-number gutter (default true). */
    showLineNumbers?: boolean;
    /** Highlight the active line (default true). */
    showActiveLine?: boolean;
    /** Plugin-supplied diagnostics. Reactive: a new array → re-render markers. */
    diagnostics?: StudioDiagnostic[];
    /** Plugin-supplied static completions. */
    completions?: StudioCompletionItem[];
    /** Plugin-supplied static snippets (CodeMirror `${1:placeholder}` syntax). */
    snippets?: StudioSnippetItem[];
    /** Force the lint gutter on/off (defaults to "on when diagnostics is non-empty"). */
    showLintGutter?: boolean;
  }

  export interface StudioTextPaneController {
    focus(): void;
    /** Scroll to a 1-based line number, optionally selecting it. */
    scrollToLine(line: number, select?: boolean): void;
    /** Current text — same as the latest `value` propagated out. */
    getValue(): string;
  }
</script>

<script lang="ts">
  import { onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import {
    createStudioExtensions,
    languageExtension,
    makeStudioCompartments,
    diagnosticsExtension,
    buildCompletionSource,
    completionExtension,
  } from '$lib/utils/studio-codemirror';

  let {
    value,
    language,
    readOnly = false,
    oninput,
    onselect,
    onfocus,
    footer,
    showLineNumbers = true,
    showActiveLine = true,
    diagnostics,
    completions,
    snippets,
    showLintGutter,
  }: StudioTextPaneProps = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  /** Set while we're applying an external `value` change, so the
   *  updateListener doesn't echo it back via `oninput`. */
  let suppressEmit = false;
  /** Last text we emitted via `oninput`. When the controlled `value` prop
   *  echoes this exact string back (the parent stored our edit and fed it
   *  straight back in), skip the reconcile entirely — otherwise every
   *  keystroke pays an O(n) `doc.toString()` just to discover it's a no-op,
   *  which is the dominant cost when typing in a large document. */
  let lastEmitted: string | null = null;

  const compartments = makeStudioCompartments();

  function mount(target: HTMLDivElement) {
    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged && !suppressEmit) {
        const text = u.state.doc.toString();
        lastEmitted = text;
        oninput?.(text);
      }
      // Pure selection change (not an edit — edits flow through `oninput`).
      if (u.selectionSet && !u.docChanged && !suppressEmit && onselect) {
        const r = u.state.selection.main;
        onselect({ from: r.from, to: r.to, text: u.state.sliceDoc(r.from, r.to) });
      }
      if (u.focusChanged && u.view.hasFocus) onfocus?.();
    });

    const state = EditorState.create({
      doc: value,
      extensions: [
        createStudioExtensions(
          {
            language, readOnly, showLineNumbers, showActiveLine,
            diagnostics, completions, snippets, showLintGutter,
          },
          compartments,
        ),
        updateListener,
      ],
    });
    view = new EditorView({ state, parent: target });
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────
  $effect(() => {
    if (hostEl && !view) mount(hostEl);
  });

  onDestroy(() => {
    view?.destroy();
    view = undefined;
  });

  // ── value (controlled) → editor ───────────────────────────────────────
  $effect(() => {
    const next = value;
    if (!view) return;
    // Our own edit echoed straight back through the controlled prop — nothing
    // to reconcile, and crucially no need to stringify the whole document.
    if (next === lastEmitted) return;
    const current = view.state.doc.toString();
    if (current === next) return;
    suppressEmit = true;
    try {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: next },
      });
    } finally {
      suppressEmit = false;
    }
  });

  // ── language → compartment ────────────────────────────────────────────
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: compartments.language.reconfigure(languageExtension(language)),
    });
  });

  // ── readOnly → compartment ────────────────────────────────────────────
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: compartments.readOnly.reconfigure(
        EditorState.readOnly.of(readOnly),
      ),
    });
  });

  // ── diagnostics → compartment ─────────────────────────────────────────
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: compartments.diagnostics.reconfigure(
        diagnosticsExtension(diagnostics),
      ),
    });
  });

  // ── completions / snippets → compartment ──────────────────────────────
  $effect(() => {
    if (!view) return;
    const src = buildCompletionSource(completions, snippets);
    view.dispatch({
      effects: compartments.completion.reconfigure(completionExtension(src)),
    });
  });

  // ── Imperative API ────────────────────────────────────────────────────
  export function focus() {
    view?.focus();
  }

  export function scrollToLine(line: number, select = false) {
    if (!view) return;
    const doc = view.state.doc;
    const clamped = Math.max(1, Math.min(line, doc.lines));
    const lineInfo = doc.line(clamped);
    view.dispatch({
      selection: select
        ? { anchor: lineInfo.from, head: lineInfo.to }
        : { anchor: lineInfo.from },
      effects: EditorView.scrollIntoView(lineInfo.from, { y: 'center' }),
    });
    view.focus();
  }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }
</script>

<div class="stp-root">
  <div class="stp-editor" bind:this={hostEl}></div>
  {#if footer}
    <div class="stp-footer">{@render footer()}</div>
  {/if}
</div>

<style>
  /* The CodeMirror EditorView fills the editor box; the footer (if any)
     sits below it. Sized to fill whatever parent container the modal
     allots us — no intrinsic height, no scrollbar arbitration here. */
  .stp-root {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .stp-editor {
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .stp-editor :global(.cm-editor) {
    height: 100%;
  }
  .stp-footer {
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 6px 12px;
    background: var(--bg-overlay);
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
</style>
