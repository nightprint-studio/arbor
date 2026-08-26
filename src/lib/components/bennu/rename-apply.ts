/**
 * Apply byte-offset edits to a source string (the FE side of Bennu rename apply).
 *
 * The backend's rename edits are keyed by **UTF-8 byte offsets** (the whole engine
 * works in bytes); CodeMirror / JS strings are UTF-16. Rather than convert offsets
 * per edit, we splice on the encoded byte array and decode once — correct for any
 * file, open or not, without touching a CodeMirror view.
 *
 * Edits are assumed non-overlapping (the planner guarantees this per file); we apply
 * them right-to-left so earlier offsets stay valid as later ones are rewritten.
 *
 * ## Every edit is checked against what it says it is replacing
 *
 * A `RenameEdit` carries `old`: the exact text the backend saw at `[start, end)`. When it is
 * present and does **not** match the bytes actually there, the edit is dropped rather than applied.
 *
 * This is not defensive padding — it is the check that turns a whole family of coordinate bugs from
 * silent code corruption into a no-op. A plan computed against CRLF bytes and applied to an
 * LF-normalised buffer drifts one byte per preceding line, so its edits splice a name into the
 * middle of a comment, a string, or an unrelated identifier, and the result still compiles often
 * enough that nobody notices immediately. The guard was already in the payload and simply was not
 * being read.
 */

/** One byte-span replacement (a subset of the wire `RenameEdit`). */
export interface ByteEdit {
  /** Start byte offset (inclusive). */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** Replacement text. */
  new_text: string;
  /** The exact text the backend saw at `[start, end)`. Checked before the splice when present. */
  old?: string;
}

/** How an edit was rejected, for a caller that wants to report rather than swallow it. */
export interface RejectedEdit {
  edit: ByteEdit;
  /** What was actually at the edit's range. */
  found: string;
}

/** The outcome of applying a set of edits: the new text, and anything that did not fit it. */
export interface ApplyResult {
  text: string;
  rejected: RejectedEdit[];
}

/**
 * Apply every edit whose `old` still matches, and report the ones that did not.
 *
 * Rejecting is the safe half of the trade: a dropped rename leaves code that compiles and a name
 * still spelled the old way, which is visible and fixable. An applied wrong-offset rename leaves
 * code nobody wrote.
 */
export function applyByteEditsChecked(text: string, edits: ByteEdit[]): ApplyResult {
  const rejected: RejectedEdit[] = [];
  if (!edits.length) return { text, rejected };
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  let bytes = enc.encode(text);
  // Right-to-left so each splice doesn't shift the offsets of edits not yet applied.
  const ordered = [...edits].sort((a, b) => b.start - a.start);
  for (const e of ordered) {
    if (e.start > e.end || e.end > bytes.length) {
      rejected.push({ edit: e, found: '' });
      continue;
    }
    const found = dec.decode(bytes.subarray(e.start, e.end));
    // `old` is optional so a caller with a hand-built edit still works; when the backend sent it,
    // it is authoritative.
    if (e.old !== undefined && e.old !== found) {
      rejected.push({ edit: e, found });
      continue;
    }
    const head = bytes.subarray(0, e.start);
    const ins = enc.encode(e.new_text);
    const tail = bytes.subarray(e.end);
    const merged = new Uint8Array(head.length + ins.length + tail.length);
    merged.set(head, 0);
    merged.set(ins, head.length);
    merged.set(tail, head.length + ins.length);
    bytes = merged;
  }
  return { text: dec.decode(bytes), rejected };
}

/** Return `text` with every edit applied. A no-op when `edits` is empty. */
export function applyByteEdits(text: string, edits: ByteEdit[]): string {
  return applyByteEditsChecked(text, edits).text;
}
