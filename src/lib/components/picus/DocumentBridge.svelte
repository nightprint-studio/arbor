<script lang="ts">
  /**
   * Keeps the right-hand tool panels and one editor pointed at the same thing.
   *
   * Renders nothing. It exists because the wiring is a handful of effects with
   * non-obvious conditions on them, and every editor host in Picus needs all of
   * them — written per view they would be several chances to get the last one
   * wrong, which is the one that leaks a stale answer between documents.
   *
   * It feeds **both** tools rather than one, and that is deliberate: they want the
   * same three things from an editor (its text, a channel to select a byte range
   * in it, and a signal that the document went away), and two bridges stacked in
   * every view would be the same wiring written twice.
   *
   * The conditions:
   *  1. **Only work when somebody is looking.** A round trip per keystroke for a
   *     panel that is closed is work nobody asked for.
   *  2. **A click in a panel selects bytes in the editor** — through
   *     `selectByteRange`, because everything the backend reports is UTF-8 and
   *     every one of those offsets is wrong by the number of accented characters
   *     before it if it reaches CodeMirror unconverted.
   *  3. **The document going away clears both panels.** Without it they keep
   *     describing the file you just closed, which reads as broken rather than as
   *     stale.
   *
   * The caret direction is not here: it arrives through the editor's own `oncaret`
   * prop, so its host passes it. One line, at the place that already owns the
   * editor's props.
   */
  import { untrack } from 'svelte';

  import { astStore } from '$lib/stores/picus/ast.svelte';
  import { bufferRestructureStore } from '$lib/stores/picus/restructure-buffer.svelte';
  import { parseFaultStore } from '$lib/stores/picus/parse-faults.svelte';
  import { statementSpanStore } from '$lib/stores/picus/statement-spans.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { Dialect } from '$lib/types/picus';

  /** Structural, like the editor handles the other views bind: a host binds to the
   *  imperative surface it uses, never to the component's whole type. */
  interface EditorHandle {
    selectByteRange: (startByte: number, endByte: number) => void;
    getValue: () => string;
  }

  interface Props {
    editor: EditorHandle | null;
    /**
     * The host's copy of the buffer. Used as the **trigger**, never as the text
     * that is read — see below.
     */
    text: string;
    /** Which grammar to parse against. Omit and the parser takes the portable
     *  intersection, which is the honest answer for a document with no engine. */
    dialect?: Dialect;
  }

  let { editor, text, dialect }: Props = $props();

  const showingTree = $derived(picusUiStore.toolOpen && picusUiStore.toolSection === 'ast');
  const showingReplace = $derived(
    picusUiStore.toolOpen && picusUiStore.toolSection === 'restructure',
  );

  /**
   * Read the **editor's own document**, not the host's string.
   *
   * They are not the same on a CRLF file: CodeMirror normalises line endings when
   * it takes a document, so the host's raw text is one byte longer per line than
   * what the editor holds. Every offset computed from it is then short by the
   * number of lines above it — which reads as a tree that is subtly, increasingly
   * misaligned the further down the file you look, and as a structural replace
   * that splices into the middle of the wrong statement.
   *
   * `selectByteRange` and `replaceByteRange` convert against that same document, so
   * taking the text from there is what makes the two ends agree. The host's `text`
   * stays as the dependency that tells us a keystroke happened.
   */
  $effect(() => {
    if (!showingTree || !editor) return;
    void text;
    astStore.follow(untrack(() => editor.getValue()));
  });

  $effect(() => {
    if (!showingReplace || !editor) return;
    void text;
    bufferRestructureStore.follow(untrack(() => editor.getValue()));
  });

  /**
   * The parse, which is **not** gated on a panel being open.
   *
   * Unlike the two above, nobody goes looking for this: it draws the squiggle under
   * SQL the grammar cannot read, and a squiggle that only appears when you happen
   * to have the syntax-tree panel open would be worse than none.
   */
  $effect(() => {
    if (!editor) return;
    void text;
    parseFaultStore.follow(untrack(() => editor.getValue()), dialect);
  });

  /**
   * Where the statements are, for the same reason and on the same terms.
   *
   * Not gated on a panel either, and gated on nothing else: completion, hover,
   * ghost text and the semantic diagnostics all read these boundaries through
   * `scanSql`, so a buffer nobody is asking about is a buffer where the caret is in
   * the wrong statement. Everything downstream degrades gracefully until the answer
   * lands — see `statementSpanStore`.
   */
  $effect(() => {
    if (!editor) return;
    void text;
    statementSpanStore.follow(untrack(() => editor.getValue()), dialect);
  });

  $effect(() => {
    const request = astStore.selectRequest;
    if (!request || !editor) return;
    // `untrack`: the call reaches into the editor, and nothing it touches there
    // should become a dependency of this effect.
    untrack(() => editor?.selectByteRange(request.start, request.end));
  });

  $effect(() => () => {
    astStore.clear();
    bufferRestructureStore.clear();
    parseFaultStore.clear();
    statementSpanStore.clear();
  });
</script>
