/**
 * A short fingerprint of a document's text — which document an undo history was recorded against.
 *
 * A history is a list of changes at positions; applied to a text other than the one it was recorded on, it
 * edits the wrong characters. So a history travels with the hash of its document, and is dropped unless the
 * document it is about to be given hashes the same. FNV-1a over the UTF-16 units, plus the length folded in
 * so two texts of different size never share one through a lucky collision.
 */
export function docHash(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash ^ text.length) >>> 0;
}
