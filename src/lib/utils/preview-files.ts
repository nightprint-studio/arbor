/**
 * Files that open as a **preview** rather than as a buffer.
 *
 * The distinction is not "binary": a `.jar` is binary and Bennu refuses it outright, while
 * a `.png` and a `.docx` are binary and open perfectly well — they just do not open in a
 * text editor. What these have in common is that nothing decodes them into the source
 * cache, which is what makes them safe: a tab with no buffer behind it cannot be saved
 * back as an empty string by a stray Ctrl+S.
 *
 * One predicate, because three places have to agree about it — the store (which decides
 * whether to read the file at all), `saveText` (which refuses to write one) and the editor
 * (which decides what to mount). Three separate lists is how the store starts reading a
 * file the editor is already previewing.
 */

import { isImageFile } from './image-files';

/** A Word document. `.doc` is the old binary format and `docx-preview` cannot read it — it
 *  is a ZIP of XML that only the newer one is. So `.doc` is not here: it belongs with the
 *  files Bennu says it cannot open, which is at least true. */
export function isWordFile(path: string | null | undefined): boolean {
  if (!path) return false;
  const name = path.split(/[\\/]/).pop() ?? path;
  return /\.docx$/i.test(name);
}

/** Whether this file opens as a preview instead of as an editable buffer. */
export function opensAsPreview(path: string | null | undefined): boolean {
  return isImageFile(path) || isWordFile(path);
}
