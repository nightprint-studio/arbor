/**
 * Bevy states, read out of the type registry: a state machine is a resource `State<T>` (the current value)
 * beside a resource `NextState<T>` (set it to move), both over an enum `T`. Nothing here touches the game —
 * it only recognises the pair and the shapes their values come in.
 */

export interface StateEntry {
  /** The enum the state is over: `my_game::AppState`. */
  enumPath: string;
  /** The `State<T>` resource: read it for the current value. */
  statePath: string;
  /** The `NextState<T>` resource, when the game registered it: write it to change state. */
  nextPath: string | null;
}

const STATE = /(?:^|::)State<(.+)>$/;
const NEXT = /(?:^|::)NextState<(.+)>$/;

/** Pair up `State<T>` and `NextState<T>` resource type paths; one entry per `T`, alphabetical. */
export function parseStateTypes(typePaths: readonly string[]): StateEntry[] {
  const byEnum = new Map<string, StateEntry>();
  const entry = (enumPath: string) => {
    let e = byEnum.get(enumPath);
    if (!e) byEnum.set(enumPath, (e = { enumPath, statePath: '', nextPath: null }));
    return e;
  };
  for (const path of typePaths) {
    const s = STATE.exec(path);
    const n = NEXT.exec(path);
    if (s) entry(s[1]).statePath = path;
    else if (n) entry(n[1]).nextPath = path;
  }
  return [...byEnum.values()].filter((e) => e.statePath).sort((a, b) => a.enumPath.localeCompare(b.enumPath));
}

/**
 * The current variant name out of a `State<T>` value, whatever shape reflection gave it: `"Playing"`,
 * `{"Paused": {…}}`, `["Playing"]`, `{"0": "Playing"}` (the newtype wrapper is peeled first).
 */
export function currentStateName(value: unknown): string | null {
  let v = value;
  for (let i = 0; i < 3; i++) {
    if (Array.isArray(v) && v.length === 1) v = v[0];
    else if (v && typeof v === 'object' && !Array.isArray(v) && '0' in v && Object.keys(v).length === 1) v = (v as Record<string, unknown>)['0'];
    else break;
  }
  if (typeof v === 'string') return v;
  if (v && typeof v === 'object' && !Array.isArray(v)) {
    const keys = Object.keys(v);
    if (keys.length === 1) return keys[0];
  }
  return null;
}

/** The value to give `NextState<T>` to ask for `variantSample` (a unit variant is its name as a string). */
export function pendingValue(variantSample: unknown): unknown {
  return { Pending: variantSample };
}

const DESTRUCTIVE = /quit|exit|close|shutdown|game.?over|main.?menu|title|reset|restart|unload/i;

/** Moving to a state with a name like this usually throws the session away: worth a confirmation. */
export function isDestructiveTransition(variantName: string): boolean {
  return DESTRUCTIVE.test(variantName);
}

/** A `State<T>` / `NextState<T>` resource: listed under States, not again among the resources. */
export function isStateResource(typePath: string): boolean {
  return STATE.test(typePath) || NEXT.test(typePath);
}
