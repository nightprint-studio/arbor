/**
 * Whole-file source transforms — the ones you ask for by pressing a key, not by putting the caret
 * on something.
 *
 * Both route on the file rather than on an engine: a language with a server is formatted by its
 * server, Java by Bennu's own formatter, and the editor calls one function either way. That is why
 * these do not live in `lsp.ts` — nothing here is a language-server concept.
 *
 * Both return **edits**, not replacement text: applying them through CodeMirror keeps the change in
 * the undo history as one step and the caret in place, and a transform that touched three lines does
 * not mark the whole file dirty.
 *
 * Same conventions as the rest of `ipc/bennu`: every call wraps its fields under `{ args: … }` and
 * every offset is a **UTF-8 byte offset**.
 */

import { bennu } from '../rpc';
import type { SourceEdit } from '$lib/types/bennu';

/**
 * Format the whole buffer, whichever engine knows the language: a server where there is one,
 * Bennu's own formatter for Java.
 *
 * An empty list means the file is already formatted — not that nothing could format it.
 *
 * Wire: `bennu_format` — `{ file, source, tab_size?, insert_spaces? }`.
 */
export function formatBuffer(
  file: string,
  source: string,
  tabSize?: number,
  insertSpaces?: boolean,
): Promise<SourceEdit[]> {
  return bennu('bennu_format', {
    args: { file, source, tab_size: tabSize, insert_spaces: insertSpaces },
  });
}

/**
 * Drop the imports the file does not use and put what is left in order — one edit over the whole
 * import block.
 *
 * Java only; anything else answers with nothing. Never collapses a package to a wildcard and never
 * adds an import — see the backend module for why. An empty list means there was nothing to change.
 *
 * Wire: `bennu_optimize_imports` — `{ file, source }`.
 */
export function optimizeImports(file: string, source: string): Promise<SourceEdit[]> {
  return bennu('bennu_optimize_imports', { args: { file, source } });
}
