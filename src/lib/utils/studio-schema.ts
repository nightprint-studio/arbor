/**
 * Shared schema-aware walker used by every studio modal that decorates
 * its tree from a Rust-struct or JSON-Schema sidecar.
 *
 * Centralises the field-name resolution rules so future serde-attr work
 * lands in one place instead of 5 near-identical copies:
 *
 *   1. **Direct match** — `seg === f.name` or `(f.aliases ?? []).includes(seg)`.
 *      Covers `#[serde(rename = "...")]` + `#[serde(alias = "...")]` + the
 *      Rust source ident (added to aliases by the host when distinct
 *      from the serialised name).
 *
 *   2. **Flatten fallback** — when no field matches, walk every field
 *      marked `#[serde(flatten)]` and try to resolve `seg` inside its
 *      type:
 *        - `Map<K, V>` / `BTreeMap` / `HashMap` → `V` for any unmatched
 *          key (the catch-all-keys pattern Spring Boot uses).
 *        - `Option<T>` → unwrap and retry.
 *        - `Named { path }` resolving to a struct → recursively look up
 *          `seg` in that struct's fields (its own aliases + nested
 *          flatten included).
 *
 * The walker is purely declarative — no mutation, no IO — so it's safe
 * to call from a `$derived` block.
 */

import type {
  Schema, TypeDef, ResolvedType, FieldDef,
} from '$lib/ipc/studio-format';

export function typeAtPath(schema: Schema | null, path: string[]): ResolvedType | null {
  if (!schema) return null;
  let cur: ResolvedType = { kind: 'named', path: schema.root_type };
  for (const seg of path) {
    cur = stepTypeBySegment(schema, cur, seg);
    if (cur.kind === 'unknown' || cur.kind === 'external') return cur;
  }
  return cur;
}

export function stepTypeBySegment(
  schema: Schema | null,
  ty:     ResolvedType,
  seg:    string,
): ResolvedType {
  if (!schema) return { kind: 'unknown', hint: 'no schema' };
  switch (ty.kind) {
    case 'option': return stepTypeBySegment(schema, ty.inner, seg);
    case 'vec':    return ty.inner;
    case 'map':    return ty.value;
    case 'tuple': {
      const idx = parseInt(seg, 10);
      if (!Number.isFinite(idx) || idx < 0 || idx >= ty.items.length) {
        return { kind: 'unknown', hint: `tuple index ${seg} out of range` };
      }
      return ty.items[idx];
    }
    case 'named': {
      const def: TypeDef | undefined = schema.types[ty.path];
      if (!def) return { kind: 'unknown', hint: `unresolved ${ty.path}` };
      if (def.kind === 'alias')  return stepTypeBySegment(schema, def.target, seg);
      if (def.kind === 'struct') return stepStruct(schema, def, seg);
      return { kind: 'unknown', hint: `cannot step into enum ${def.name}` };
    }
    default: return { kind: 'unknown', hint: `cannot step into ${ty.kind}` };
  }
}

function fieldMatches(f: FieldDef, seg: string): boolean {
  if (f.name === seg) return true;
  const aliases = f.aliases ?? [];
  return aliases.includes(seg);
}

function stepStruct(
  schema: Schema,
  def:    Extract<TypeDef, { kind: 'struct' }>,
  seg:    string,
): ResolvedType {
  // 1. Direct field match (name + aliases).
  const direct = def.fields.find(f => fieldMatches(f, seg));
  if (direct) return direct.ty;

  // 2. Flatten fallback — try every `#[serde(flatten)]` field.
  //    Order matches source order; first hit wins. Cycles are bounded
  //    by `seen` to keep pathological cyclic schemas from looping.
  const seen = new Set<string>();
  for (const ff of def.fields) {
    if (!ff.flatten) continue;
    const hit = stepIntoFlatten(schema, ff.ty, seg, seen);
    if (hit) return hit;
  }

  return { kind: 'unknown', hint: `unknown field "${seg}" on ${def.name}` };
}

/**
 * Resolve `seg` inside a flatten-field's type. Returns the matched
 * type, or `null` when nothing resolves cleanly (so the caller can
 * keep iterating siblings).
 */
function stepIntoFlatten(
  schema: Schema,
  ty:     ResolvedType,
  seg:    string,
  seen:   Set<string>,
): ResolvedType | null {
  switch (ty.kind) {
    case 'option': return stepIntoFlatten(schema, ty.inner, seg, seen);
    case 'map':    return ty.value;  // catch-all-keys — any seg resolves to V
    case 'named': {
      if (seen.has(ty.path)) return null;
      seen.add(ty.path);
      const def: TypeDef | undefined = schema.types[ty.path];
      if (!def) return null;
      if (def.kind === 'alias') return stepIntoFlatten(schema, def.target, seg, seen);
      if (def.kind !== 'struct') return null;
      // Direct match inside the flattened struct.
      const direct = def.fields.find(f => fieldMatches(f, seg));
      if (direct) return direct.ty;
      // Recurse into its own flatten fields.
      for (const ff of def.fields) {
        if (!ff.flatten) continue;
        const hit = stepIntoFlatten(schema, ff.ty, seg, seen);
        if (hit) return hit;
      }
      return null;
    }
    // Vec / Tuple / Primitive can't carry arbitrary key-named children
    // in a flatten — only Map and nested structs make sense semantically.
    default: return null;
  }
}

/** Convenience for the inspector "missing required fields" list —
 *  walks a struct's fields including flattened sub-structs. Returns a
 *  flat list de-duped by serialised name. */
export function flattenedStructFields(
  schema: Schema,
  def:    Extract<TypeDef, { kind: 'struct' }>,
): FieldDef[] {
  const out: FieldDef[] = [];
  const seenNames = new Set<string>();
  const seenTypes = new Set<string>();
  const pushUnique = (f: FieldDef) => {
    if (seenNames.has(f.name)) return;
    seenNames.add(f.name);
    out.push(f);
  };
  const walk = (struct: Extract<TypeDef, { kind: 'struct' }>) => {
    for (const f of struct.fields) {
      if (!f.flatten) { pushUnique(f); continue; }
      // Step into flatten — but only when it's a struct (Map flatten
      // expands to arbitrary keys we can't enumerate).
      let inner = f.ty;
      if (inner.kind === 'option') inner = inner.inner;
      if (inner.kind !== 'named') continue;
      if (seenTypes.has(inner.path)) continue;
      seenTypes.add(inner.path);
      const sub: TypeDef | undefined = schema.types[inner.path];
      if (!sub) continue;
      if (sub.kind === 'alias') continue;  // alias chain through flatten is unusual
      if (sub.kind !== 'struct') continue;
      walk(sub);
    }
  };
  walk(def);
  return out;
}
