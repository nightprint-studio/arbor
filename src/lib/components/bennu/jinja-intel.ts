/**
 * What can be written inside a Jinja template's tags — the model completion (`jinja-completion.ts`) and hover
 * (`jinja-hover.ts`) both read, so the two cannot disagree about a template.
 *
 * In every template: the statement names after `{%`, the filters after `|`, the tests after `is`, and
 * the names the template binds itself with `{% set %}` and `{% for %}`. In a code template — one kept
 * under `bennu/templates/<kind>/` — also the data its kind is rendered with, followed through the
 * template's own loops, so
 * inside `{% for case in field.cases %}` typing `case.` offers a case's fields. That data comes from a
 * JSON Schema the backend generates from the structs a template is rendered with, so completion and
 * rendering cannot disagree about what a template can read.
 */

import { templateSchema, type TemplateKindId, type TemplateSchema } from '$lib/ipc/bennu/templates';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { templateKindOfPath } from './templates/template-paths';

export interface JsonSchema {
  type?: string | string[];
  description?: string;
  properties?: Record<string, JsonSchema>;
  items?: JsonSchema;
  anyOf?: JsonSchema[];
  oneOf?: JsonSchema[];
  allOf?: JsonSchema[];
  const?: unknown;
  enum?: unknown[];
  /** A method of the object rather than a field: `style.local(type)`. */
  'x-method'?: boolean;
}

/** Jinja's own filters, as Bennu's template engine has them. */
export const BUILTIN_FILTERS: [string, string][] = [
  ['abs', 'The absolute value of a number'],
  ['attr', 'attr(name) — an attribute looked up by name'],
  ['batch', 'batch(n) — the items in groups of n'],
  ['bool', 'The value as true or false'],
  ['capitalize', 'The first letter upper case, the rest lower case'],
  ['count', 'How many items — the same as length'],
  ['default', 'default(value) — value, when this one is undefined'],
  ['d', 'Short for default'],
  ['dictsort', "A map's items, sorted by key"],
  ['escape', 'Escaped for HTML'],
  ['e', 'Short for escape'],
  ['first', 'The first item'],
  ['float', 'The value as a floating-point number'],
  ['indent', 'indent(width) — every line after the first, indented'],
  ['int', 'The value as an integer'],
  ['items', "A map's (key, value) pairs"],
  ['join', 'join(separator) — the items as one string'],
  ['last', 'The last item'],
  ['length', 'How many items, or characters'],
  ['lines', 'The text as a list of lines'],
  ['list', 'The value as a list'],
  ['lower', 'Lower case'],
  ['map', 'map("filter") or map(attribute="name") — every item transformed'],
  ['max', 'The largest item'],
  ['min', 'The smallest item'],
  ['reject', 'reject("test") — the items that fail a test'],
  ['rejectattr', 'rejectattr("name", "test") — the items whose attribute fails a test'],
  ['replace', 'replace(old, new) — every occurrence replaced'],
  ['reverse', 'In reverse order'],
  ['round', 'round(precision) — a number, rounded'],
  ['safe', 'Marked as needing no escaping'],
  ['select', 'select("test") — the items that pass a test'],
  ['selectattr', 'selectattr("name", "test") — the items whose attribute passes a test'],
  ['slice', 'slice(n) — the items in n columns'],
  ['sort', 'Sorted; sort(attribute="name") by an attribute'],
  ['split', 'split(separator) — the text as a list'],
  ['string', 'The value as text'],
  ['sum', 'The sum of the items'],
  ['title', 'Every word capitalised'],
  ['trim', 'Whitespace removed from both ends'],
  ['unique', 'Every item once'],
  ['upper', 'Upper case'],
];

