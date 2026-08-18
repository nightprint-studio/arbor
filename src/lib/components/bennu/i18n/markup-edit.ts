/**
 * What the toolbar writes, computed rather than typed.
 *
 * ## One replacement, not two insertions
 *
 * Wrapping a selection is naturally two edits — a prefix before it and a `}` after. Done that way
 * it is also two undo steps, and the second one is at an offset the first has moved. So every button
 * here produces a **single** replacement of the selected range with the whole construct, which makes
 * it one atomic change, one undo step, and immune to the ordering problem.
 *
 * ## Offsets are file bytes throughout
 *
 * The caller hands over the selection in **file** byte offsets (what `content_start` plus a span
 * inside the value comes to) and gets file byte offsets back, so nothing here has to know where the
 * value sits or convert between UTF-16 and UTF-8. The selected *text* travels as a string and is put
 * back verbatim — it is already markup, and re-encoding it would be the one way to corrupt it.
 *
 * ## Where the caret lands
 *
 * With a selection, the construct closes around it and the selection is kept: the next button wraps
 * the same words again, which is how `$red.bold{…}` gets written as two presses. With no selection
 * the caret lands **inside the braces**, because a construct with nothing in it is a construct you
 * were about to type into.
 */

/** What to write. */
export type Insert =
  /** `$a.b{…}` — a chain, so the picker can offer `red.bold` as one item. */
  | { what: 'style'; names: readonly string[] }
  /** `@key{…}`. */
  | { what: 'glossary'; key: string }
  /** `~name(args){…}`, with either half optional — args and body are both optional in the grammar. */
  | { what: 'control'; name: string; args?: string; body?: boolean }
  /** `{name}` — never wraps: a placeholder has no content, it *is* the content. */
  | { what: 'placeholder'; name: string };

/** The selection the construct is being written around, in file byte offsets. */
export interface Selection {
  start: number;
  end: number;
  /** The selected text, verbatim. Empty when the selection is. */
  text: string;
}

/** A single replacement, plus where the selection should be afterwards. All offsets are file bytes. */
export interface MarkupEdit {
  start: number;
  end: number;
  text: string;
  selectStart: number;
  selectEnd: number;
}

/** The characters the markup gives meaning to, and which therefore need a `\` to be literal. */
const SPECIAL = /[$@~{}\\]/g;

/**
 * A name as the grammar will accept it: the characters a name may contain, and nothing else.
 *
 * Not cosmetic. A style called `red bold` writes `$red bold{…}`, whose name ends at the space — so
 * the construct silently becomes the style `red` applied to text beginning with `bold`. Dropping the
 * offending characters produces something wrong in a way that is *visible*, which is the better
 * failure of the two. Dots and colons survive because the grammar uses them: `$mod:red.bold`.
 */
export function safeName(raw: string): string {
  return raw.trim().replace(/[^A-Za-z0-9_.:-]/g, '');
}

/** A placeholder name: as {@link safeName}, minus the two characters that only a style may use. */
export function safeParam(raw: string): string {
  return raw.trim().replace(/[^A-Za-z0-9_]/g, '');
}

/** Escape text so the markup reads it as literal characters. */
export function escapeMarkup(text: string): string {
  return text.replace(SPECIAL, (c) => `\\${c}`);
}

const enc = new TextEncoder();
const byteLen = (s: string) => enc.encode(s).length;

/**
 * The edit one toolbar button makes, or `null` when there is nothing to write — an empty name, which
 * is what an unfinished picker hands over.
 */
export function markupEdit(insert: Insert, sel: Selection): MarkupEdit | null {
  const built = build(insert, sel.text);
  if (!built) return null;
  const { prefix, middle, suffix, caretInside } = built;

  const text = prefix + middle + suffix;
  const afterPrefix = sel.start + byteLen(prefix);
  return {
    start: sel.start,
    end: sel.end,
    text,
    // With content, keep it selected so the next button wraps the same words. Without, sit where the
    // typing goes.
    selectStart: afterPrefix,
    selectEnd: caretInside ? afterPrefix : afterPrefix + byteLen(middle),
  };
}

interface Built {
  prefix: string;
  middle: string;
  suffix: string;
  /** True when `middle` is empty and the caret should end up between the braces. */
  caretInside: boolean;
}

function build(insert: Insert, selected: string): Built | null {
  const wrap = (prefix: string, middle: string): Built => ({
    prefix,
    middle,
    suffix: '}',
    caretInside: middle.length === 0,
  });

  switch (insert.what) {
    case 'style': {
      const names = insert.names.map(safeName).filter(Boolean);
      if (!names.length) return null;
      return wrap(`$${names.join('.')}{`, selected);
    }
    case 'glossary': {
      const key = safeName(insert.key);
      if (!key) return null;
      return wrap(`@${key}{`, selected);
    }
    case 'control': {
      const name = safeName(insert.name);
      if (!name) return null;
      // Arguments are positional, raw and never interpreted — `0.8` and `amp=2` both pass through —
      // so they are trimmed and otherwise left exactly as written.
      const args = insert.args?.trim() ? `(${insert.args.trim()})` : '';
      // A control with no body is a legal and common thing (`~sleep(0.8)` paces the line it is in),
      // so it is offered as such rather than as an empty pair of braces to delete.
      const wantsBody = insert.body ?? (selected.length > 0 || !args);
      if (!wantsBody) {
        return { prefix: `~${name}${args}`, middle: '', suffix: '', caretInside: false };
      }
      return wrap(`~${name}${args}{`, selected);
    }
    case 'placeholder': {
      const name = safeParam(insert.name);
      if (!name) return null;
      // A placeholder replaces the selection rather than wrapping it: there is nowhere in `{name}`
      // for the old text to go, and silently deleting it would be worse than replacing it visibly.
      return { prefix: `{${name}}`, middle: '', suffix: '', caretInside: false };
    }
  }
}
