/**
 * What the inspector's value editor needs to decide, kept out of the components so it can be tested.
 *
 * The editor is **value-driven**: BRP serialises through each type's own `Serialize`, so the shape
 * from the registry can disagree with the wire (`Vec3` is a struct of x/y/z but travels as
 * `[x, y, z]`). The value picks the control; the shape only refines it (integer vs. float, which
 * enum variants exist) and says how a child is *addressed* in a reflect path.
 */

import type { BrpField, BrpShape, BrpVariant } from '$lib/ipc/bennu/brp';

export type JsonKind = 'null' | 'bool' | 'number' | 'string' | 'array' | 'object';

export function jsonKind(v: unknown): JsonKind {
  if (v === null || v === undefined) return 'null';
  if (Array.isArray(v)) return 'array';
  switch (typeof v) {
    case 'boolean': return 'bool';
    case 'number': return 'number';
    case 'string': return 'string';
    default: return 'object';
  }
}

/** `my_game::combat::Health` → `Health`; `State<my_game::Phase>` → `State<Phase>`. Mirrors the
 *  backend's `short_type_name`, which cannot be asked for here without a round-trip per row. */
export function shortTypeName(path: string): string {
  const tail = (s: string) => s.slice(s.lastIndexOf('::') + 2);
  const lt = path.indexOf('<');
  if (lt >= 0 && path.endsWith('>')) return `${tail(path.slice(0, lt))}<${tail(path.slice(lt + 1, -1))}>`;
  return tail(path);
}

/** The shape of a struct field (or of a struct-variant field), if the shape knows it. */
export function fieldShape(shape: BrpShape | undefined, name: string): BrpShape | undefined {
  return shape?.kind === 'struct' ? shape.fields.find((f) => f.name === name)?.shape : undefined;
}

/** The variant an enum value currently is: a bare string is a unit variant, `{ Name: … }` the rest. */
export function currentVariant(value: unknown): string | null {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && !Array.isArray(value)) return Object.keys(value)[0] ?? null;
  return null;
}

export function variantOf(shape: BrpShape | undefined, name: string | null): BrpVariant | undefined {
  return shape?.kind === 'enum' ? shape.variants.find((v) => v.name === name) : undefined;
}

/** The reflect path of an object member: `.key`. */
export function memberPath(parent: string, key: string): string {
  return `${parent}.${key}`;
}

/**
 * The reflect path of array element `index`.
 *
 * Which spelling depends on what the array *is*: a `Vec3` travels as `[x, y, z]` but is reflected as
 * a struct, so element 1 is `.y`; a tuple struct is `.1`; a `Vec`/array is `[1]`. With no shape the
 * list form is the best guess — and if it is wrong the game says so in words.
 */
export function elementPath(parent: string, shape: BrpShape | undefined, index: number): string {
  if (shape?.kind === 'struct') {
    const f: BrpField | undefined = shape.fields[index];
    if (f) return `${parent}.${f.name}`;
  }
  if (shape?.kind === 'tuple') return `${parent}.${index}`;
  return `${parent}[${index}]`;
}

/** The shape of array element `index`, matching {@link elementPath}'s reading of the shape. */
export function elementShape(shape: BrpShape | undefined, index: number): BrpShape | undefined {
  if (shape?.kind === 'struct') return shape.fields[index]?.shape;
  if (shape?.kind === 'tuple') return shape.items[index];
  if (shape?.kind === 'list') return shape.item;
  return undefined;
}

/** The label to draw beside element `index`: the field name for a struct-shaped array, else the index. */
export function elementLabel(shape: BrpShape | undefined, index: number): string {
  return shape?.kind === 'struct' ? (shape.fields[index]?.name ?? String(index)) : String(index);
}

/** A short array of plain numbers reads best on one line (`x 0  y 1.5  z 0`). */
export function isInlineNumbers(v: unknown): v is number[] {
  return Array.isArray(v) && v.length > 0 && v.length <= 4 && v.every((x) => typeof x === 'number');
}

/**
 * Where an enum's payload lives, as the reflect path and shape the editor recurses with.
 *
 * `{ Moving: 3 }` is a newtype (`.0`), `{ Moving: [1, 2] }` a tuple (`.0`, `.1`), and a struct
 * variant `{ Aiming: { angle: 1 } }` addresses its fields by name — so the payload's own path is
 * the enum's path and the member is appended by the usual array/object rules. This returns the
 * variant's payload *shape* in the form those rules expect.
 */
export function payloadShape(variant: BrpVariant | undefined): BrpShape | undefined {
  if (!variant) return undefined;
  if (variant.kind === 'struct') return { kind: 'struct', type_path: variant.name, fields: variant.fields };
  if (variant.kind === 'tuple') {
    return variant.items.length === 1
      ? variant.items[0]
      : { kind: 'tuple', type_path: variant.name, items: variant.items };
  }
  return undefined;
}

/** The path of an enum's newtype payload (`{ Variant: v }` → `<path>.0`). */
export function newtypePath(parent: string): string {
  return `${parent}.0`;
}

/**
 * An entity id the way Bevy prints it: `index` v `generation` (`3v1`). The wire carries the two packed
 * into one integer — index in the low 32 bits — and the packed form means nothing to a reader.
 * Exact for the ids a real session produces; a generation past 2^21 would exceed what a JS number can
 * hold, and such an id is shown by its raw digits instead of a wrong tag.
 */
export function entityTag(id: number): string {
  if (!Number.isSafeInteger(id)) return String(id);
  const index = id % 4294967296;
  const generation = Math.floor(id / 4294967296);
  return `${index}v${generation}`;
}