export const TESTS: [string, string][] = [
  ['defined', 'The value exists'],
  ['undefined', 'The value does not exist'],
  ['none', 'The value is none'],
  ['true', 'The value is true'],
  ['false', 'The value is false'],
  ['boolean', 'A boolean'],
  ['number', 'A number'],
  ['integer', 'An integer'],
  ['float', 'A floating-point number'],
  ['string', 'Text'],
  ['sequence', 'A list, or anything indexable'],
  ['mapping', 'A map'],
  ['iterable', 'Something a for loop can go through'],
  ['startingwith', 'startingwith("prefix") — text starting with prefix'],
  ['endingwith', 'endingwith("suffix") — text ending with suffix'],
  ['lower', 'Text in lower case'],
  ['upper', 'Text in upper case'],
  ['even', 'An even number'],
  ['odd', 'An odd number'],
  ['divisibleby', 'divisibleby(n) — a multiple of n'],
  ['eq', 'eq(value) — equal to value'],
  ['ne', 'ne(value) — not equal to value'],
  ['lt', 'lt(value) — less than value'],
  ['le', 'le(value) — at most value'],
  ['gt', 'gt(value) — greater than value'],
  ['ge', 'ge(value) — at least value'],
  ['in', 'in(sequence) — one of the items of sequence'],
  ['safe', 'Marked as needing no escaping'],
  ['escaped', 'Already escaped'],
  ['filter', 'The name of a filter that exists'],
  ['test', 'The name of a test that exists'],
];

export const FUNCTIONS: [string, string][] = [
  ['range', 'range(stop) / range(start, stop, step) — a sequence of integers'],
  ['dict', 'dict(a=1, b=2) — a map, from keyword arguments'],
  ['namespace', 'namespace() — an object a loop can assign attributes on'],
  ['debug', 'debug() — every variable in scope, to find out what a template sees'],
];

export const EXPRESSION_WORDS = ['and', 'or', 'not', 'in', 'is', 'if', 'else', 'true', 'false', 'none'];

export const LOOP: JsonSchema = {
  type: 'object',
  description: 'The innermost for loop',
  properties: {
    index: { type: 'integer', description: 'The position in the loop, from 1' },
    index0: { type: 'integer', description: 'The position in the loop, from 0' },
    revindex: { type: 'integer', description: 'The position counted from the end, down to 1' },
    revindex0: { type: 'integer', description: 'The position counted from the end, down to 0' },
    first: { type: 'boolean', description: 'The first iteration' },
    last: { type: 'boolean', description: 'The last iteration' },
    length: { type: 'integer', description: 'How many items the loop goes through' },
    depth: { type: 'integer', description: 'How deeply this loop is nested, from 1' },
    depth0: { type: 'integer', description: 'How deeply this loop is nested, from 0' },
    previtem: { description: 'The item before this one' },
    nextitem: { description: 'The item after this one' },
    cycle: { description: 'cycle(a, b, …) — one of its arguments per iteration, in turn' },
    changed: { description: 'changed(value) — whether value differs from the previous iteration' },
  },
};

/** The kind of code template the active file is — read off the folder it is kept in; `null` for a Jinja file kept
 *  anywhere else, which gets the language alone. Asked when completion or hover is, so it follows the tab. */
export function activeTemplateKind(): TemplateKindId | null {
  return templateKindOfPath(projectStore.activeFilePath ?? '');
}

const schemas = new Map<TemplateKindId, Promise<TemplateSchema | null>>();

/** A kind's schema, fetched once per window. A failure is not remembered: the backend may simply not
 *  have been up yet, and the next completion asks again. */
export function loadSchema(kind: TemplateKindId | null): Promise<TemplateSchema | null> {
  if (!kind) return Promise.resolve(null);
  let pending = schemas.get(kind);
  if (!pending) {
    pending = templateSchema(kind).catch(() => {
      schemas.delete(kind);
      return null;
    });
    schemas.set(kind, pending);
  }
  return pending;
}

/** The text of the `{{` or `{%` tag the caret is inside, from its opener — `null` outside one, or in a
 *  comment. */
export function openTag(before: string): string | null {
  const open = Math.max(before.lastIndexOf('{{'), before.lastIndexOf('{%'));
  const close = Math.max(before.lastIndexOf('}}'), before.lastIndexOf('%}'));
  if (open < 0 || close > open) return null;
  const comment = before.lastIndexOf('{#');
  if (comment > open && before.indexOf('#}', comment) < 0) return null;
  return before.slice(open);
}

export interface Binding {
  /** What the name was bound to (`field.cases`). */
  path: string;
  /** A loop variable: one item of `path`, rather than `path` itself. */
  each: boolean;
}

