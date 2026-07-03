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
 */

/** One byte-span replacement (a subset of the wire `RenameEdit`). */
export interface ByteEdit {
  /** Start byte offset (inclusive). */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** Replacement text. */
  new_text: string;
}

/** Return `text` with every edit applied. A no-op when `edits` is empty. */
export function applyByteEdits(text: string, edits: ByteEdit[]): string {
  if (!edits.length) return text;
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  let bytes = enc.encode(text);
  // Right-to-left so each splice doesn't shift the offsets of edits not yet applied.
  const ordered = [...edits].sort((a, b) => b.start - a.start);
  for (const e of ordered) {
    const head = bytes.subarray(0, e.start);
    const ins = enc.encode(e.new_text);
    const tail = bytes.subarray(e.end);
    const merged = new Uint8Array(head.length + ins.length + tail.length);
    merged.set(head, 0);
    merged.set(ins, head.length);
    merged.set(tail, head.length + ins.length);
    bytes = merged;
  }
  return dec.decode(bytes);
}
