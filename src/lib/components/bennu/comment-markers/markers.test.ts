import { describe, expect, it } from 'vitest';
import { commentSpans } from './comments';
import { DEFAULT_KEYWORDS, effectiveKeywords, findMarkers, parseKeywords, toneOf } from './markers';

const OPTS = { keywords: [...DEFAULT_KEYWORDS], caseSensitive: true };
const words = (src: string, path: string, opts = OPTS) => findMarkers(src, path, opts).map((h) => h.keyword);

describe('where the comments are', () => {
  it('knows a // inside a string is not a comment', () => {
    const src = 'String u = "http://x/TODO"; // real\n';
    const spans = commentSpans(src, 'A.java');
    expect(spans).toHaveLength(1);
    expect(src.slice(spans[0].from, spans[0].to)).toBe('// real');
  });

  it('finds block and line comments in Java, JS and Rust', () => {
    expect(commentSpans('/* a */ int x; // b', 'A.java')).toHaveLength(2);
    expect(commentSpans('const r = /\\//; // c', 'a.ts')).toHaveLength(1);
    expect(commentSpans('/* /* nested */ */ fn f() {}', 'a.rs')).toHaveLength(1);
  });

  it('reads other languages by their own syntax', () => {
    expect(commentSpans('<a><!-- x --></a>', 'p.xml')).toHaveLength(1);
    expect(commentSpans('k: v # note\nq: "# not"', 'c.yaml')).toHaveLength(1);
    expect(commentSpans('select 1 -- one', 'q.sql')).toHaveLength(1);
    expect(commentSpans('anything', 'a.unknownext')).toEqual([]);
  });
});

describe('finding markers', () => {
  it('colours a marker in a comment and ignores one in a string or in code', () => {
    const src = 'String s = "TODO in a string";\n// TODO: real\nint TODO = 1;\n';
    const hits = findMarkers(src, 'A.java', OPTS);
    expect(hits.map((h) => [h.keyword, h.line])).toEqual([['TODO', 2]]);
  });

  it('takes the author in brackets with the word', () => {
    const src = '// FIXME(alice): broken\n';
    const [hit] = findMarkers(src, 'A.java', OPTS);
    expect(src.slice(hit.from, hit.to)).toBe('FIXME(alice)');
  });

  it('respects word boundaries', () => {
    expect(words('// TODOLIST and XTODO and TODO_X\n', 'A.java')).toEqual([]);
    expect(words('// TODO.\n', 'A.java')).toEqual(['TODO']);
  });

  it('is case sensitive unless asked', () => {
    expect(words('// todo: x\n', 'A.java')).toEqual([]);
    expect(words('// todo: x\n', 'A.java', { ...OPTS, caseSensitive: false })).toEqual(['TODO']);
  });

  it('finds several markers across lines, in block comments too', () => {
    const src = '/*\n * NOTE a\n * BUG b\n */\n// HACK c\n';
    expect(findMarkers(src, 'A.java', OPTS).map((h) => [h.keyword, h.line])).toEqual([['NOTE', 2], ['BUG', 3], ['HACK', 5]]);
  });

  it('does not scan comments at all when no word is there', () => {
    expect(findMarkers('// nothing to see\n', 'A.java', OPTS)).toEqual([]);
  });

  it('works with a custom list', () => {
    expect(words('// REVIEW later\n// TODO x\n', 'A.java', { keywords: ['REVIEW'], caseSensitive: true })).toEqual(['REVIEW']);
  });
});

describe('the word list', () => {
  it('parses separators, drops duplicates and invalid words', () => {
    expect(parseKeywords('todo, FIXME;  todo  a-b\nHACK')).toEqual(['todo', 'FIXME', 'HACK']);
  });

  it('falls back to the built-in words when empty', () => {
    expect(effectiveKeywords([])).toEqual([...DEFAULT_KEYWORDS]);
    expect(effectiveKeywords(['REVIEW'])).toEqual(['REVIEW']);
  });

  it('gives each kind its tone, and an unknown word the accent', () => {
    expect(toneOf('fixme')).toBe('danger');
    expect(toneOf('HACK')).toBe('warning');
    expect(toneOf('REVIEW')).toBe('accent');
  });
});