/** The names the template binds to data before the caret, the latest binding of each name winning. */
export function bindingsIn(text: string): Map<string, Binding> {
  const out = new Map<string, Binding>();
  const found: { at: number; name: string; binding: Binding }[] = [];
  for (const m of text.matchAll(/\{%-?\s*for\s+([A-Za-z_]\w*)(?:\s*,\s*([A-Za-z_]\w*))?\s+in\s+([A-Za-z_][\w.]*)/g)) {
    // `for key, value in map` — only the value has a shape worth completing.
    found.push({ at: m.index ?? 0, name: m[2] ?? m[1], binding: { path: m[3], each: true } });
  }
  for (const m of text.matchAll(/\{%-?\s*set\s+([A-Za-z_]\w*)\s*=\s*([A-Za-z_][\w.]*)\s*-?%\}/g)) {
    found.push({ at: m.index ?? 0, name: m[1], binding: { path: m[2], each: false } });
  }
  found.sort((a, b) => a.at - b.at);
  for (const { name, binding } of found) out.set(name, binding);
  return out;
}

/** Every name a `set` or `macro` defines before the caret. */
export function namesDefinedIn(text: string): string[] {
  return [...text.matchAll(/\{%-?\s*(?:set|macro)\s+([A-Za-z_]\w*)/g)].map((m) => m[1]);
}

export function insideLoop(text: string): boolean {
  const opened = text.match(/\{%-?\s*for\b/g)?.length ?? 0;
  const closed = text.match(/\{%-?\s*endfor\b/g)?.length ?? 0;
  return opened > closed;
}

/** The schema of what `path` names — through the template's bindings, `loop`, and the context. */
export function resolve(path: string, context: JsonSchema, bound: Map<string, Binding>, depth = 0): JsonSchema | undefined {
  if (depth > 8) return undefined;
  const [head, ...rest] = path.split('.');
  let current: JsonSchema | undefined;
  const binding = bound.get(head);
  if (binding && binding.path.split('.')[0] !== head) {
    const target = resolve(binding.path, context, bound, depth + 1);
    current = binding.each ? itemsOf(target) : target;
  } else if (head === 'loop') {
    current = LOOP;
  } else {
    current = membersOf(context)[head];
  }
  for (const segment of rest) {
    if (!current) return undefined;
    current = membersOf(current)[segment];
  }
  return current;
}

function variantsOf(schema: JsonSchema): JsonSchema[] {
  return [...(schema.anyOf ?? []), ...(schema.oneOf ?? []), ...(schema.allOf ?? [])];
}

/** The properties a schema describes, merged across `anyOf` / `oneOf` / `allOf` — the shape an
 *  `Option` and a tagged enum arrive in. */
export function membersOf(schema: JsonSchema | undefined): Record<string, JsonSchema> {
  if (!schema) return {};
  const out: Record<string, JsonSchema> = {};
  for (const part of variantsOf(schema)) Object.assign(out, membersOf(part));
  return Object.assign(out, schema.properties ?? {});
}

export function itemsOf(schema: JsonSchema | undefined): JsonSchema | undefined {
  if (!schema) return undefined;
  if (schema.items) return schema.items;
  for (const part of variantsOf(schema)) {
    const found = itemsOf(part);
    if (found) return found;
  }
  return undefined;
}

export function typeLabel(schema: JsonSchema | undefined): string {
  if (!schema) return '';
  const types = new Set<string>();
  const collect = (s: JsonSchema) => {
    if (Array.isArray(s.type)) s.type.forEach((t) => types.add(t));
    else if (s.type) types.add(s.type);
    variantsOf(s).forEach(collect);
  };
  collect(schema);
  types.delete('null');
  if (types.has('array')) {
    const item = typeLabel(itemsOf(schema));
    return item ? `list of ${item}` : 'list';
  }
  if (schema.enum) return schema.enum.map(String).join(' | ');
  return [...types].join(' | ');
}

/** The values a tagged value's `kind` can take. */
export function kindValues(schema: JsonSchema | undefined): string[] {
  const out = new Set<string>();
  const visit = (s: JsonSchema) => {
    const kind = s.properties?.kind;
    if (kind) {
      if (typeof kind.const === 'string') out.add(kind.const);
      for (const value of kind.enum ?? []) if (typeof value === 'string') out.add(value);
    }
    variantsOf(s).forEach(visit);
  };
  if (schema) visit(schema);
  return [...out];
}
