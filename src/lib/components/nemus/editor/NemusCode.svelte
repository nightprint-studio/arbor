<script lang="ts">
  /**
   * Read-only `.nemus` code sample — a tiny CodeMirror view with the nemus syntax
   * highlight (and theme) but none of the editor machinery (no gutters, line
   * numbers, lint or selection). Used by the in-app Docs so code samples are
   * highlighted with the SAME colours as the real editor, instead of flat
   * `<pre><code>` text — and can never drift from the live highlighter.
   */
  import { onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { createNemusViewerExtensions } from './nemus-cm';

  let { code }: { code: string } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let view: EditorView | undefined;

  function mount(target: HTMLDivElement) {
    view = new EditorView({
      state: EditorState.create({ doc: code, extensions: [createNemusViewerExtensions()] }),
      parent: target,
    });
  }

  $effect(() => { if (host && !view) mount(host); });

  // Keep the rendered sample in sync if the `code` prop changes (rare — samples
  // are static — but cheap and keeps it a proper controlled component).
  $effect(() => {
    const next = code;
    if (!view) return;
    const cur = view.state.doc.toString();
    if (cur === next) return;
    view.dispatch({ changes: { from: 0, to: cur.length, insert: next } });
  });

  onDestroy(() => { view?.destroy(); view = undefined; });
</script>

<div class="nemus-code" bind:this={host}></div>

<style>
  /* The CodeMirror theme paints the text colours; this just frames the block like
     the docs' former <pre> (panel surface, rounded, scrolls horizontally). */
  .nemus-code {
    margin: 10px 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-input);
    font-size: 12.5px;
  }
  .nemus-code :global(.cm-editor) { background: transparent; }
  .nemus-code :global(.cm-scroller) {
    font-family: var(--font-code);
    line-height: 1.55;
    padding: 10px 12px;
    overflow-x: auto;
  }
  .nemus-code :global(.cm-content) { padding: 0; }
  /* No caret / focus ring on an inert viewer. */
  .nemus-code :global(.cm-editor.cm-focused) { outline: none; }
  .nemus-code :global(.cm-cursor) { display: none; }
</style>
