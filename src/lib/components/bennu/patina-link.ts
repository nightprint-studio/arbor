/**
 * The Ctrl/Cmd-hover link of a Patina (`.ptn`) line: the one identifier under the pointer.
 *
 * The editor's generic link range is a run of `[A-Za-z0-9_$/.\-]`, which is right for a Struts path
 * (`/do/Category/viewTree`) and wrong here: `gen.set_if_air` is two names, a receiver and a method, and
 * `ui::pausa::RIPRENDI` is three path segments. Underlining the whole run promises a click on `gen` that lands on
 * `set_if_air`, or the other way round. Here the dot and the `::` are separators, so the link is the segment the
 * pointer is on.
 *
 * Pure, over one line of text, so it can be tested without an editor. Strings and `//` comments give no link
 * (a name inside them is prose, not a reference the language follows) — except the string argument of a form
 * (`ui!("scope", "id")`, `t!("mod:key")`, `block!("x")`), whose whole content names a catalog entry: that is one link, hover and click alike.
 */

const NAME = /[\p{L}\p{N}_]/u;

/** The spans of a line that are a string literal or a comment, as `[from, to)` pairs. `//` ends the line; Patina has no block comments. */
function inertSpans(text: string): Array<[number, number]> {
  const spans: Array<[number, number]> = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '/' && text[i + 1] === '/') {
      spans.push([i, text.length]);
      break;
    }
    if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
      // An unterminated string runs to the end of the line, as the stream mode colours it.
      const end = Math.min(j + 1, text.length);
      spans.push([i, end]);
      i = end;
      continue;
    }
    i++;
  }
  return spans;
}

/** Whether the string opening at column `open` is an argument of a form: the innermost parenthesis still open there is the `(` of a `name!(`. */
function inForm(text: string, open: number): boolean {
  const stack: boolean[] = [];
  const spans = inertSpans(text.slice(0, open));
  for (let i = 0; i < open; i++) {
    const span = spans.find(([a, b]) => i >= a && i < b);
    if (span) {
      i = span[1] - 1;
      continue;
    }
    if (text[i] === '(') stack.push(i > 1 && text[i - 1] === '!' && NAME.test(text[i - 2]));
    else if (text[i] === ')') stack.pop();
  }
  return stack.length > 0 && stack[stack.length - 1];
}

/**
 * The link range, in columns of `text`, for a pointer at column `rel` (the gap before the character at `rel`), or `null`.
 *
 * The character to the right of the pointer decides; at the end of a name the one on the left does, so `gen|.set` is `gen`
 * and `gen.|set` is `set`. A form keeps its `!` (`block!`), as the colouring does. A word that starts with a digit is a number.
 */
export function patinaLinkRange(text: string, rel: number): { from: number; to: number } | null {
  const r = Math.max(0, Math.min(rel, text.length));
  // A string argument of a form: its content, whole. Any other string is inert.
  for (const [a, b] of inertSpans(text)) {
    if (text[a] !== '"') continue;
    const closed = b - a >= 2 && text[b - 1] === '"';
    const inner = { from: a + 1, to: closed ? b - 1 : b };
    if (r >= inner.from && r <= inner.to && r > a && inner.to > inner.from) {
      return inForm(text, a) ? inner : null;
    }
  }
  const at = r < text.length && NAME.test(text[r]) ? r : r > 0 && NAME.test(text[r - 1]) ? r - 1 : -1;
  if (at < 0) return null;
  if (inertSpans(text).some(([a, b]) => at >= a && at < b)) return null;

  let from = at;
  let to = at + 1;
  while (from > 0 && NAME.test(text[from - 1])) from--;
  while (to < text.length && NAME.test(text[to])) to++;
  if (/\d/.test(text[from])) return null;
  // `block!(…)`: the name and its bang, but not `!=`.
  if (text[to] === '!' && text[to + 1] !== '=') to++;
  return { from, to };
}
