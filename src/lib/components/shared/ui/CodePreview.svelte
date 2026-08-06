<script lang="ts">
  /**
   * A file shown read-only around one place in it — the context column of a results list.
   *
   * ## It is the editor, not a lookalike
   *
   * The obvious cheap answer is to render a window of lines through a static highlighter, and it
   * was tried twice. Both times it produced a preview that did not match the buffer three feet
   * away:
   *
   *   * the editor's own chrome highlighter (`highlightToHtml`) is one regex pass with a
   *     **Java/C-family keyword list** — on XML it colours the quotes and nothing else, on YAML
   *     nothing at all — and its `.cm-tok-*` classes are scoped by CodeMirror to `.cm-editor`,
   *     so outside one they style nothing;
   *   * Prism knows every language, but it is a **second** highlighter: its Java and the
   *     editor's tree-sitter Java disagree about fields, declarations and every semantic class
   *     the editor adds, so the two drift by construction.
   *
   * So this mounts the real thing. Two facts make that affordable, and they are the reason this
   * is not the extravagance it sounds like:
   *
   *   * `CodeEditor` swaps its document **in place** when `value` changes — one instance for the
   *     whole session of a modal, not one per arrow key;
   *   * CodeMirror renders only the viewport, so handing it a 10 000-line file costs what the
   *     visible dozen lines cost.
   *
   * ## The whole file, not a window
   *
   * Which buys three things a slice cannot: the line numbers are the file's own, a block comment
   * or a text block spanning the edge is coloured correctly, and you can scroll to read past the
   * result instead of being handed exactly what somebody guessed you wanted.
   *
   * ## The language comes from the host
   *
   * This lives in `shared/ui` and may not know that `.jsp` is a thing — that is a product's
   * vocabulary. The host resolves the descriptor (Bennu has `languageForPath`) and passes it,
   * exactly as it passes the text.
   */
  import CodeEditor from './code-editor/CodeEditor.svelte';
  import { makeByteToU16, type LanguageDescriptor } from './code-editor';

  interface Props {
    /** The whole file. */
    text: string;
    language: LanguageDescriptor;
    /** 1-based line to band and scroll to. */
    activeLine?: number | null;
    /**
     * The match, as a **byte** range in `text` — everything upstream of here counts bytes
     * (tree-sitter, the backend, the search), and a range that crossed this seam as characters
     * would be a bug waiting for the first accented identifier. Converted here, once.
     */
    markBytes?: { start: number; end: number } | null;
    class?: string;
  }

  let { text, language, activeLine = null, markBytes = null, class: klass = '' }: Props = $props();

  let editor = $state<{ revealLine: (line: number) => void } | null>(null);

  const lineHighlights = $derived(
    activeLine ? [{ line: activeLine, className: 'cp-active' }] : [],
  );

  const marks = $derived.by(() => {
    if (!markBytes) return [];
    const toU16 = makeByteToU16(text);
    return [{ from: toU16(markBytes.start), to: toU16(markBytes.end), className: 'cp-match' }];
  });

  /**
   * Scroll to the banded line whenever it — or the file — changes.
   *
   * `revealLine`, never `scrollToLineCol`: that one ends by focusing, which is right when you
   * are *going* somewhere and wrong here. This editor sits beside a search field the user is
   * typing into, and a preview that grabbed the caret on every arrow key would make the field
   * unusable — which is exactly what it did.
   *
   * On a frame, because the document swap that a new file triggers happens in the editor's own
   * effect: scrolling in this tick would scroll the *previous* document to a line the new one
   * has not got yet.
   */
  $effect(() => {
    void text;
    const line = activeLine;
    if (!line) return;
    const frame = requestAnimationFrame(() => editor?.revealLine(line));
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="cp {klass}">
  <!--
    Keyed on the language, and this is load-bearing.

    `CodeEditor` builds its extension set at mount and never rebuilds it — its `language` is
    static, the way `wrap` and the gutter are. Swapping only the document (which is what makes
    this column cheap) therefore leaves the editor parsing every later file with the **first**
    one's grammar: pick an XML result first and every Java file after it is coloured as XML,
    which is to say not at all.

    So the document swap stays for the common case — walking hits inside one file, or across
    files of the same kind — and the editor is rebuilt only when the *kind* actually changes.
  -->
  {#key language.id}
    <CodeEditor bind:this={editor} {language} value={text} {marks} {lineHighlights} readOnly />
  {/key}
</div>

<style>
  .cp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .cp :global(.cm-editor) { flex: 1; min-height: 0; }

  /* The result's line. A band rather than a tint: it is the row the whole column exists to
     show, and it competes with syntax colour for attention. */
  .cp :global(.cp-active) { background: var(--bg-selected); }

  /* The match inside it — a solid fill, because a search hit has to be findable at a glance in
     a wall of monospace. */
  .cp :global(.cp-match) {
    background: var(--accent);
    color: var(--bg-base);
    border-radius: 2px;
  }
</style>
