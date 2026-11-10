/**
 * Where the caret is inside a Patina string — the one question completion needs answered locally, before it asks
 * the backend.
 *
 * A `.ptn` string has three places a caret can be, and each wants a different list:
 *
 * | caret | what completes |
 * |---|---|
 * | outside a string | the usual: names, members, paths |
 * | in the **text** of a string | an id, if the string is the argument of a catalog form (`t!("ui:…")`, `block!("…")`); nothing otherwise |
 * | inside an interpolation **hole** `"{p.x}"` | a variable, parameter or constant, and after a dot a field |
 *
 * The backend decides *what* (it owns the catalogs and the types); this decides *how the editor treats the token*:
 * an id is `ui:avvio.solo` — one word to the user, three to an identifier regex — while a name in a hole is a plain
 * identifier. Getting the token wrong is the bug that made the text keys complete as `ui:ui:avvio.solo`.
 *
 * One line only: a Patina string does not span lines (an unterminated one ends where the line does), so the text
 * before the caret **on its line** is all there is to read. The rules are the lexer's: `\` escapes, `{{` and `}}`
 * are literal braces, a raw string (`r"…"`) has no holes.
 */

export type StringContext = 'none' | 'text' | 'hole';

/** What surrounds the caret, given the text of its line **before** the caret. */
export function stringContext(before: string): StringContext {
  let inStr = false;
  let raw = false;
  let hole = false;
  for (let i = 0; i < before.length; i++) {
    const c = before[i];
    if (!inStr) {
      if (c === '/' && before[i + 1] === '/') return 'none';
      if (c === '"') {
        inStr = true;
        hole = false;
        raw = before[i - 1] === 'r' && !/[\p{L}\p{N}_]/u.test(before[i - 2] ?? '');
      }
      continue;
    }
    if (c === '\\') {
      i++;
      continue;
    }
    if (c === '"') {
      inStr = false;
      continue;
    }
    if (raw) continue;
    if (c === '{') {
      if (before[i + 1] === '{') i++;
      else hole = true;
    } else if (c === '}') {
      if (before[i + 1] === '}') i++;
      else hole = false;
    }
  }
  if (!inStr) return 'none';
  return hole ? 'hole' : 'text';
}
