/**
 * The composer's paste handling: the DOM side of `assistant-paste.ts`.
 *
 * `planPaste` decides what should go into the box; this applies it to the textarea in a way that keeps
 * the browser's own undo stack (one Ctrl+Z takes the whole block back out), and carries the
 * Ctrl+Shift+V verb — paste exactly what is on the clipboard, with no fence.
 */
import { MAX_PASTE_CHARS, planPaste } from './assistant-paste';

export interface PasteDeps {
  enabled: () => boolean;
  /** The file open in the editor, for the language when the content does not say. */
  activePath: () => string | null;
  notify: (message: string) => void;
}

/** Type `text` at the selection as if the user had: undoable, and it fires `input` so bindings follow. */
function insertUndoable(el: HTMLTextAreaElement, text: string) {
  el.focus();
  if (document.execCommand('insertText', false, text)) return;
  // No execCommand (or it refused): still correct, only not undoable.
  el.setRangeText(text, el.selectionStart, el.selectionEnd, 'end');
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

export function createPasteHandlers(deps: PasteDeps) {
  // Set by Ctrl+Shift+V, consumed by the paste that follows it.
  let plainOnce = false;

  function onkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === 'v') {
      plainOnce = true;
      setTimeout(() => { plainOnce = false; }, 600);
    }
  }

  function onpaste(e: ClipboardEvent) {
    const el = e.target;
    if (!(el instanceof HTMLTextAreaElement)) return;
    const pasted = e.clipboardData?.getData('text/plain') ?? '';
    if (!pasted) return;

    const plain = plainOnce;
    plainOnce = false;
    const before = el.value.slice(0, el.selectionStart);
    const after = el.value.slice(el.selectionEnd);

    const plan = plain
      ? ({ action: 'native' } as const)
      : planPaste({ pasted, before, after, activePath: deps.activePath(), enabled: deps.enabled() });

    if (plan.action === 'insert') {
      e.preventDefault();
      insertUndoable(el, plan.text);
      if (plan.truncated) deps.notify(`The pasted code was cut at ${Math.round(MAX_PASTE_CHARS / 1000)} KB.`);
      return;
    }
    if (pasted.length > MAX_PASTE_CHARS) {
      e.preventDefault();
      insertUndoable(el, pasted.slice(0, MAX_PASTE_CHARS));
      deps.notify(`The pasted text was cut at ${Math.round(MAX_PASTE_CHARS / 1000)} KB.`);
    }
  }

  return { onkeydown, onpaste };
}
