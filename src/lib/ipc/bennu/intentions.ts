/**
 * Bennu Alt+Enter intentions IPC — one round-trip that returns every quick-fix applicable at the
 * caret (parameterize logging, NP-safe equals, isEmpty()/boolean/negated-comparison
 * simplifications). Each offer is a byte-range edit the editor applies via `replaceByteRange`.
 *
 * Routes through the generic `bennu(...)` bridge to the `bennu_intentions_at` handler. Adding a new
 * intention is a change in the Rust `bennu-intentions` crate only — no new IPC.
 */

import { bennu } from '../rpc';

/** One applicable intention — a stable id, a human label, and a byte-range edit. Mirrors the BE
 *  `OfferWire`. */
export interface IntentionOffer {
  id: string;
  label: string;
  /** Start byte offset of the range to replace. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  replacement: string;
  /**
   * A non-edit action the editor dispatches instead of applying the range edit. Absent for a
   * plain edit.
   *
   * - `"move-to-package"` — move the file to the folder its `package` declares.
   * - `"rename-symbol"` — rename the symbol at `start` to `replacement`, straight away. Only ever
   *   sent for a declaration whose references cannot leave the file (a local, a parameter).
   * - `"rename-symbol-preview"` — the same rename, but through the preview modal, because it can
   *   reach other files.
   * - `"rename-file"` — rename the open file to `replacement` (a base name, never a path). No edit
   *   travels with it: this is the half of a file-name/type-name disagreement where the **type** is
   *   the name being kept.
   * - `"create-class"` — create the file for the type named in `replacement`, then open it.
   * - `"override-methods"` — open the implement/override picker.
   * - `"generate-constructor"` / `"generate-getters-setters"` — open the Generate modal in that mode.
   */
  action?: string;
  /**
   * The popup section it belongs to: a `fix` repairs a problem under the caret, an `intention`
   * changes code that is already right, a `generate` opens a generator. The backend decides — it is
   * the one that knows whether a diagnostic is involved.
   */
  category: 'fix' | 'intention' | 'generate';
  /** The candidate to suggest among several — the nearest import. Absent when none stands out. */
  preferred?: boolean;
  /**
   * The whole edit, when it touches more than one place — a rewrite and the import it needs.
   * Offsets are all into the buffer as it was when asked, applied as one transaction (one undo).
   * Absent for a single-range offer, which `start`/`end`/`replacement` describe on their own.
   */
  edits?: { start: number; end: number; text: string }[];
  /**
   * What to select once the edits are applied — a placeholder the offer wrote, to be typed over.
   *
   * `start`/`end` are UTF-8 **bytes into the inserted text of one edit**: `edits[edit]`, or for a
   * single-range offer (no `edits`) `replacement` as edit `0`. Relative to the edit rather than the
   * document so it means the same thing wherever that text lands; `selectionAfterEdits` in
   * `components/bennu/offer-selection.ts` resolves it. Absent for an offer that selects nothing.
   */
  select?: { edit: number; start: number; end: number };
}

/** A diagnostic as a quick-fix needs it: what kind, and where. */
export interface DiagRef {
  /** The stable kind slug — `unused-import`, `unhandled-checked-exception`. */
  code: string;
  /** Byte span in `source`. */
  start: number;
  end: number;
}

/**
 * Every intention applicable at byte `offset` in `source` (empty when none fit).
 *
 * `diagnostics` are the ones the editor is already showing: the offers include a **fix** for each
 * one under the caret, and passing them is what saves the backend revalidating the whole file to
 * answer a single keystroke. Only the `code` and the span travel — the fixes read the source, never
 * the message.
 *
 * Wire: `bennu_intentions_at` — `{ file, source, offset, diagnostics }`.
 */
export function intentionsAt(
  file: string,
  source: string,
  offset: number,
  diagnostics: readonly DiagRef[] = [],
): Promise<IntentionOffer[]> {
  return bennu('bennu_intentions_at', { args: { file, source, offset, diagnostics } });
}
