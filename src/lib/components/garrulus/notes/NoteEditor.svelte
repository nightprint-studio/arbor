<script lang="ts">
  /**
   * A note, in the markdown editor.
   *
   * **This is a mount of a mount.** The editor is `$lib/utils/markdown-editor` and the
   * CodeMirror plumbing around it is `shared/ui/MarkdownEditor` — the same one the markdown
   * modal and Bennu's `.md` tabs use. What is left here is the vault's half of the contract:
   * which bytes to show, which note answered, and what a blur means.
   *
   * Nothing about wikilinks or tags is inlined here either: the syntax lives in
   * `$lib/utils/markdown-vault`, and what the vault answers (which titles exist, what following a
   * link does) is the provider built in `vault-link-provider.ts` (`docs/garrulus-design.md` §7.0).
   *
   * **The bytes are the record.** `onChange` reports the document verbatim and `onBlur` asks the
   * store to write it; nothing on this path trims, re-indents or re-wraps. A save that tidied the
   * file would rewrite lines the user never touched and turn the next sync into a diff nobody can
   * read.
   */
  import type { EditorView } from '@codemirror/view';
  import MarkdownEditor from '$lib/components/shared/ui/MarkdownEditor.svelte';
  import { markdownFormatKeymap, type FormatState } from '$lib/utils/markdown-format';
  import { vaultMarkdown } from '$lib/utils/markdown-vault';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { createVaultLinkProvider, refreshVaultTags } from './vault-link-provider';

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
    /** What the caret is in, for the formatting toolbar's pressed buttons. */
    onFormatState?: (state: FormatState) => void;
  }

  let { notePath, revision, text, docPath, readOnly = false, onChange, onBlur, onFormatState }: Props = $props();

  let editor = $state<{ focus: () => void; exec: (c: (v: EditorView) => boolean) => boolean } | null>(
    null,
  );

  // One provider and one extension set for this mount: the editor reads them when a document is
  // (re)built, so rebuilding them per render would only make garbage.
  const vault = vaultMarkdown(createVaultLinkProvider());
  const extensions = [...vault.extensions, markdownFormatKeymap];

  // Tags in use change when notes do; the catalogue array is replaced on every refresh.
  $effect(() => {
    void garrulusNotesStore.notes;
    void refreshVaultTags();
  });

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

  /** Run a formatting command on the note — what the toolbar calls. */
  export function exec(command: (view: EditorView) => boolean): boolean {
    return editor?.exec(command) ?? false;
  }
</script>

<MarkdownEditor
  bind:this={editor}
  {docKey}
  {text}
  {docPath}
  {readOnly}
  {extensions}
  {onFormatState}
  completionSources={vault.completionSources}
  onChange={(t, key) => onChange(noteOf(key), t)}
  onBlur={(key) => onBlur?.(noteOf(key))}
/>
