import { describe, it, expect, vi } from 'vitest';
import { StringStream } from '@codemirror/language';
import Prism from 'prismjs';

// The completion and hover sources pull in the whole editor; the tokenizer needs none of it.
vi.mock('./lsp-lang', () => ({ backendCompletionSource: () => () => null, backendHoverSource: () => () => null }));

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
