<script lang="ts">
  /**
   * A note, in the markdown editor.
   *
   * **This is a mount, not an editor.** The editor is
   * `$lib/utils/markdown-editor` — the same CodeMirror 6 + Lezer live preview
   * `shared/MarkdownEditorModal` uses — and it stays shared:
   * `docs/garrulus-design.md` §7.0 decided it becomes product-agnostic and moves
   * to `shared/ui/markdown-editor/`, so a second copy carrying vault knowledge is
   * exactly the outcome that decision exists to prevent. Nothing about wikilinks,
   * tags or note types is inlined here.
   *
   * What is vault-specific arrives as props and leaves as callbacks: which bytes
   * to show, where the file is (so relative images resolve), and what to do when
   * they change or the caret leaves. When the shared editor grows its
   * `LinkProvider` / `TagProvider` seam, that provider object becomes one more
   * prop on this component and nothing else here moves.
   *
   * **The bytes are the record.** `onChange` reports the document verbatim and
   * `onBlur` asks the store to write it; nothing on this path trims, re-indents
   * or re-wraps. A save that tidied the file would rewrite lines the user never
   * touched and turn the next sync into a diff nobody can read.
   */
  import { onMount, untrack } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import {
    createMarkdownExtensions,
    makeMarkdownCompartments,
  } from '$lib/utils/markdown-editor';

  interface Props {
    /** Vault-relative path of the note. Echoed back on every callback so the
     *  caller never has to assume the note that answered is still the one in
     *  front — a rebuild fires `blur` after the switch, not before it. */
    notePath: string;
    /** Bumped by the store on each read of this note. Together with `notePath` it
     *  identifies the *document*, which is what the editor is rebuilt for. It is
     *  deliberately not the text: that changes on every keystroke, and an editor
     *  that rebuilt itself on those would fight the caret. */
    revision: number;
    /** The document's bytes. Read when the document changes, never after. */
    text: string;
    /** Absolute path of the file, so `![](./img.png)` resolves. */
    docPath: string | null;
    /** Reading mode: the whole note renders, because nothing holds a caret. */
    readOnly?: boolean;
    onChange: (notePath: string, text: string) => void;
    /** The caret left the editor. The store decides whether that is a save. */
    onBlur?: (notePath: string) => void;
  }

  let { notePath, revision, text, docPath, readOnly = false, onChange, onBlur }: Props = $props();

  const docKey = $derived(`${notePath}#${revision}`);

  let hostEl = $state<HTMLDivElement | undefined>(undefined);
  let view: EditorView | null = null;
  const compartments = makeMarkdownCompartments();

  /** Focus the document — the workspace calls this when a tab comes forward. */
  export function focus() {
    view?.focus();
  }

  function mount(host: HTMLElement, doc: string) {
    // The path this document belongs to, frozen at mount: the extensions below
    // outlive a prop change by exactly one teardown, and that teardown is a blur.
    const path = notePath;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc,
        extensions: [
          createMarkdownExtensions({ readOnly, docPath }, compartments),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChange(path, u.state.doc.toString());
          }),
          EditorView.domEventHandlers({
            blur: () => {
              onBlur?.(path);
              return false;
            },
          }),
        ],
      }),
    });
  }

  // Rebuild on a new document, and only then. `text` and `readOnly` are read
  // untracked: the first is re-read here on purpose (it is the document's initial
  // content) and would otherwise re-enter on every keystroke; the second is
  // reconfigured below without a rebuild.
  $effect(() => {
    const key = docKey;
    const host = hostEl;
    if (!host) return;
    void key;
    untrack(() => {
      view?.destroy();
      view = null;
      mount(host, text);
    });
    // Opening a note means being in it: the caret follows, so typing works
    // without a Tab. Deferred a microtask so the mount has settled first.
    queueMicrotask(() => view?.focus());
  });

  // Reading mode flips through the compartment rather than by rebuilding, so the
  // scroll position and the undo history survive the toggle.
  $effect(() => {
    const ro = readOnly;
    if (!view) return;
    view.dispatch({
      effects: compartments.readOnly.reconfigure(EditorState.readOnly.of(ro)),
    });
  });

  onMount(() => () => {
    view?.destroy();
    view = null;
  });
</script>

<div class="gne" bind:this={hostEl}></div>

<style>
  .gne {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .gne :global(.cm-editor) {
    height: 100%;
    background: var(--bg-base);
  }
</style>
