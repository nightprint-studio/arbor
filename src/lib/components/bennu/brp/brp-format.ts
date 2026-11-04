/**
 * Copying a value out of the inspector: as the JSON the game sent, or laid out as RON.
 *
 * The RON is the *shape of the JSON* in RON syntax — structs as `(field: value)`, lists as `[a, b]`,
 * strings quoted — not a type-aware serialisation: the game's wire form already flattened what its
 * `Serialize` impls chose to (`Vec3` is `[x, y, z]`, a unit enum a bare string), and without the type
 * there is nothing to rebuild a `Vec3(…)` from. Good for reading and for pasting into a scene file
 * that names the same fields; not a guarantee that the game will parse it back.
 */

const LINE_WIDTH = 72;
const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;

export function toJson(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? 'null';
}

/** `value` as RON text, wrapped at about {@link LINE_WIDTH} columns, two-space indent. */
export function toRon(value: unknown): string {
  return ron(value, 0);
}

function ron(value: unknown, depth: number): string {
  if (value === null || value === undefined) return 'None';
  if (typeof value === 'boolean') return String(value);
  if (typeof value === 'number') return Number.isFinite(value) ? String(value) : 'None';
  if (typeof value === 'string') return JSON.stringify(value);
  if (Array.isArray(value)) return group('[', ']', value.map((v) => ron(v, depth + 1)), depth);
  if (typeof value === 'object') {
    const parts = Object.entries(value as Record<string, unknown>).map(
      ([k, v]) => `${IDENT.test(k) ? k : JSON.stringify(k)}: ${ron(v, depth + 1)}`,
    );
    return group('(', ')', parts, depth);
  }
  return 'None';
}

/** `open a, b close` on one line when it fits and has no line break inside, else one item per line. */
function group(open: string, close: string, items: string[], depth: number): string {
  if (items.length === 0) return open + close;
  const flat = `${open}${items.join(', ')}${close}`;
  if (!flat.includes('\n') && flat.length + depth * 2 <= LINE_WIDTH) return flat;
  const pad = '  '.repeat(depth + 1);
  return `${open}\n${items.map((i) => `${pad}${i},`).join('\n')}\n${'  '.repeat(depth)}${close}`;
}

/** A whole entity as copied: `type path → value`, ordered by type path so two copies diff cleanly. */
export function entityDocument(components: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(components).sort(([a], [b]) => a.localeCompare(b)));
}
