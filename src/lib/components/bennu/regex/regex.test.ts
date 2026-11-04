/**
 * The regex model: parsing and its diagnostics, the Java-semantics matcher, substitution, split,
 * the explanation and the highlight spans.
 */

import { describe, expect, it } from 'vitest';
import { explainRegex } from './explain';
import { runRegex } from './execute';
import { parseRegex } from './parse';
import { segmentsOf } from './segments';
import { highlightRegex } from './tokens';
import { toJavaCompile, toJavaString, toJsLiteral, toRustRegex } from './export-code';
import type { Flavour, MatchMode, RunResult } from './types';

function run(
  pattern: string, text: string,
  o: { flavour?: Flavour; flags?: string; mode?: MatchMode; replacement?: string } = {},
): RunResult {
  return runRegex({
    flavour: o.flavour ?? 'java', pattern, flags: o.flags ?? '', text, mode: o.mode ?? 'find',
    replacement: o.replacement ?? null, maxMatches: 1000,
  });
}

const found = (r: RunResult) => r.matches.map((m) => m.groups[0]!.text);

describe('parse', () => {
  it('counts capturing groups and names them', () => {
    const p = parseRegex('(a)(?<year>\\d+)(?:x)', 'java');
    expect(p.errors).toEqual([]);
    expect(p.groupCount).toBe(2);
    expect(p.groupNames).toEqual([null, null, 'year']);
  });

  it('reports an unclosed group on the opening parenthesis', () => {
    const p = parseRegex('a(b', 'java');
    expect(p.errors[0]).toMatchObject({ from: 1, message: 'Unclosed group' });
  });

  it('reports an unmatched closing parenthesis', () => {
    expect(parseRegex('a)b', 'java').errors[0].message).toBe("Unmatched closing ')'");
  });

  it('reports a dangling quantifier and an unclosed class', () => {
    expect(parseRegex('*a', 'java').errors[0].message).toBe("Dangling meta character '*'");
    expect(parseRegex('[abc', 'java').errors[0].message).toBe('Unclosed character class');
  });

  it('reports a reversed range and an unknown property', () => {
    expect(parseRegex('[z-a]', 'java').errors[0].message).toBe('Illegal character range');
    expect(parseRegex('\\p{NoSuchThing}', 'java').errors[0].message).toContain('Unknown character property');
  });

  it('gates syntax by flavour', () => {
    expect(parseRegex('a*+', 'java').errors).toEqual([]);
    expect(parseRegex('a*+', 'js').errors.length).toBeGreaterThan(0);
    expect(parseRegex('(?<=a)b', 'rust').errors.length).toBeGreaterThan(0);
    expect(parseRegex('(?<=a)b', 'js').errors).toEqual([]);
    expect(parseRegex('(?P<n>a)', 'rust').errors).toEqual([]);
    expect(parseRegex('(?P<n>a)', 'java').errors.length).toBeGreaterThan(0);
  });

  it('rejects an unbounded look-behind in Java only', () => {
    expect(parseRegex('(?<=a+)b', 'java').errors[0].message).toContain('Look-behind');
    expect(parseRegex('(?<=a{1,3})b', 'java').errors).toEqual([]);
  });

  it('flags a back-reference to a name that does not exist', () => {
    expect(parseRegex('\\k<nope>', 'java').errors[0].message).toContain('does not exist');
  });
});

describe('java matching', () => {
  it('finds every match, leftmost first', () => {
    expect(found(run('\\d+', 'a1 b22 c333'))).toEqual(['1', '22', '333']);
  });

  it('is greedy by default, lazy with ?, and possessive with +', () => {
    expect(found(run('<.+>', '<a><b>'))).toEqual(['<a><b>']);
    expect(found(run('<.+?>', '<a><b>'))).toEqual(['<a>', '<b>']);
    expect(run('a*+a', 'aaaa').matches).toHaveLength(0);
  });

  it('captures groups, named and numbered', () => {
    const r = run('(?<k>\\w+)=(\\d+)', 'x=10');
    expect(r.matches[0].groups.map((g) => g?.text)).toEqual(['x=10', 'x', '10']);
    expect(r.groupNames).toEqual([null, 'k', null]);
  });

  it('backtracks into alternation and groups', () => {
    expect(found(run('(a|ab)(c|bcd)', 'abcd'))).toEqual(['abcd']);
  });

  it('honours back-references and atomic groups', () => {
    expect(found(run('(\\w)\\1', 'hello'))).toEqual(['ll']);
    expect(run('(?>a+)a', 'aaa').matches).toHaveLength(0);
  });

  it('supports look-ahead and bounded look-behind', () => {
    expect(found(run('\\w+(?=;)', 'ab; cd'))).toEqual(['ab']);
    expect(found(run('(?<=\\$)\\d+', 'cost $42'))).toEqual(['42']);
    expect(found(run('(?<!\\$)\\b\\d+', '$42 7'))).toEqual(['7']);
  });

  it('applies flags, including inline ones', () => {
    expect(found(run('abc', 'ABC', { flags: 'i' }))).toEqual(['ABC']);
    expect(found(run('(?i)abc', 'ABC'))).toEqual(['ABC']);
    expect(run('a.b', 'a\nb').matches).toHaveLength(0);
    expect(run('a.b', 'a\nb', { flags: 's' }).matches).toHaveLength(1);
  });

  it('anchors per line with the multi-line flag', () => {
    expect(found(run('^\\w+', 'ab\ncd', { flags: 'm' }))).toEqual(['ab', 'cd']);
    expect(found(run('^\\w+', 'ab\ncd'))).toEqual(['ab']);
  });

  it('advances past an empty match instead of looping', () => {
    const r = run('x*', 'ab');
    expect(r.matches.map((m) => [m.start, m.end])).toEqual([[0, 0], [1, 1], [2, 2]]);
  });

  it('supports the whole-text and start-only modes', () => {
    expect(run('\\d+', '123', { mode: 'matches' }).matches).toHaveLength(1);
    expect(run('\\d+', '123a', { mode: 'matches' }).matches).toHaveLength(0);
    expect(run('\\d+', '12a', { mode: 'lookingAt' }).matches).toHaveLength(1);
  });

  it('handles character class intersection and quoting', () => {
    expect(found(run('[a-z&&[^aeiou]]+', 'hello'))).toEqual(['h', 'll']);
    expect(found(run('\\Qa.b\\E', 'a.b axb'))).toEqual(['a.b']);
  });

  it('stops a catastrophic pattern at the step budget instead of hanging', () => {
    const r = run('(a+)+$', 'a'.repeat(40) + 'b');
    expect(r.aborted).toBe(true);
  });
});

