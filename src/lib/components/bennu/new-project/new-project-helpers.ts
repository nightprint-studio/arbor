/**
 * Pure helpers of the New Project wizard — the logic that decides what the form shows and sends,
 * kept out of the components so it can be tested without a DOM.
 */

import type {
  FieldValue, GeneratorField, NameRule, NewProjectGenerator, Problem, ProjectRequest,
} from '$lib/ipc/bennu/new-project';

export type FormValues = Record<string, FieldValue>;

/** Every field's default, as a values object. */
export function defaultsOf(fields: readonly GeneratorField[]): FormValues {
  const out: FormValues = {};
  for (const f of fields) out[f.key] = Array.isArray(f.default) ? [...f.default] : f.default;
  return out;
}

function sameKind(f: GeneratorField, v: FieldValue): boolean {
  switch (f.kind) {
    case 'toggle': return typeof v === 'boolean';
    case 'multi': return Array.isArray(v);
    default: return typeof v === 'string';
  }
}

/**
 * Values for a new set of fields, keeping what the user already chose where it still makes sense.
 *
 * Needed because the form is rebuilt twice: when the remote options arrive (a skeleton form was
 * on screen a moment before) and when the user flips between generators that share keys
 * (`group`, `version`, `java`). A choice survives if the field exists, has the same kind, and —
 * for a select — is still one of its options; a multi-select keeps only the ids that still exist.
 */
export function mergeValues(fields: readonly GeneratorField[], previous: FormValues): FormValues {
  const out = defaultsOf(fields);
  for (const f of fields) {
    const had = previous[f.key];
    if (had === undefined || !sameKind(f, had)) continue;
    if (f.kind === 'select' && f.options.length > 0 && !f.options.some((o) => o.value === had)) continue;
    if (f.kind === 'multi') {
      const known = new Set(f.options.map((o) => o.value));
      out[f.key] = (had as string[]).filter((id) => known.size === 0 || known.has(id));
      continue;
    }
    out[f.key] = had;
  }
  return out;
}

/** Whether a field is shown under the current values. */
export function isVisible(field: GeneratorField, values: FormValues): boolean {
  const w = field.show_when;
  if (!w) return true;
  return String(values[w.key]) === w.value;
}

/** The fields to draw, split into the always-visible set and the "More options" fold. */
export function splitFields(
  fields: readonly GeneratorField[],
  values: FormValues,
): { basic: GeneratorField[]; advanced: GeneratorField[] } {
  const shown = fields.filter((f) => isVisible(f, values));
  return { basic: shown.filter((f) => !f.advanced), advanced: shown.filter((f) => f.advanced) };
}

/** The message for `field`, from a preview's problems. */
export function problemFor(problems: readonly Problem[], field: string): string | null {
  return problems.find((p) => p.field === field)?.message ?? null;
}

/** Problems about fields the form doesn't draw (e.g. a hidden advanced option) still need a home. */
export function strayProblems(problems: readonly Problem[], drawn: ReadonlySet<string>): Problem[] {
  return problems.filter((p) => !drawn.has(p.field));
}

// ── generators ───────────────────────────────────────────────────────────────────────────

/** The generators of one category, in catalog order. */
export function generatorsIn(generators: readonly NewProjectGenerator[], category: string): NewProjectGenerator[] {
  return generators.filter((g) => g.category === category);
}

/** The generator to select when the wizard opens: the remembered one if it still exists. */
export function initialGenerator(
  generators: readonly NewProjectGenerator[],
  remembered: string,
): NewProjectGenerator | null {
  const usable = generators.filter((g) => g.available);
  return usable.find((g) => g.id === remembered) ?? usable[0] ?? generators[0] ?? null;
}

/** The generator `step` rows away from `currentId` inside the full ordered list, skipping nothing. */
export function neighbour(generators: readonly NewProjectGenerator[], currentId: string, step: 1 | -1): NewProjectGenerator | null {
  if (!generators.length) return null;
  const i = generators.findIndex((g) => g.id === currentId);
  const next = i < 0 ? 0 : (i + step + generators.length) % generators.length;
  return generators[next];
}

/** What the Name field shows as a placeholder, per identifier rule. */
export function namePlaceholder(rule: NameRule): string {
  switch (rule) {
    case 'crate': return 'my-crate';
    case 'npm': return 'my-app';
    default: return 'demo';
  }
}

/** The first name from `base`, `base-2`, `base-3`… that `taken` doesn't claim. */
export function freeName(base: string, taken: (name: string) => boolean): string {
  if (!taken(base)) return base;
  for (let n = 2; n < 1000; n++) if (!taken(`${base}-${n}`)) return `${base}-${n}`;
  return base;
}

/** The request the BE gets: trimmed text, and only options the current form actually has. */
export function buildRequest(
  generator: string,
  name: string,
  location: string,
  git: boolean,
  fields: readonly GeneratorField[],
  values: FormValues,
): ProjectRequest {
  const options: FormValues = {};
  for (const f of fields) {
    const v = values[f.key];
    if (v !== undefined) options[f.key] = typeof v === 'string' ? v.trim() : v;
  }
  return { generator, name: name.trim(), location: location.trim().replace(/[\\/]+$/, ''), git, options };
}

/** `location/name` with forward slashes — only for display before the BE has answered. */
export function joinForDisplay(location: string, name: string): string {
  const loc = location.trim().replace(/\\/g, '/').replace(/\/+$/, '');
  const n = name.trim();
  return loc && n ? `${loc}/${n}` : '';
}

/** A run id: unique per run, only characters the BE accepts. */
export function newRunId(now: number = Date.now(), rand: number = Math.random()): string {
  return `np-${now.toString(36)}-${Math.floor(rand * 36 ** 4).toString(36)}`;
}
