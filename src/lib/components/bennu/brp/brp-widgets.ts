/**
 * Which widget draws a value. The editor is value-driven (a JSON kind picks a control), and this adds
 * the few *types* the inspector draws specially — a `Transform`, a colour, a vector — plus the hints a
 * game's `#[derive(RemoteInspect)]` sent (a slider's range, "this is a colour", "this is an angle").
 */

export interface InspectHint {
  /** The value is a colour (a hex string, `[r, g, b(, a)]` or a `{red, green, blue}` struct). */
  color?: boolean;
  range?: [number, number];
  step?: number;
  /** Seconds since the Unix epoch, drawn as a date. */
  date?: boolean;
  /** Radians, drawn and edited in degrees. */
  angle?: boolean;
  readonly?: boolean;
  label?: string;
  unit?: string;
  /** A sentence about the field (or, under the empty field path, about the type): `#[inspect(description)]` or the `///` docs. */
  description?: string;
}

/** `type path → { field path → hint }`, as `arbor.inspect_hints` answers. Field paths are dotted: `stats.hp`, `0`. */
export type HintMap = Record<string, Record<string, InspectHint>>;

export type WidgetKind =
  | 'transform' | 'color' | 'quat' | 'vec' | 'duration' | 'handle' | 'date' | 'angle' | 'slider' | 'generic';

const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isNums = (v: unknown, n?: number): v is number[] => Array.isArray(v) && v.every(isNum) && (n === undefined || v.length === n);
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const tail = (path: string | undefined) => (path ?? '').split('<')[0].split('::').pop() ?? '';
/** `Vec2`, `Vec3A`, `IVec3`, `DVec4`, … — the glam vectors a value of 2–4 numbers can be. */
const isVecName = (name: string) => /^[DIUB]?Vec[234]A?$/.test(name);
const hasFraction = (v: number[]) => v.some((n) => !Number.isInteger(n));

/** A struct holding red, green, blue (and usually alpha): `Srgba`, `LinearRgba`, `Hsla`-less colours. */
export function isColorStruct(v: unknown): v is { red: number; green: number; blue: number; alpha?: number } {
  return isObject(v) && isNum(v.red) && isNum(v.green) && isNum(v.blue);
}

export function classify(typePath: string | undefined, value: unknown, hint?: InspectHint): WidgetKind {
  const name = tail(typePath);
  if (isObject(value) && isNums(value.translation, 3) && isNums(value.rotation, 4) && isNums(value.scale, 3)) return 'transform';
  if (isColorStruct(value) || (hint?.color && (isNums(value, 3) || isNums(value, 4)))) return 'color';
  if (isObject(value) && isNum(value.secs) && isNum(value.nanos) && Object.keys(value).length === 2) return 'duration';
  if (isObject(value) && Object.keys(value).length === 1 && ('Strong' in value || 'Weak' in value)) return 'handle';
  if (isNums(value, 4) && name === 'Quat') return 'quat';
  if (hint?.date && isNum(value)) return 'date';
  if (hint?.angle && isNum(value)) return 'angle';
  if (hint?.range && isNum(value)) return 'slider';
  // A short list of numbers is a vector when its type says so; with no type to go by, only when it has a
  // fractional part — `[111, 112]` is far more likely a list of entities than a position.
  if ((isNums(value, 2) || isNums(value, 3) || isNums(value, 4)) && (isVecName(name) || (name === '' && hasFraction(value)))) return 'vec';
  return 'generic';
}

/** `.translation.x` / `.items[2]` → `translation.x` / `items.2`, the key hints are stored under. */
export function hintKey(reflectPath: string): string {
  return reflectPath.replace(/\[(\d+)\]/g, '.$1').replace(/^\./, '');
}

export function hintFor(hints: HintMap | undefined, typePath: string | undefined, reflectPath: string): InspectHint | undefined {
  return typePath ? hints?.[typePath]?.[hintKey(reflectPath)] : undefined;
}

const AXES = ['x', 'y', 'z', 'w'] as const;
export const axisName = (i: number): string => AXES[i] ?? String(i);

/** Unix seconds ⇄ the `YYYY-MM-DD` a date input edits (UTC). */
export function secondsToDate(s: number): string {
  return new Date(s * 1000).toISOString().slice(0, 10);
}

export function dateToSeconds(date: string): number | null {
  const t = Date.parse(`${date}T00:00:00Z`);
  return Number.isNaN(t) ? null : t / 1000;
}

/** `{secs, nanos}` ⇄ fractional seconds. */
export const durationToSeconds = (d: { secs: number; nanos: number }): number => d.secs + d.nanos / 1e9;
export function secondsToDuration(s: number): { secs: number; nanos: number } {
  const secs = Math.max(0, Math.floor(s));
  return { secs, nanos: Math.round((Math.max(0, s) - secs) * 1e9) };
}
