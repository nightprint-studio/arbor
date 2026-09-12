/**
 * Hover inside a Jinja template: what the name under the pointer is — a field of the data a code template is
 * rendered with, a name the template bound, a filter, a test, a statement, a `bennu.` directive.
 *
 * Read off the model completion offers (`jinja-intel.ts`), so a card and the completion list cannot disagree
 * about a template's data. Outside a tag the text is the language the template writes, and a card about it
 * would be a guess — so there is none.
 */

import type { EditorView, Tooltip } from '@codemirror/view';
import { hoverCardDom, type HoverCard } from '$lib/components/shared/ui/code-editor';
import {
  BUILTIN_FILTERS,
  FUNCTIONS,
  LOOP,
  TESTS,
  activeTemplateKind,
  bindingsIn,
  itemsOf,
  loadSchema,
  membersOf,
  openTag,
  resolve,
  typeLabel,
  type JsonSchema,
} from './jinja-intel';

/** What each directive says — Template reference's table, a line each. */
const DIRECTIVES: Record<string, string> = {
  description: 'What the template is for, shown next to its name in every list and in completion.',
  output: 'For a template from a class: `members` — inserted into the class, the default; `file` — a new file beside it; `text` — to copy.',
  file: "The name of the file written, relative to the class's folder or the folder chosen. It is a template too: `{{ class.name }}Repository.java`.",
  requires: 'What a project needs for the template to be offered, comma-separated: `java >= 16`, `lombok`, `spring-boot >= 3`, `!lombok`.',
  constants: 'For validation tests: the classes to look up the constants holding each expected message in.',
};

/** The statements a code template uses, by the name that opens them — a closing name reads its opener's. */
const STATEMENTS: Record<string, [string, string]> = {
  if: ['{% if … %} … {% elif … %} … {% else %} … {% endif %}', 'Writes a part only when its condition holds.'],
  for: ['{% for item in list %} … {% endfor %}', 'Writes a part once for each item. Inside it, `loop` says where the loop is: `loop.first`, `loop.last`, `loop.index`.'],
  set: ['{% set name = value %}', 'A name for a value, from here to the end of the block it is in.'],
  macro: ['{% macro name(arguments) %} … {% endmacro %}', 'A piece of template written again wherever it is called: `{{ name(…) }}`.'],
  call: ['{% call name(arguments) %} … {% endcall %}', 'Calls a macro, with this block as what `caller()` writes inside it.'],
  filter: ['{% filter name %} … {% endfilter %}', 'A filter applied to everything the block writes.'],
  with: ['{% with name = value %} … {% endwith %}', 'Names that exist only inside the block.'],
  raw: ['{% raw %} … {% endraw %}', 'Written exactly as it is, tags included.'],
};

/** The hover source for a Jinja template. */
export function jinjaHover() {
  return (view: EditorView, pos: number): Promise<Tooltip | null> => hover(view, pos);
}

async function hover(view: EditorView, pos: number): Promise<Tooltip | null> {
  const line = view.state.doc.lineAt(pos);
  const text = line.text;
  let start = pos - line.from;
  let end = start;
  while (start > 0 && /\w/.test(text[start - 1])) start--;
  while (end < text.length && /\w/.test(text[end])) end++;
  if (start === end) return null;
  const from = line.from + start;
  const before = view.state.doc.sliceString(Math.max(0, from - 20_000), from);
  const card = await cardFor(text.slice(start, end), before);
  if (!card) return null;
  return { pos: from, end: line.from + end, above: true, create: () => ({ dom: hoverCardDom(card) }) };
}

async function cardFor(word: string, before: string): Promise<HoverCard | null> {
  if (/\{#-?\s*bennu\.$/.test(before)) {
    const doc = DIRECTIVES[word];
    return doc ? { kind: 'directive', signature: `bennu.${word}`, doc } : null;
  }
  const tag = openTag(before);
  if (tag === null) return null;
  if (/^\{%-?\s*$/.test(tag)) return statementCard(word);
  if (/\|\s*$/.test(tag)) return filterCard(word);
  if (/\bis\s+(?:not\s+)?$/.test(tag)) {
    const doc = described(TESTS, word);
    return doc ? { kind: 'test', signature: `is ${word}`, doc } : null;
  }

  const schema = await loadSchema(activeTemplateKind());
  const context = (schema?.context as JsonSchema) ?? {};
  const bound = bindingsIn(before);
  const member = /([A-Za-z_][\w.]*)\.$/.exec(tag);
  if (member) {
    const path = `${member[1]}.${word}`;
    return dataCard(path, resolve(path, context, bound));
  }
  const binding = bound.get(word);
  if (binding) {
    const target = resolve(binding.path, context, bound);
    const shape = binding.each ? itemsOf(target) : target;
    return {
      kind: 'variable',
      signature: typed(word, shape),
      container: binding.each ? `each of ${binding.path}` : `= ${binding.path}`,
      doc: shape?.description ?? null,
    };
  }
  if (word === 'loop') return { kind: 'variable', signature: 'loop', doc: LOOP.description ?? null };
  const own = membersOf(context)[word];
  if (own) return dataCard(word, own);
  const doc = described(FUNCTIONS, word);
  return doc ? { kind: 'function', signature: `${word}()`, doc } : null;
}

function statementCard(word: string): HoverCard | null {
  const opener = word.startsWith('end') ? word.slice(3) : word === 'elif' || word === 'else' ? 'if' : word;
  const entry = STATEMENTS[opener];
  return entry ? { kind: 'statement', signature: entry[0], doc: entry[1] } : null;
}

async function filterCard(word: string): Promise<HoverCard | null> {
  const schema = await loadSchema(activeTemplateKind());
  const custom = schema?.filters.find((filter) => filter.name === word);
  if (custom) return { kind: 'filter', signature: `| ${word}`, container: 'Bennu', doc: custom.doc };
  const doc = described(BUILTIN_FILTERS, word);
  return doc ? { kind: 'filter', signature: `| ${word}`, doc } : null;
}

/** A field of the data — or a method, whose description starts with its call: `local(type) — …`. */
function dataCard(path: string, schema: JsonSchema | undefined): HoverCard | null {
  if (!schema) return null;
  const call = schema['x-method'] ? /^(\w+\([^)]*\))\s*—\s*([\s\S]*)$/.exec(schema.description ?? '') : null;
  if (call) {
    const owner = path.includes('.') ? path.slice(0, path.lastIndexOf('.') + 1) : '';
    return { kind: 'method', signature: `${owner}${call[1]}`, doc: call[2] };
  }
  return { kind: 'field', signature: typed(path, schema), doc: schema.description ?? null };
}

function typed(name: string, schema: JsonSchema | undefined): string {
  const type = typeLabel(schema);
  return type ? `${name}: ${type}` : name;
}

function described(catalog: readonly [string, string][], word: string): string | null {
  return catalog.find(([name]) => name === word)?.[1] ?? null;
}
