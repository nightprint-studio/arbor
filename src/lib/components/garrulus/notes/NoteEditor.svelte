<script lang="ts">
  /**
   * A note, in the markdown editor.
   *
   * **This is a mount of a mount.** The editor is `$lib/utils/markdown-editor` and the
   * CodeMirror plumbing around it is `shared/ui/MarkdownEditor` — the same one the markdown
   * modal and Bennu's `.md` tabs use. What is left here is the vault's half of the contract:
   * which bytes to show, which note answered, and what a blur means.
   *
   * Nothing about wikilinks, tags or note types is inlined here either: when the shared editor
   * grows its `LinkProvider` / `TagProvider` seam, that provider object becomes one more prop on
   * this component and nothing else moves (`docs/garrulus-design.md` §7.0).
   *
   * **The bytes are the record.** `onChange` reports the document verbatim and `onBlur` asks the
   * store to write it; nothing on this path trims, re-indents or re-wraps. A save that tidied the
   * file would rewrite lines the user never touched and turn the next sync into a diff nobody can
   * read.
   */
  import MarkdownEditor from '$lib/components/shared/ui/MarkdownEditor.svelte';

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

  let editor = $state<{ focus: () => void } | null>(null);

  /** The document's identity for the editor: the note, plus the read that produced these bytes. */
  const docKey = $derived(`${notePath}#${revision}`);

  /** The note a callback's key belongs to. The editor hands back the key it was mounted with —
   *  which is the whole point on a blur, since that one arrives *after* the switch and would
   *  otherwise be attributed to the note that has just replaced this one. */
  function noteOf(key: string): string {
    return key.slice(0, key.lastIndexOf('#'));
  }

  /** Focus the document — the workspace calls this when a tab comes forward. */
  export function focus() {
    editor?.focus();
  }
</script>

<MarkdownEditor
  bind:this={editor}
  {docKey}
  {text}
  {docPath}
  {readOnly}
  onChange={(t, key) => onChange(noteOf(key), t)}
  onBlur={(key) => onBlur?.(noteOf(key))}
/>
