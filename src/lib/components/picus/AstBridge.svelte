<script lang="ts">
  /**
   * Keeps the syntax-tree panel and one editor pointed at the same thing.
   *
   * Renders nothing. It exists because the wiring is three effects with three
   * non-obvious conditions on them, and every editor host in Picus needs all
   * three — written per view they would be three chances to get the last one
   * wrong, which is the one that leaks a stale tree between documents.
   *
   * The three:
   *  1. **Only parse when somebody is looking.** A round trip per keystroke for a
   *     panel that is closed is work nobody asked for.
   *  2. **A click in the panel selects bytes in the editor** — through
   *     `selectByteRange`, because everything the backend reports is UTF-8 and
   *     every one of those offsets is wrong by the number of accented characters
   *     before it if it reaches CodeMirror unconverted.
   *  3. **The document going away clears the tree.** Without it the panel keeps
   *     describing the file you just closed, which reads as the panel being broken
   *     rather than as it being stale.
   *
   * The caret direction is not here: it arrives through the editor's own `oncaret`
   * prop, so its host passes it. One line, at the place that already owns the
   * editor's props.
   */
  import { untrack } from 'svelte';

  import { astStore } from '$lib/stores/picus/ast.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

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
     * that is parsed — see below.
     */
    text: string;
  }

  let { editor, text }: Props = $props();

  const watching = $derived(picusUiStore.toolOpen && picusUiStore.toolSection === 'ast');

  /**
   * Parse the **editor's own document**, not the host's string.
   *
   * They are not the same on a CRLF file: CodeMirror normalises line endings when
   * it takes a document, so the host's raw text is one byte longer per line than
   * what the editor holds. Every offset computed from it is then short by the
   * number of lines above it — which reads as a tree that is subtly, increasingly
   * misaligned the further down the file you look, and is worst inside the
   * procedural blocks that live at the bottom of an update script.
   *
   * `selectByteRange` converts against that same document, so taking the text from
   * there is what makes the two ends agree. The host's `text` stays as the
   * dependency that tells us a keystroke happened.
   */
  $effect(() => {
    if (!watching || !editor) return;
    void text;
    astStore.follow(untrack(() => editor.getValue()));
  });

  $effect(() => {
    const request = astStore.selectRequest;
    if (!request || !editor) return;
    // `untrack`: the call reaches into the editor, and nothing it touches there
    // should become a dependency of this effect.
    untrack(() => editor?.selectByteRange(request.start, request.end));
  });

  $effect(() => () => astStore.clear());
</script>
