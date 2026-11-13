import { describe, it, expect, vi } from 'vitest';
import { StringStream } from '@codemirror/language';
import Prism from 'prismjs';

// The completion and hover sources pull in the whole editor; the tokenizer needs none of it.
vi.mock('./lsp-lang', () => ({
  backendCompletionSource: () => () => null,
  eagerBackendCompletionSource: () => () => null,
  keyBackendCompletionSource: () => () => null,
  backendHoverSource: () => () => null,
}));
// Comment rendering pulls in the markdown editor (DOM widgets, Tauri, mermaid); the tokenizer needs none of it.
vi.mock('$lib/utils/comment-markdown', () => ({ commentMarkdown: () => [] }));

import { patinaMode } from './patina-lang';
import '$lib/utils/prism-languages/patina';

/** Tokenise one line with a real {@link StringStream}, as `StreamLanguage` would. */
function tokens(line: string): Array<[string, string | null]> {
  const state = patinaMode.startState!(2);
  const stream = new StringStream(line, 2, 2, undefined);
  const out: Array<[string, string | null]> = [];
  let guard = 0;
  while (!stream.eol()) {
    const before = stream.pos;
    const style = patinaMode.token(stream, state);
    expect(stream.pos, `stalled at ${before} in ${JSON.stringify(line)}`).toBeGreaterThan(before);
    out.push([line.slice(before, stream.pos), style]);
    stream.start = stream.pos;
    if (++guard > 200) throw new Error('runaway');
  }
  return out;
}

const styleOf = (line: string, text: string) => tokens(line).find(([s]) => s === text)?.[1];

describe('patina stream mode: primitive types', () => {
  it('colours a primitive in an annotation, a signature and a return type', () => {
    expect(styleOf('let x: int = 3', 'int')).toBe('primitiveType');
    expect(styleOf('fn f(a: str) -> fx {', 'str')).toBe('primitiveType');
    expect(styleOf('fn f(a: str) -> fx {', 'fx')).toBe('primitiveType');
  });

  it('colours a primitive inside generics and after `as`', () => {
    expect(styleOf('List<int>', 'int')).toBe('primitiveType');
    expect(styleOf('Option<bool>', 'bool')).toBe('primitiveType');
    expect(styleOf('x as f32', 'f32')).toBe('primitiveType');
  });

  it('keeps other types, associated calls and keywords apart', () => {
    expect(styleOf('List<int>', 'List')).toBe('typeName');
    expect(styleOf('let p: BlockPos = x', 'BlockPos')).toBe('typeName');
    expect(styleOf('Foo::new()', 'new')).toBe('staticCall');
    expect(styleOf('let x: int = 3', 'let')).toBe('keyword');
    expect(styleOf('fn f(a: str) -> fx {', 'fn')).toBe('keyword');
  });

  it('does not colour a name that happens to spell a primitive', () => {
    expect(styleOf('p.str', 'str')).toBe('propertyName');
    expect(styleOf('Foo { str: 1 }', 'str')).toBe('propertyName');
    expect(styleOf('fn f(str: int) {', 'str')).not.toBe('primitiveType');
    expect(styleOf('int::MAX', 'int')).toBe('namespace');
  });
});

describe('patina Prism grammar: primitive types', () => {
  const classes = (code: string, text: string): string[] => {
    const walk = (toks: Array<string | Prism.Token>): string[] | null => {
      for (const t of toks) {
        if (typeof t === 'string') continue;
        if (typeof t.content === 'string' && t.content === text) return [t.type, ...(Array.isArray(t.alias) ? t.alias : t.alias ? [t.alias] : [])];
        if (Array.isArray(t.content)) {
          const r = walk(t.content);
          if (r) return r;
        }
      }
      return null;
    };
    return walk(Prism.tokenize(code, Prism.languages.patina)) ?? [];
  };

  it('tokenises primitives as `primitive-type`, apart from keywords and other types', () => {
    expect(classes('let x: int = 3', 'int')).toContain('primitive-type');
    expect(classes('fn f(a: str) -> fx {', 'fx')).toContain('primitive-type');
    expect(classes('Option<bool>', 'bool')).toContain('primitive-type');
    expect(classes('let x: int = 3', 'let')).toContain('keyword');
    expect(classes('Option<bool>', 'Option')).toContain('class-name');
  });

  it('leaves a field called like a primitive alone', () => {
    expect(classes('Foo { str: 1 }', 'str')).not.toContain('primitive-type');
  });
});

/** Tokenise several lines, carrying the state across them as `StreamLanguage` does. */
function tokensOver(lines: string[]): Array<[string, string | null]> {
  const state = patinaMode.startState!(2);
  const out: Array<[string, string | null]> = [];
  for (const line of lines) {
    const stream = new StringStream(line, 2, 2, undefined);
    let guard = 0;
    while (!stream.eol()) {
      const before = stream.pos;
      const style = patinaMode.token(stream, state);
      expect(stream.pos).toBeGreaterThan(before);
      out.push([line.slice(before, stream.pos), style]);
      stream.start = stream.pos;
      if (++guard > 200) throw new Error('runaway');
    }
  }
  return out;
}

