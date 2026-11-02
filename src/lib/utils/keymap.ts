/**
 * A declarative key table for the product shells' `onkeydown` handlers.
 *
 * Replaces a hand-ordered cascade of `if (mod && key === 'x') { …; e.preventDefault(); return; }`
 * with a list of rules evaluated in the order they are declared. It owns no bindings of
 * its own: a rule either carries a plain predicate (the shell's fixed chords, built with
 * {@link chord}) or a `Keybinding` that goes through the existing {@link matchesBinding}
 * so a user remap in `keybindingsStore` keeps working.
 *
 * Semantics, chosen to be a 1:1 translation of the cascades this replaces:
 *  - rules are tried top to bottom; the FIRST rule whose `match` and `when` both hold runs
 *    and the dispatch stops — order is precedence, exactly as it was in the `if` chain;
 *  - `preventDefault` defaults to true and is applied AFTER `run`, as the cascades did;
 *  - `run` may return {@link DECLINE} (a runtime condition only it can evaluate, e.g.
 *    "is there an active editor?"): nothing is prevented and evaluation continues with the
 *    next rule, like an `if` whose inner condition failed. A sentinel rather than `false`,
 *    so an arrow that merely forwards a store method returning a boolean cannot decline by
 *    accident;
 *  - guards that must hold BEFORE any rule (modal open, wrong window) stay in the caller:
 *    they are not key rules, and keeping them out is what keeps this file tiny.
 */
import { matchesBinding, type Keybinding } from '$lib/utils/keybindings';

/** Returned by a rule's `run` to say "not mine after all" — see the module comment. */
export const DECLINE = Symbol('keymap.decline');

export interface KeyRule {
  /** Stable name — what the rule does, for readability and for exporting the table. */
  id: string;
  /** A predicate (see {@link chord}) or a store-backed binding. */
  match: ((e: KeyboardEvent) => boolean) | Keybinding;
  /** Extra applicability condition; when false the rule is skipped and evaluation continues. */
  when?: (e: KeyboardEvent) => boolean;
  /** The action. Return {@link DECLINE} to let later rules see the event. */
  run: (e: KeyboardEvent) => unknown;
  /** Default true. Set false for rules that must let the browser keep the key. */
  preventDefault?: boolean;
}

/** Ctrl on Windows/Linux, Cmd on macOS — the same fold `matchesBinding` applies. */
export function isMod(e: KeyboardEvent): boolean {
  return e.ctrlKey || e.metaKey;
}

export interface ChordSpec {
  /** `true`/`false` require Ctrl-or-Cmd held / not held; omit for "don't care". */
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
  /** `KeyboardEvent.key`, compared case-insensitively; an array matches any of them. */
  key?: string | string[];
  /** `KeyboardEvent.code` — layout-independent (digits on non-US layouts). */
  code?: string;
}

/**
 * Build a predicate from a tri-state chord. An omitted modifier is "don't care", NOT
 * "must be off": the cascades this table replaces were inconsistent about it (some tested
 * `!e.shiftKey`, some ignored it), and flipping that would silently change which events
 * a rule claims. Write `shift: false` wherever the original said `!e.shiftKey`.
 */
export function chord(spec: ChordSpec): (e: KeyboardEvent) => boolean {
  const keys = spec.key === undefined ? null : [spec.key].flat().map((k) => k.toLowerCase());
  return (e) => {
    if (spec.mod !== undefined && isMod(e) !== spec.mod) return false;
    if (spec.shift !== undefined && e.shiftKey !== spec.shift) return false;
    if (spec.alt !== undefined && e.altKey !== spec.alt) return false;
    if (keys && !keys.includes(e.key.toLowerCase())) return false;
    if (spec.code !== undefined && e.code !== spec.code) return false;
    return true;
  };
}

/** Run the first applicable rule. Returns true when one handled the event. */
export function dispatchKeymap(e: KeyboardEvent, rules: readonly KeyRule[]): boolean {
  for (const rule of rules) {
    const hit = typeof rule.match === 'function' ? rule.match(e) : matchesBinding(e, rule.match);
    if (!hit) continue;
    if (rule.when && !rule.when(e)) continue;
    if (rule.run(e) === DECLINE) continue;
    if (rule.preventDefault !== false) e.preventDefault();
    return true;
  }
  return false;
}
