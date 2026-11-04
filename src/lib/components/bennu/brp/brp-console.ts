/**
 * The console's pure parts: reading the params box, ranking method names, keeping the history.
 * The store owns the calls; nothing here touches the game.
 */

export interface ConsoleEntry {
  method: string;
  /** The params text as typed (empty = none). */
  params: string;
}

export const HISTORY_LIMIT = 50;
export const SUGGESTION_LIMIT = 12;

/** The params box as a value: empty is "no params", anything else must be JSON. */
export function parseParams(text: string): { ok: true; value: unknown } | { ok: false; error: string } {
  const trimmed = text.trim();
  if (trimmed === '') return { ok: true, value: undefined };
  try {
    return { ok: true, value: JSON.parse(trimmed) };
  } catch (e) {
    return { ok: false, error: `Params are not valid JSON: ${e instanceof Error ? e.message : String(e)}` };
  }
}

/**
 * Method names for what was typed: names starting with it first, then names containing it, each group
 * alphabetical. Nothing is offered for an empty box (the full list is the game's whole API and is
 * reachable through the history and `rpc.discover`), nor once the box holds exactly one suggestion.
 */
export function suggestMethods(methods: readonly string[], typed: string): string[] {
  const needle = typed.trim().toLowerCase();
  if (needle === '') return [];
  const starts: string[] = [];
  const contains: string[] = [];
  for (const m of methods) {
    const lower = m.toLowerCase();
    if (lower === needle) continue;
    if (lower.startsWith(needle)) starts.push(m);
    else if (lower.includes(needle)) contains.push(m);
  }
  return [...starts.sort(), ...contains.sort()].slice(0, SUGGESTION_LIMIT);
}

/** Newest first; a repeat of an earlier call moves to the top instead of piling up. */
export function pushHistory(history: readonly ConsoleEntry[], entry: ConsoleEntry): ConsoleEntry[] {
  const rest = history.filter((h) => h.method !== entry.method || h.params !== entry.params);
  return [entry, ...rest].slice(0, HISTORY_LIMIT);
}