describe('patina stream mode: attributes', () => {
  it('colours the `#` and the name of a bare attribute', () => {
    expect(tokens('#[server]')).toEqual([['#', 'attribute'], ['[', null], ['server', 'attribute'], [']', null]]);
    expect(styleOf('#[worldgen]', 'worldgen')).toBe('attribute');
    expect(styleOf('#[protected]', 'protected')).toBe('attribute');
    expect(styleOf('#![allow]', '#')).toBe(undefined); // `#!` is one token, below
    expect(tokens('#![allow]')[0]).toEqual(['#!', 'attribute']);
  });

  it('names the attribute and leaves the arguments to their own colours', () => {
    const line = '#[on(DecorateChunk, decoration: "dungeon_doors")]';
    expect(styleOf(line, 'on')).toBe('attribute');
    expect(styleOf(line, 'DecorateChunk')).toBe('typeName');
    expect(styleOf(line, 'decoration')).toBe('propertyName');
    expect(styleOf(line, '"dungeon_doors"')).toBe('string');
    expect(styleOf('#[command(name: "tp", ok: true)]', 'command')).toBe('attribute');
  });

  it('does not mistake a name in the arguments for the attribute name', () => {
    const t = tokens('#[on(StepOn)]').filter(([, s]) => s === 'attribute').map(([x]) => x);
    expect(t).toEqual(['#', 'on']);
  });

  it('colours the name when the `#[` and the name are on different lines, and after the attribute ends', () => {
    const out = tokensOver(['#[', '    server', ']', 'fn f() {}']);
    expect(out.filter(([, s]) => s === 'attribute').map(([x]) => x)).toEqual(['#', 'server']);
    expect(out).toContainEqual(['fn', 'keyword']);
    expect(out).toContainEqual(['f', 'variableName.definition']);
  });

  it('colours each attribute of a stack, one per line', () => {
    const out = tokensOver(['#[server]', '#[protected]', '#[command(name: "x")]', 'fn f() {}']);
    expect(out.filter(([, s]) => s === 'attribute').map(([x]) => x)).toEqual(['#', 'server', '#', 'protected', '#', 'command']);
  });

  it('an attribute that never closes stops at the next `{`, not at the end of the file', () => {
    const out = tokensOver(['#[server', 'fn f() {', '    let x = 1;', '}']);
    expect(out).toContainEqual(['fn', 'keyword']);
    expect(out).toContainEqual(['let', 'keyword']);
  });

  it('a lone `#` is not an attribute', () => {
    expect(styleOf('a # b', '#')).toBe('punctuation');
  });
});

describe('patina stream mode: strings that carry an id or a hole', () => {
  it('keeps a plain string as one token', () => {
    expect(tokens('let a = "hello"')).toContainEqual(['"hello"', 'string']);
  });

  it('gives the argument of t!(…) the text-key colour and the other catalog forms the id colour', () => {
    expect(styleOf('let a = t!("ui:avvio.solo")', '"ui:avvio.solo"')).toBe('textKey');
    expect(styleOf('let a = block!("tnt")', '"tnt"')).toBe('catalogId');
    expect(styleOf('let a = ui!("barra", "matassa-1")', '"barra"')).toBe('catalogId');
    expect(styleOf('let a = ui!("barra", "matassa-1")', '"matassa-1"')).toBe('catalogId');
  });

  it('leaves a string that is not directly the argument of a form as text', () => {
    expect(styleOf('let a = t!(f("x"))', '"x"')).toBe('string');
    expect(styleOf('assert!(ok, "message")', '"message"')).toBe('string');
    expect(styleOf('log!("line")', '"line"')).toBe('string');
    expect(styleOf('let a = t!("k") + "plain"', '"plain"')).toBe('string');
  });

  it('cuts a string around its interpolation holes', () => {
    expect(tokens('let a = "hi {p.name} and {n}!"').filter(([, st]) => st !== null)).toEqual([
      ['let', 'keyword'], ['a', 'variableName'], ['=', 'operator'],
      ['"', 'string'], ['hi ', 'string'], ['{', 'holeBrace'], ['p.name', 'holePath'], ['}', 'holeBrace'],
      [' and ', 'string'], ['{', 'holeBrace'], ['n', 'holePath'], ['}', 'holeBrace'], ['!', 'string'], ['"', 'string'],
    ]);
  });

  it('reads {{ }} and a lone { as text, and an escape as part of the string', () => {
    expect(tokens('let a = "{{x}} {1}"')).toContainEqual(['"{{x}} {1}"', 'string']);
    const out = tokens('let a = "{{x}}"');
    expect(out.filter(([, s]) => s === 'holeBrace')).toEqual([]);
    expect(tokens('let a = "{1}"').filter(([, s]) => s === 'holeBrace')).toEqual([]);
  });

  it('colours a hole inside a catalog form the same way and ends the string at the closing quote', () => {
    const out = tokens('let a = t!("k {x}") + 1');
    expect(out).toContainEqual(['x', 'holePath']);
    expect(out).toContainEqual(['1', 'number']);
    expect(out.find(([s]) => s === 'k ')?.[1]).toBe('textKey');
  });

  it('forgets an unterminated string at the end of its line', () => {
    const out = tokensOver(['let a = "open {x', 'let b = 2']);
    expect(out).toContainEqual(['let', 'keyword']);
    expect(out.filter(([s]) => s === 'b').map(([, st]) => st)).toEqual(['variableName']);
  });
});
