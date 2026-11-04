/**
 * What an endpoint's DTOs look like, as Bennu read them from the code — and what Hirundo does with that:
 * a JSON skeleton for a new request, the `body.…` paths an assertion can name, and the keys a JSON body can
 * have at the caret. All pure, so it is tested without an editor.
 */
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { Schema, SchemaField } from '$lib/types/hirundo';
import { sampleFor } from '../request-model';

/** A sample value for a scalar of this Java type, as JSON. */
function scalarSample(type: string): unknown {
  const t = type.replace(/^java\.(lang|util)\./, '');
  if (/^(Long|long|Integer|int|Short|short|BigInteger|Byte|byte)$/.test(t)) return 1;
  if (/^(Double|double|Float|float|BigDecimal|Number)$/.test(t)) return 1.0;
  if (/^(Boolean|boolean)$/.test(t)) return true;
  const s = sampleFor(t);
  return s || '';
}

/** A JSON value for a schema: objects with all their fields, arrays of one element, an enum's first constant. */
export function sampleOf(schema: Schema | undefined | null, depth = 0): unknown {
  if (!schema || depth > 4) return null;
  switch (schema.kind) {
    case 'object': return Object.fromEntries((schema.fields ?? []).map((f) => [f.name, f.schema ? sampleOf(f.schema, depth + 1) : scalarSample(f.type_text)]));
    case 'array': return schema.item ? [sampleOf(schema.item, depth + 1)] : [];
    case 'map': return {};
    case 'enum': return schema.constants?.[0] ?? '';
    default: return scalarSample(schema.type_text);
  }
}

export function skeletonText(schema: Schema | undefined | null): string {
  return JSON.stringify(sampleOf(schema), null, 2);
}

/** The fields marked required across the whole schema, as `a.b` paths — for the notes of a new request. */
export function requiredPaths(schema: Schema | undefined | null, prefix = '', depth = 0): string[] {
  if (!schema || depth > 4) return [];
  if (schema.kind === 'array') return requiredPaths(schema.item, `${prefix}[]`, depth + 1);
  return (schema.fields ?? []).flatMap((f) => [
    ...(f.required ? [`${prefix ? `${prefix}.` : ''}${f.name}`] : []),
    ...requiredPaths(f.schema, `${prefix ? `${prefix}.` : ''}${f.name}`, depth + 1),
  ]);
}

/** The object schema reached by following `segments` (names; `[*]` / `[n]` step into an array's element). */
export function follow(schema: Schema | undefined | null, segments: string[]): Schema | null {
  let at: Schema | null = schema ?? null;
  for (const seg of segments) {
    if (!at) return null;
    if (/^\[(\*|\d+)\]$/.test(seg)) {
      at = at.kind === 'array' ? (at.item ?? null) : null;
    } else {
      if (at.kind === 'array') at = at.item ?? null;
      const field: SchemaField | undefined = at?.fields?.find((f) => f.name === seg);
      at = field ? (field.schema ?? null) : null;
    }
  }
  if (at?.kind === 'array' && at.item) return at.item;
  return at;
}

/** The fields offered after `body.a.b[*].` — those of the schema that path reaches. */
export function fieldsAfter(schema: Schema | undefined | null, path: string): SchemaField[] {
  const segments = path.split(/(?=\[)|\./).filter(Boolean);
  return follow(schema, segments)?.fields ?? [];
}

/** The key path of the JSON object the caret is in: `{ "a": { "b": [ { | ` → `['a', 'b', '[*]']`. */
export function jsonContext(text: string, caret: number): string[] {
  const stack: string[] = [];
  let key = '';
  let inString = false;
  let current = '';
  let afterColon = false;
  for (let i = 0; i < Math.min(caret, text.length); i++) {
    const c = text[i];
    if (inString) {
      if (c === '\\') { i++; continue; }
      if (c === '"') { inString = false; if (!afterColon) key = current; }
      else current += c;
      continue;
    }
    if (c === '"') { inString = true; current = ''; }
    else if (c === ':') afterColon = true;
    else if (c === '{') { stack.push(afterColon ? key : ''); afterColon = false; key = ''; }
    else if (c === '[') { stack.push(afterColon ? key : ''); stack.push('[*]'); afterColon = false; key = ''; }
    else if (c === '}') { stack.pop(); afterColon = false; }
    else if (c === ']') { stack.pop(); stack.pop(); afterColon = false; }
    else if (c === ',') afterColon = false;
  }
  return stack.filter((s) => s !== '');
}

/** Completion of assertion subjects: `body.` and the paths below it, from the response schema. */
export function assertionCompletion(getSchema: () => Schema | null) {
  return (c: CompletionContext): CompletionResult | null => {
    const line = c.state.doc.lineAt(c.pos);
    const before = line.text.slice(0, c.pos - line.from);
    const m = /(?:^|[\s(])body((?:\.[\w$]+|\[\*\]|\[\d+\])*)\.([\w$]*)$/.exec(before);
    if (!m) return null;
    const schema = getSchema();
    if (!schema) return null;
    const fields = fieldsAfter(schema, m[1].replace(/^\./, ''));
    if (!fields.length) return null;
    return {
      from: c.pos - m[2].length,
      options: fields.map((f) => ({
        label: f.name,
        detail: f.type_text,
        info: f.required ? 'required' : undefined,
        type: 'property',
        apply: f.schema?.kind === 'array' ? `${f.name}[*]` : f.name,
      })),
      validFor: /^[\w$]*$/,
    };
  };
}

/** Completion of the keys of a JSON request body, at the caret's depth, from the request schema. */
export function jsonKeyCompletion(getSchema: () => Schema | null) {
  return (c: CompletionContext): CompletionResult | null => {
    const text = c.state.doc.toString();
    const before = text.slice(0, c.pos);
    const m = /"([\w$]*)$/.exec(before);
    if (!m) return null;
    // Only where a key can start: after `{` or `,` (and whitespace).
    const lead = before.slice(0, before.length - m[0].length).trimEnd();
    if (!(lead.endsWith('{') || lead.endsWith(','))) return null;
    const schema = getSchema();
    if (!schema) return null;
    const fields = follow(schema, jsonContext(text, c.pos - m[0].length))?.fields ?? [];
    if (!fields.length) return null;
    return {
      from: c.pos - m[1].length,
      options: fields.map((f) => ({ label: f.name, detail: f.type_text, info: f.required ? 'required' : undefined, type: 'property', apply: `${f.name}": ` })),
      validFor: /^[\w$]*$/,
    };
  };
}
