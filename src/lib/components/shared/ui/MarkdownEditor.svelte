<script lang="ts">
  /**
   * The markdown editor, as a mount.
   *
   * The editor itself is `$lib/utils/markdown-editor` — CodeMirror 6 + Lezer with the
   * Obsidian-style live preview, where the markup characters are concealed on every line
   * except the one holding the caret. This component is the ten lines around it that three
   * places were each writing for themselves: the CodeMirror view, the rebuild-on-a-new-document
   * rule, the read-only compartment, and the teardown.
   *
   * **What is a document is the caller's business.** `docKey` identifies it; when that string
   * changes the view is rebuilt from `text`, and `text` is read at that moment only. Keying on
   * the text instead would rebuild the editor on every keystroke and fight the caret — which is
   * why the callers that have a revision counter pass `path#revision` rather than the bytes.
   *
   * Every callback carries the key of the document that produced it, **frozen at mount**. A view
   * outlives a prop change by exactly one teardown, and that teardown is a blur: without the
   * frozen key, "the caret left" would name the document that has just replaced it, and the
   * caller would write one file's bytes over another's.
   *
   * **The bytes are the record.** `onChange` reports the document verbatim; nothing here trims,
   * re-indents or re-wraps. A save that tidied the file would rewrite lines nobody touched.
   *
   * Product-agnostic on purpose (`docs/garrulus-design.md` §7.0): no vault, no project, no
   * store. What a product knows arrives as props and leaves as callbacks. The next step of that
   * decision is moving the util under `shared/ui/markdown-editor/`; this is the mount half of it.
   */
  import { onMount, untrack } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import {
    createMarkdownExtensions,
    goToHeading,
    makeMarkdownCompartments,
  } from '$lib/utils/markdown-editor';

  interface Props {
    /** Identity of the document. A change rebuilds the view from `text`. */
    docKey: string;
    /** The document's bytes. Read when `docKey` changes, never after. */
    text: string;
    /** Absolute path of the file, so `![](./img.png)` and friends resolve. */
    docPath?: string | null;
    /** Reading mode. Flipped through a compartment, so the scroll position and the undo
     *  history survive the toggle. */
    readOnly?: boolean;
    /** Put the caret in the document as soon as it mounts. On for an editor you opened to
     *  type in; off for one that appears beside something else you were already using. */
    autofocus?: boolean;
    onChange?: (text: string, docKey: string) => void;
    /** The caret left the editor — for a caller whose save is "when you look away". */
    onBlur?: (docKey: string) => void;
    /** 1-based caret position, for a host that shows it in a status bar. */
    onCaret?: (line: number, col: number) => void;
    /**
     * A link pointed at a file — open it.
     *
     * The editor resolves the path (relative ones against `docPath`) and hands it over; what
     * opening means is the host's: a tab, a note, a window. Omitted → the operating system does
     * it, which is the right answer for the `.pdf` beside the document and the wrong one for the
     * `.md` next to it.
     */
    onOpenLink?: (path: string, anchor: string | null) => void;
    /**
     * The files `[…](` completes to — absolute paths, offered relative to this document.
     *
     * A function, called when the list is built, so a host with a live project tree does not
     * have to push a new array at this component every time the tree moves. Omitted → the
     * completion still offers this file's own headings, which needs nobody's help.
     */
    fileIndex?: () => string[];
  }

  let {
    docKey,
    text,
    docPath = null,
    readOnly = false,
    autofocus = true,
    onChange,
    onBlur,
    onCaret,
    onOpenLink,
    fileIndex,
  }: Props = $props();

  let hostEl = $state<HTMLDivElement | undefined>(undefined);
  let view: EditorView | null = null;
  const compartments = makeMarkdownCompartments();

  /** Focus the document — a host calls this when its tab comes forward. */
  export function focus() {
    view?.focus();
  }

  /**
   * Put the caret on the heading a `#slug` names, GitHub-style ids from the heading's own text.
   *
   * For the host that has just opened this file because a `guida.md#uso` link asked it to: the
   * jump belongs to the document that arrives, not to the one that was clicked, so it cannot
   * happen inside the editor that saw the click. `false` when there is no such heading — or when
   * the view has not mounted yet, which is the caller's cue to try again on the next frame.
   */
  export function goToAnchor(slug: string): boolean {
    return view ? goToHeading(view, slug) : false;
  }

  function mount(host: HTMLElement, doc: string) {
    // The document this view belongs to, for as long as it lives — see the note above.
    const key = docKey;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc,
        extensions: [
          createMarkdownExtensions(
            { readOnly, docPath, onOpenLink: onOpenLink ?? null, fileIndex: fileIndex ?? null },
            compartments,
          ),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChange?.(u.state.doc.toString(), key);
            if ((u.docChanged || u.selectionSet) && onCaret) {
              const head = u.state.selection.main.head;
              const line = u.state.doc.lineAt(head);
              onCaret(line.number, head - line.from + 1);
            }
          }),
          EditorView.domEventHandlers({
            blur: () => {
              onBlur?.(key);
              return false;
            },
          }),
        ],
      }),
    });
  }

  // Rebuild on a new document, and only then. `text` and `readOnly` are read untracked: the
  // first is the document's initial content (tracking it would re-enter on every keystroke),
  // the second is reconfigured below without a rebuild.
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
    // Deferred a microtask so the mount has settled before the caret lands in it.
    if (autofocus) queueMicrotask(() => view?.focus());
  });

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

<div class="mde" bind:this={hostEl}></div>

<style>
  .mde {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .mde :global(.cm-editor) {
    height: 100%;
    background: var(--bg-base);
  }
</style>
