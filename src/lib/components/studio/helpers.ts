/**
 * Shared helpers for the per-format StudioModal wrappers.
 *
 * Each wrapper used to keep its own copy of these (indentLabel, basename,
 * fmtBytes). They are trivial but appeared verbatim in five files — moved
 * here so the wrappers can shrink without losing the behaviour.
 */

/** Tab → "Tab", N-space string → "N sp". Used by the indent dropdown. */
export function indentLabel(unit: string): string {
  if (unit === '\t') return 'Tab';
  return `${unit.length} sp`;
}

/** Filesystem basename that works for both POSIX and Windows separators. */
export function basename(p: string | null | undefined): string {
  if (!p) return '';
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

/** Human-readable byte size — used by the modal header `size` chip. */
export function fmtBytes(n: number): string {
  if (n < 1024)              return `${n} B`;
  if (n < 1024 * 1024)       return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * Derive a TypePill `kind` bucket from a Studio ResolvedType. The pill
 * widget accepts free-form strings and runs them through its own
 * resolver, but we map containers (option/vec/map/tuple/external) to
 * the names that resolver actually recognises — for primitives, the
 * raw Rust type name is forwarded as-is so `i32` → int, `f64` → float,
 * `String` → string, etc. `named` types defer to the schema (struct vs
 * enum); `external`/`unknown` fall through to the muted bucket.
 */
export function typePillKind(
  ty:     { kind: string; name?: string; path?: string } | null | undefined,
  schema: { types?: Record<string, { kind: 'struct' | 'enum' | 'alias' }> } | null | undefined,
): string {
  if (!ty) return 'unknown';
  switch (ty.kind) {
    case 'primitive': return ty.name ?? 'unknown';
    case 'option':    return 'option';
    case 'vec':       return 'array';
    case 'map':       return 'map';
    case 'tuple':     return 'tuple';
    case 'external':  return 'unknown';
    case 'unknown':   return 'unknown';
    case 'named': {
      if (!schema?.types || !ty.path) return 'struct';
      const def = schema.types[ty.path];
      if (!def) return 'struct';
      if (def.kind === 'enum')   return 'enum';
      if (def.kind === 'struct') return 'struct';
      return 'unknown';
    }
    default: return 'unknown';
  }
}
