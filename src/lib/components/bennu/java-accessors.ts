/**
 * java-accessors — pure, deterministic detection of which class fields already
 * have a getter, setter and/or with-method in a Java source. Used to surface
 * G / S / W markers in the Generate modal field rows and the Structure panel so
 * the user sees at a glance what's already there before generating duplicates.
 *
 * Deliberately regex/scan-based (like `java-outline.ts`) — no tree-sitter here,
 * the shared editor owns the real parser and re-parsing would duplicate work. A
 * cheap method-signature scan is plenty for "does an accessor for this field
 * exist". Kept a small pure helper (no Svelte, no IPC, no DOM) so it's
 * unit-testable and reusable from both consumers.
 *
 * Matching rules (per field `name`, both camelCase and record/fluent styles):
 *   • getter  — `getName()` / `isName()` (boolean) / `name()` (fluent/record).
 *   • setter  — `setName(T)` / `name(T)` (fluent, one arg).
 *   • wither  — `withName(...)`.
 *
 * SEAM — the reliable source of "existing accessors" is the backend symbol model
 * (the same `bennu_symbols` seam `java-outline.ts` calls out). Today we scan the
 * raw source; when the BE lands, feed method signatures from it into
 * `detectAccessors` (or a variant that takes `JavaSymbol[]`) — the returned
 * `AccessorMap` shape stays the same, so both consumers are agnostic.
 */

/** Which accessors already exist for one field. */
export interface AccessorFlags {
  getter: boolean;
  setter: boolean;
  wither: boolean;
}

/** field name → existing-accessor flags. */
export type AccessorMap = Record<string, AccessorFlags>;

const EMPTY: AccessorFlags = { getter: false, setter: false, wither: false };

/** Uppercase the first character (`name` → `Name`) for `getName`/`setName`/`withName`. */
function upperFirst(s: string): string {
  return s.length ? s[0].toUpperCase() + s.slice(1) : s;
}

/** A single-line method signature: modifiers, then the (bounded) return-type prefix,
 *  the name, and the raw param list. Anchored per line with `[^(){};=]*?` — kept off
 *  `;`/`=`/`{`/`}` so it can't wander past the signature, and applied line-by-line
 *  against short lines only (below) so it can never span a big comment/body: the old
 *  GLOBAL `[^;={}]*?` scan over the whole source was the Generate-modal freeze on a
 *  large legacy `.java`. */
const METHOD_SIG_RE =
  /^\s*(?:(?:public|private|protected|static|final|synchronized|native|default|abstract)\s+)+[^(){};=]*?\b([A-Za-z_]\w*)\s*\(([^)]*)\)/;

/** Declaration lines are short; skip long/minified lines (perf + a hard backtracking
 *  backstop). A method signature never legitimately runs this long on one line. */
const MAX_SIG_LINE = 400;

/**
 * Scan `source` for the set of method names declared in it (best-effort). We only
 * need the method *name* and whether it takes ≥1 argument, so a light per-line
 * signature regex is enough. Returns two sets: no-arg method names and one-or-more-arg
 * method names (a method can appear in both if overloaded).
 */
function scanMethods(source: string): { noArg: Set<string>; withArg: Set<string> } {
  const noArg = new Set<string>();
  const withArg = new Set<string>();
  for (const raw of source.split(/\r?\n/)) {
    if (raw.length > MAX_SIG_LINE) continue;
    const m = METHOD_SIG_RE.exec(raw);
    if (!m) continue;
    const name = m[1];
    const hasArg = m[2].trim().length > 0;
    if (hasArg) withArg.add(name);
    else noArg.add(name);
  }
  return { noArg, withArg };
}

/**
 * Detect, for each field in `fieldNames`, which accessors already exist in
 * `source`. Pure — same input always yields the same map. Unknown fields map to
 * all-false. `fieldTypes` (optional) lets a boolean field also match an `isX()`
 * getter; without it we accept `isX()` for any field (harmless over-match, keeps
 * the helper type-free).
 */
export function detectAccessors(source: string, fieldNames: string[]): AccessorMap {
  const { noArg, withArg } = scanMethods(source);
  const out: AccessorMap = {};
  for (const name of fieldNames) {
    const Cap = upperFirst(name);
    const getter =
      noArg.has('get' + Cap) || // getName()
      noArg.has('is' + Cap) ||  // isName()  (boolean bean)
      noArg.has(name);          // name()    (record / fluent)
    const setter =
      withArg.has('set' + Cap) || // setName(T)
      withArg.has(name);          // name(T)   (fluent)
    const wither = withArg.has('with' + Cap); // withName(T)
    out[name] = { getter, setter, wither };
  }
  return out;
}

/** Convenience accessor with an all-false fallback for unknown fields. */
export function flagsFor(map: AccessorMap, name: string): AccessorFlags {
  return map[name] ?? EMPTY;
}
