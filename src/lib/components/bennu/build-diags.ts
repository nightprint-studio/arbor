/**
 * The compiler's own diagnostics, placed in the buffer.
 *
 * A build failure already knows exactly which line broke — the Build panel lists them and
 * clicking one jumps there. But finding them meant going to the panel and coming back for
 * each, which is the round trip the editor's right-edge stripe exists to remove: with the
 * compiler's errors in the same lint set as everything else, `cargo build` failing paints
 * red marks down the scrollbar of the file you are already looking at.
 *
 * The conversion is the whole job, and it is not free: a {@link BuildDiagnostic} is a
 * **line and column** (what a compiler prints), while the editor takes **UTF-8 byte
 * offsets** (what the rest of Bennu's backend speaks). The mapping has to be done against
 * the same text the editor is showing, which is why this takes the buffer rather than
 * reading the file.
 */

import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor';
import type { BuildDiagnostic } from '$lib/types/bennu';

/** Forward slashes, lower case — the two sides of the seam disagree on both. The backend
 *  emits `/`, Windows hands us `\`, and a compiler may print either. */
function norm(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/** Whether `d.file` names `file`.
 *
 *  Compilers do not agree on what a path is: `javac` prints an absolute one, `cargo check`
 *  prints one **relative to the directory cargo ran in** — the workspace root. So a relative
 *  path is resolved against the root before comparing, rather than matched by suffix: in a
 *  workspace, `src/lib.rs` is the name of one file per crate, and a suffix match would put
 *  every crate's errors in whichever `lib.rs` happens to be open. */
function isFile(reported: string, file: string, root: string): boolean {
  const r = norm(reported);
  const target = norm(file);
  if (r === target) return true;
  const absolute = /^([a-z]:)?\//.test(r);
  return !absolute && `${norm(root)}/${r}` === target;
}

/** `note` is the compiler's third level and reads as information, not as a problem. */
function severityOf(s: string): EditorDiagnostic['severity'] {
  if (s === 'error') return 'error';
  if (s === 'warning') return 'warning';
  return 'info';
}

/**
 * Byte offset of the start of each 1-based line, plus a final entry for end-of-text.
 *
 * Built once per conversion rather than per diagnostic: a failing build can report a
 * hundred problems in one file, and re-scanning the buffer for each would be quadratic in
 * exactly the case where it hurts.
 */
function lineStarts(text: string): number[] {
  const enc = new TextEncoder();
  const starts = [0, 0]; // index 0 unused; line 1 starts at byte 0.
  let bytes = 0;
  for (const line of text.split('\n')) {
    bytes += enc.encode(line).length + 1; // + the '\n' itself
    starts.push(bytes);
  }
  return starts;
}

/**
 * The subset of `diagnostics` that belongs to `file`, as editor diagnostics over `text`.
 *
 * A diagnostic with no file, or one naming a different file, is dropped — it belongs to
 * another buffer and the Build panel is where it is read. A diagnostic whose line is past
 * the end of the buffer is dropped too: the file has been edited since the build, and a
 * mark at a position that no longer exists is worse than no mark.
 */
export function buildDiagnosticsFor(
  root: string,
  file: string,
  text: string,
  diagnostics: BuildDiagnostic[],
): EditorDiagnostic[] {
  const mine = diagnostics.filter((d) => d.file && d.line && isFile(d.file, file, root));
  if (mine.length === 0) return [];

  const lines = text.split('\n');
  const starts = lineStarts(text);
  const enc = new TextEncoder();
  const out: EditorDiagnostic[] = [];

  for (const d of mine) {
    const line = d.line as number;
    if (line < 1 || line > lines.length) continue;
    const src = lines[line - 1] ?? '';
    // A column is 1-based and counts CHARACTERS; the offset counts bytes. Slicing the
    // prefix and measuring it is the only conversion that is right for both.
    const col = Math.max(1, Math.min(d.col ?? 1, src.length + 1));
    const from = starts[line] + enc.encode(src.slice(0, col - 1)).length;
    // No end column in a compiler line, so the mark runs to the end of the line — which
    // is where the eye goes anyway, and never lands mid-character.
    const to = starts[line] + enc.encode(src).length;
    out.push({
      from,
      to: Math.max(from, to),
      severity: severityOf(d.severity),
      // Named, because in a lint tooltip next to a live analysis warning "who said this"
      // is the first question — and the answer changes what you do about it.
      message: `${d.message}  (build)`,
    });
  }
  return out;
}
