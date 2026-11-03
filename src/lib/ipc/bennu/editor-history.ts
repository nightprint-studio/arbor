/**
 * The undo/redo history of an open tab, kept on disk across a restart.
 *
 * Handlers live in `crates/products/bennu/runtime/be/src/editor_history.rs`. The backend only stores
 * the JSON; `hash` is the editor's hash of the text the history was recorded against, and the editor
 * is the one that refuses a history that does not fit its document.
 */

import { bennu } from '../rpc';

export interface StoredHistory {
  hash: number;
  history: unknown;
}

export const saveEditorHistory = (root: string, file: string, hash: number, history: unknown): Promise<void> =>
  bennu('bennu_editor_history_save', { root, file, hash, history });

/** The kept history of `file`, or `null` when there is none. */
export const loadEditorHistory = (root: string, file: string): Promise<StoredHistory | null> =>
  bennu('bennu_editor_history_load', { root, file });

/** Forget the history of `file` — its tab was closed. */
export const forgetEditorHistory = (root: string, file: string): Promise<void> =>
  bennu('bennu_editor_history_forget', { root, file });
