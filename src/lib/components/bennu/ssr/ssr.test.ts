import { describe, expect, it } from 'vitest';
import { diffLines, hunksOf, changeCounts } from './ssr-diff';
import { byteToIndex, contextAround, hunkFor } from './ssr-context';
import {
  formatElapsed, groupByFile, hasInClause, insertVariable, nextHoleName, parentOf, queryDiagnostics, relativeTo,
  replacementDiagnostics, stepIndex, summary, withScope,
} from './ssr-model';
import type { SsrExplained } from '$lib/ipc/bennu/ssr';

const explained = (over: Partial<SsrExplained>): SsrExplained => ({
  error: null, errorLine: 0, alternatives: 1, captures: [], expansion: [], literals: [], ...over,
});

describe('scope', () => {
  it('adds the scope as the language\'s own `in` clause', () => {
    expect(withScope('log.$m$($a$)', { kind: 'directory', path: 'core/src' })).toBe('log.$m$($a$)\nin core/src');
    expect(withScope('x\n', { kind: 'file', path: 'a/B.java' })).toBe('x\nin a/B.java');
  });

  it('leaves the whole project and a query with its own `in` alone', () => {
    expect(withScope('x', { kind: 'project', path: '' })).toBe('x');
    expect(withScope('x\nin web', { kind: 'module', path: 'core' })).toBe('x\nin web');
    expect(hasInClause('a\n  in b')).toBe(true);
    expect(hasInClause('index.of($a$)')).toBe(false);
  });

  it('makes paths project-relative', () => {
    expect(relativeTo('C:\\p\\app', 'C:/p/app/core/A.java')).toBe('core/A.java');
    expect(relativeTo('/p/app/', '/p/app')).toBe('');
    expect(parentOf('core/src/A.java')).toBe('core/src');
    expect(parentOf('A.java')).toBe('');
  });
});

describe('results', () => {
  it('groups hits by file in walk order', () => {
    const hits = [
      { file: '/a', rel: 'a', n: 1 }, { file: '/b', rel: 'b', n: 2 }, { file: '/a', rel: 'a', n: 3 },
    ];
    const groups = groupByFile(hits);
    expect(groups.map((g) => [g.rel, g.hits.length])).toEqual([['a', 2], ['b', 1]]);
  });

  it('words the count and the time', () => {
    expect(summary(1, 1)).toBe('1 match in 1 file');
    expect(summary(12, 3)).toBe('12 matches in 3 files');
    expect(formatElapsed(934)).toBe('934ms');
    expect(formatElapsed(4200)).toBe('4.2s');
    expect(formatElapsed(65_000)).toBe('1m 05s');
  });

  it('steps round the list', () => {
    expect(stepIndex(2, 1, 3)).toBe(0);
    expect(stepIndex(0, -1, 3)).toBe(2);
    expect(stepIndex(0, 1, 0)).toBe(-1);
  });
});

describe('the query\'s problems', () => {
  it('underlines the line the parser named, in bytes', () => {
    const q = 'é($a$)\nbroken $';
    const [d] = queryDiagnostics(q, explained({ error: 'unclosed hole', errorLine: 2 }));
    expect(d.message).toBe('unclosed hole');
    expect(d.from).toBe(new TextEncoder().encode('é($a$)\n').length);
    expect(queryDiagnostics(q, explained({}))).toEqual([]);
  });

  it('flags a replacement capture the query does not bind', () => {
    const out = replacementDiagnostics('f($a$, $z$)', ['a']);
    expect(out).toHaveLength(1);
    expect(out[0].message).toContain('$z$');
    expect(replacementDiagnostics('f($a$)', ['a'])).toEqual([]);
  });

  it('names a new hole after the first free letter', () => {
    expect(nextHoleName([])).toBe('a');
    expect(nextHoleName(['a', 'b'])).toBe('c');
    expect(nextHoleName(['a', 'c'])).toBe('b');
  });

  it('inserts a variable at the caret without gluing it to a word', () => {
    expect(insertVariable('foo(', 4, 'a')).toEqual({ text: 'foo($a$', caret: 7 });
    expect(insertVariable('foo', 3, 'a').text).toBe('foo $a$');
  });
});

describe('the hover context', () => {
  const text = 'a\nb\nlog.debug("é" + x);\nd\ne\nf\ng\n';
  // Byte offsets, as the backend reports them: `é` is two bytes.
  const start = new TextEncoder().encode('a\nb\n').length;
  const end = start + new TextEncoder().encode('log.debug("é" + x)').length;

  it('maps a byte offset past multi-byte characters', () => {
    expect(byteToIndex('é-x', 0)).toBe(0);
    expect(byteToIndex('é-x', 2)).toBe(1);
    expect(byteToIndex('é-x', 99)).toBe(3);
  });

  it('shows the lines around the match with the match marked', () => {
    const ctx = contextAround(text, { start, end }, 2);
    expect(ctx.startLine).toBe(3);
    expect(ctx.lines.map((l) => l.no)).toEqual([1, 2, 3, 4, 5]);
    const hit = ctx.lines.find((l) => l.no === 3)!;
    expect(hit.text.slice(hit.from, hit.to)).toBe('log.debug("é" + x)');
    expect(ctx.lines.find((l) => l.no === 2)!.from).toBeUndefined();
    expect(ctx.cut).toBe(false);
  });

  it('marks each line of a match that spans several, and cuts a very long one', () => {
    const long = Array.from({ length: 30 }, (_, i) => `line${i}`).join('\n');
    const ctx = contextAround(long, { start: 0, end: long.length }, 1);
    expect(ctx.cut).toBe(true);
    expect(ctx.lines.filter((l) => l.from !== undefined)).toHaveLength(12);
    const small = contextAround('x\nyy\nzz\n', { start: 2, end: 7 }, 0);
    expect(small.lines.map((l) => [l.no, l.from, l.to])).toEqual([[2, 0, 2], [3, 0, 2]]);
  });

  it('picks the hunk of the replacement that belongs to a match', () => {
    const before = 'a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n';
    const after = 'a\nB\nc\nd\ne\nf\ng\nh\ni\nJ\n';
    const hunks = hunksOf(before, after, 1);
    expect(hunkFor(hunks, 2, 2)?.lines.some((l) => l.text === 'B')).toBe(true);
    expect(hunkFor(hunks, 10, 10)?.lines.some((l) => l.text === 'J')).toBe(true);
    expect(hunkFor(hunks, 5, 5)).toBeNull();
  });
});

describe('the replace diff', () => {
  const before = 'a\nb\nc\nd\ne\nf\ng\nh\n';
  const after = 'a\nB\nc\nd\ne\nf\ng\nH\n';

  it('pairs removed and added lines and numbers both sides', () => {
    const lines = diffLines(before, after);
    expect(lines.filter((l) => l.kind === 'del').map((l) => l.text)).toEqual(['b', 'h']);
    expect(lines.filter((l) => l.kind === 'add').map((l) => l.text)).toEqual(['B', 'H']);
    expect(lines.find((l) => l.text === 'c')).toMatchObject({ oldNo: 3, newNo: 3 });
  });

  it('keeps only the changed hunks with context, far-apart changes apart', () => {
    const hunks = hunksOf(before, after, 1);
    expect(hunks).toHaveLength(2);
    expect(changeCounts(before, after)).toEqual({ added: 2, removed: 2 });
  });

  it('has no hunk when nothing changed', () => {
    expect(hunksOf('x\ny', 'x\ny')).toEqual([]);
  });
});
