/**
 * Completion inside a Jinja template's tags — what can be written where: the statement names after `{%`, the
 * filters after `|`, the tests after `is`, the names the template binds, and in a code template the data its kind
 * is rendered with. The model is `jinja-intel.ts`'s, shared with hover.
 */

import type { Completion, CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';
import type { TemplateKindId } from '$lib/ipc/bennu/templates';
import { JINJA_TAGS } from '$lib/utils/jinja-words';
import {
  BUILTIN_FILTERS,
  EXPRESSION_WORDS,
  FUNCTIONS,
  LOOP,
  TESTS,
  activeTemplateKind,
  bindingsIn,
  insideLoop,
  kindValues,
  loadSchema,
  membersOf,
  namesDefinedIn,
  openTag,
  resolve,
  typeLabel,
  type JsonSchema,
} from './jinja-intel';

/** Completion for a Jinja template. Completion runs in the editor showing the active file, so that
 *  file's folder says which kind of code template it is — and a Jinja file kept anywhere else gets
 *  the language alone. */
export function jinjaCompletion(): CompletionSource {
  return (context) => complete(context, activeTemplateKind());
}

async function complete(ctx: CompletionContext, kind: TemplateKindId | null): Promise<CompletionResult | null> {
  const before = ctx.state.doc.sliceString(Math.max(0, ctx.pos - 20_000), ctx.pos);
  const tag = openTag(before);
  if (tag === null) return null;
  const word = ctx.matchBefore(/[A-Za-z_]\w*$/);
  const typed = word ? word.to - word.from : 0;
  const lead = tag.slice(0, tag.length - typed);
  const from = word ? word.from : ctx.pos;
  const triggered = /[.|]\s*$/.test(lead) || /^\{%-?\s*$/.test(lead) || /\bis\s+$/.test(lead) || /["']$/.test(lead);
  if (!word && !ctx.explicit && !triggered) return null;
  const result = (options: Completion[]): CompletionResult | null =>
    options.length > 0 ? { from, options, validFor: /^\w*$/ } : null;

  const bound = bindingsIn(before);

  // `value.kind == "…"` — the kinds a sample value can be.
  const tagged = /([A-Za-z_][\w.]*)\.kind\s*[!=]=\s*["']$/.exec(lead);
  if (tagged) {
    const schema = await loadSchema(kind);
    const target = schema ? resolve(tagged[1], schema.context as JsonSchema, bound) : undefined;
    return result(kindValues(target).map((label) => ({ label, type: 'constant', detail: 'kind' })));
  }

  if (/\|\s*$/.test(lead)) {
    const schema = await loadSchema(kind);
    const custom = (schema?.filters ?? []).map((f) => ({ label: f.name, type: 'function', detail: 'Bennu', info: f.doc, boost: 1 }));
    const builtin = BUILTIN_FILTERS.map(([label, info]) => ({ label, type: 'function', info }));
    return result([...custom, ...builtin]);
  }

  if (/\bis\s+(?:not\s+)?$/.test(lead)) {
    return result(TESTS.map(([label, info]) => ({ label, type: 'function', info })));
  }

  if (/^\{%-?\s*$/.test(lead)) {
    return result(JINJA_TAGS.map((label) => ({ label, type: 'keyword' })));
  }

  const member = /([A-Za-z_][\w.]*)\.$/.exec(lead);
  if (member) {
    const schema = await loadSchema(kind);
    const target = resolve(member[1], (schema?.context as JsonSchema) ?? {}, bound);
    return result(
      Object.entries(membersOf(target)).map(([label, s]) => ({
        label,
        type: s['x-method'] ? 'method' : 'property',
        detail: typeLabel(s),
        info: s.description,
      })),
    );
  }

  const schema = await loadSchema(kind);
  const options: Completion[] = [];
  const seen = new Set<string>();
  const add = (option: Completion) => {
    if (seen.has(option.label)) return;
    seen.add(option.label);
    options.push(option);
  };
  for (const [name, binding] of bound) {
    add({ label: name, type: 'variable', detail: binding.each ? `each of ${binding.path}` : binding.path, boost: 2 });
  }
  for (const name of namesDefinedIn(before)) add({ label: name, type: 'variable', boost: 2 });
  if (insideLoop(before)) add({ label: 'loop', type: 'variable', detail: 'object', info: LOOP.description, boost: 1 });
  if (schema) {
    for (const [label, s] of Object.entries(membersOf(schema.context as JsonSchema))) {
      add({ label, type: 'variable', detail: typeLabel(s), info: s.description, boost: 1 });
    }
  }
  for (const [label, info] of FUNCTIONS) add({ label, type: 'function', info });
  for (const label of EXPRESSION_WORDS) add({ label, type: 'keyword' });
  return result(options);
}
