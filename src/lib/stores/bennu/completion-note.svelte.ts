/**
 * The one line the completion machinery says out loud.
 *
 * ## Why a store for a transient string
 *
 * An explicit completion request that finds nothing renders **exactly** like a shortcut that
 * never arrived: no popup, no error, no movement. The two have opposite fixes — one is a
 * keyboard conflict outside the app, the other a cold index or a caret somewhere the engine has
 * nothing to say about — and neither is diagnosable while they look the same. This is what makes
 * them look different.
 *
 * Only ever set on an **explicit** request. While typing, silence is the correct answer, and a
 * status line blinking on every keystroke would be furniture nobody reads.
 *
 * It is a store and not component state because the completion sources are plain modules
 * (`lsp-lang.ts`, `java-lang.ts`) with no component to call into — the same reason the
 * diagnostics and the LSP status are stores.
 */
function createCompletionNoteStore() {
  /** Long enough to read a short sentence, short enough not to become furniture. */
  const LINGER_MS = 3000;

  let note = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | null = null;

  return {
    get note() {
      return note;
    },
    /** Say something once. A newer message replaces an older one rather than queueing. */
    say(message: string) {
      note = message;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        note = null;
        timer = null;
      }, LINGER_MS);
    },
    /** Drop the message early — used when a popup does open, which answers it better. */
    clear() {
      if (timer) clearTimeout(timer);
      timer = null;
      note = null;
    },
  };
}

export const completionNoteStore = createCompletionNoteStore();