describe('substitution and split', () => {
  it('expands $1 and ${name} like Matcher.replaceAll', () => {
    expect(run('(\\w+)@(?<d>\\w+)', 'me@host', { replacement: '${d}:$1' }).replaced).toBe('host:me');
  });

  it('treats a backslash as an escape and reports a bad reference', () => {
    expect(run('a', 'a', { replacement: '\\$1' }).replaced).toBe('$1');
    expect(run('a', 'a', { replacement: '$2' }).replaceError).toBe('No group 2');
  });

  it('splits like String.split: no leading empty string, trailing ones dropped', () => {
    expect(run(',', 'a,b,,').split).toEqual(['a', 'b']);
    expect(run('', 'ab').split).toEqual(['a', 'b']);
    expect(run(',', 'abc').split).toEqual(['abc']);
  });
});

describe('javascript flavour', () => {
  it('runs on the host engine and reports group indices', () => {
    const r = run('(?<n>\\d+)', 'a1 b22', { flavour: 'js', flags: 'g' });
    expect(found(r)).toEqual(['1', '22']);
    expect(r.matches[1].groups[1]).toMatchObject({ start: 4, end: 6 });
    expect(r.groupNames).toEqual([null, 'n']);
  });

  it('stops after the first match without the g flag', () => {
    expect(run('\\d', '123', { flavour: 'js' }).matches).toHaveLength(1);
  });

  it('surfaces the engine error for a bad pattern', () => {
    expect(run('(', 'x', { flavour: 'js' }).error).toBeTruthy();
  });
});

describe('explain', () => {
  it('merges literal runs and describes the structure', () => {
    const rows = explainRegex('ab(\\d+)', 'java');
    expect(rows.map((r) => r.text)).toEqual([
      'Matches the text “ab” literally',
      'Capturing group #1',
      'Matches a digit (0–9)',
      'Repeats the previous one or more times',
      'End of group',
    ]);
    expect(rows[2].depth).toBe(1);
  });
});

describe('highlight', () => {
  const classes = (p: string) => highlightRegex(p, 'java').map((s) => [p.slice(s.from, s.to), s.cls]);

  it('colours groups, quantifiers, classes and anchors', () => {
    expect(classes('^(a|b)+\\d$')).toEqual([
      ['^', 'constant'], ['(', 'keyword'], ['|', 'operator'], [')', 'keyword'], ['+', 'operator'],
      ['\\d', 'type'], ['$', 'constant'],
    ]);
  });

  it('marks a problem as invalid with its message', () => {
    const bad = highlightRegex('a(b', 'java').find((s) => s.cls === 'invalid');
    expect(bad?.message).toBe('Unclosed group');
  });
});

describe('segments', () => {
  it('cuts the text at match and group boundaries', () => {
    const r = run('(b)c', 'abcd');
    const segs = segmentsOf('abcd', r.matches);
    expect(segs.map((s) => [s.text, s.match, s.group])).toEqual([
      ['a', -1, 0], ['b', 0, 1], ['c', 0, 0], ['d', -1, 0],
    ]);
  });
});

describe('export as code', () => {
  it('quotes for each language', () => {
    expect(toJavaString('\\d+"x')).toBe('"\\\\d+\\"x"');
    expect(toJavaCompile('a', 'im')).toBe('Pattern.compile("a", Pattern.CASE_INSENSITIVE | Pattern.MULTILINE)');
    expect(toJsLiteral('a/b[/]', 'gi')).toBe('/a\\/b[/]/gi');
    expect(toRustRegex('a"b')).toBe('Regex::new(r#"a"b"#).unwrap()');
    expect(toRustRegex('a+')).toBe('Regex::new(r"a+").unwrap()');
  });
});
