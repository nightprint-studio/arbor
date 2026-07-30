/**
 * Finding the placeholders in a buffer — the "which values does this statement
 * want" question, answered before anything is sent.
 *
 * ## Why this reads tokens and not the text
 *
 * A placeholder is a lexical thing, and every way of spotting one with a regular
 * expression over raw SQL is wrong in a way somebody will hit:
 *
 * * `'$1 off'` and `-- try :codice` are a string and a comment, not parameters;
 * * `id::text` is PostgreSQL's cast, not a `:text` bind;
 * * `v := 1` is PL/SQL assignment;
 * * `:NEW.stato` inside a trigger body is the row alias, not something anyone can
 *   supply a value for.
 *
 * {@link scanSql} already knows all of that — strings, comments and dollar-quoted
 * bodies are consumed whole, `::` is excluded before `:name` is considered, and
 * `:=` never begins an identifier — so working from its `param` tokens gets every
 * one of those right for free. That is also why {@link inLiteral} is not consulted
 * here: a token inside a literal does not exist as a token, because the literal is
 * the token.
 *
 * ## Order, twice
 *
 * A slot carries both the order it is *shown* in (first appearance — the order the
 * user reads the statement in) and the position it is *bound* at. On PostgreSQL the
 * two differ whenever somebody writes `$2` before `$1`, and binding by appearance
 * there would swap two values without a word.
 */

import type { Dialect } from '$lib/types/picus';
import { scanSql } from './tokens';

/** One placeholder a statement wants a value for. */
export interface BindSlot {
  /** As the user wrote it: `:CODICE` on Oracle, `$1` on PostgreSQL. */
  label: string;
  /** 1-based position in the bind list — `$2` binds second wherever it appears. */
  index: number;
}

/** `$1`, `$17` — PostgreSQL's positional placeholder. */
const POSITIONAL = /^\$(\d+)$/;
/** `:CODICE` — Oracle's named placeholder. */
const NAMED = /^:([A-Za-z_][A-Za-z0-9_$#]*)$/;

/**
 * Row aliases inside a PL/SQL trigger body. They look exactly like binds and are
 * not: nobody supplies them, the server does. Asking for them would put two boxes
 * on screen that must be left empty for the trigger to compile.
 */
const ROW_ALIASES = new Set(['NEW', 'OLD', 'PARENT']);

/**
 * Every distinct placeholder in `sql`, in the order it first appears.
 *
 * Empty for a dialect's *other* notation on purpose — `$1` means nothing on Oracle
 * and `:name` is a host variable Picus does not bind on PostgreSQL — so a buffer
 * that carries the wrong one runs unchanged and the server says why.
 */
export function findBindSlots(sql: string, dialect: Dialect): BindSlot[] {
  const { scan } = scanSql(sql, dialect);
  const out: BindSlot[] = [];
  const seen = new Set<string>();

  for (const token of scan.tokens) {
    if (token.kind !== 'param') continue;
    const slot = dialect === 'oracle' ? named(token.text, out.length) : positional(token.text);
    if (!slot) continue;
    // Case-insensitively: SQL folds case, so `:codice` and `:CODICE` are one value
    // and asking for it twice would be asking the same question twice.
    const key = slot.label.toUpperCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(slot);
  }
  return out;
}

function positional(text: string): BindSlot | null {
  const m = POSITIONAL.exec(text);
  if (!m) return null;
  const index = Number(m[1]);
  // `$0` is not a placeholder — PostgreSQL numbers from one, and treating it as
  // one would put a value at index -1 of the bind list.
  return index >= 1 ? { label: text, index } : null;
}

function named(text: string, found: number): BindSlot | null {
  const m = NAMED.exec(text);
  if (!m || ROW_ALIASES.has(m[1].toUpperCase())) return null;
  return { label: text, index: found + 1 };
}

/**
 * The positional bind list a statement's slots need, read out of the values the
 * user supplied by label.
 *
 * Sized by the **highest** index rather than by the number of slots: a statement
 * that uses `$1` and `$3` still takes three values, and the missing one is sent as
 * NULL. That is not a guess — PostgreSQL rejects a statement whose parameter list
 * is short, and a NULL the user can see in the grid is a better answer than a
 * failure they cannot place.
 */
export function toBindList(
  slots: BindSlot[],
  valueOf: (label: string) => string | null,
): (string | null)[] {
  const size = slots.reduce((n, s) => Math.max(n, s.index), 0);
  const list: (string | null)[] = new Array(size).fill(null);
  for (const slot of slots) list[slot.index - 1] = valueOf(slot.label);
  return list;
}
