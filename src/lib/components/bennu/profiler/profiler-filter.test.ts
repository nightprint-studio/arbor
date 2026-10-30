import { describe, expect, it } from 'vitest';
import {
  DEFAULT_PREFS,
  DEFAULT_THRESHOLDS,
  classOfMember,
  elementClass,
  filterCount,
  completePackage,
  formatPatterns,
  matchesPattern,
  parsePatterns,
  keepClass,
  packageOf,
  severityOf,
  topLevelClass,
} from './profiler-filter';

const ctx = (over: Partial<typeof DEFAULT_PREFS.filter> = {}, classes: string[] | null = ['it.acme.Order', 'it.acme.util.Dates']) => ({
  filter: { ...DEFAULT_PREFS.filter, ...over },
  projectClasses: classes ? new Set(classes) : null,
});

describe('severityOf', () => {
  it('steps up at each threshold and not before', () => {
    const l = DEFAULT_THRESHOLDS.cpu_self_pct;
    expect([4.9, 5, 14.9, 15, 29.9, 30].map((v) => severityOf(v, l))).toEqual([0, 1, 1, 2, 2, 3]);
  });
});

describe('class names', () => {
  it('reduces nested, array and member spellings to the class they are about', () => {
    expect(topLevelClass('it.acme.Order$Line$1')).toBe('it.acme.Order');
    expect(elementClass('byte[][]')).toBe('byte');
    expect(elementClass('[Ljava.lang.String;')).toBe('java.lang.String');
    expect(classOfMember('java.util.Arrays.copyOf')).toBe('java.util.Arrays');
    expect(packageOf('it.acme.Order$Line')).toBe('it.acme');
    expect(packageOf('Main')).toBe('');
  });

  it('matches a bare package on a segment boundary, not as a prefix of the text', () => {
    expect(matchesPattern('it.acme.Order', 'it.acme')).toBe(true);
    expect(matchesPattern('it.acme.Order$Line', 'it.acme.Order')).toBe(true);
    expect(matchesPattern('it.acmeco.Order', 'it.acme')).toBe(false);
    expect(matchesPattern('it.acme.Order', '')).toBe(false);
  });

  it('matches a wildcard as a glob over the whole name', () => {
    expect(matchesPattern('it.acme.web.Order', 'it.acme.*')).toBe(true);
    expect(matchesPattern('it.acme.web.Order', '*.web.*')).toBe(true);
    expect(matchesPattern('it.acme.OrderDao', '*Dao')).toBe(true);
    expect(matchesPattern('it.acme.OrderDao', 'it.?cme.*')).toBe(true);
    expect(matchesPattern('it.acmeco.Order', 'it.acme.*')).toBe(false);
    // Regex characters in a pattern are literal.
    expect(matchesPattern('a.b.C', 'a+b.*')).toBe(false);
  });
});

describe('the package field', () => {
  it('reads include and exclude patterns, separated by commas or spaces, and writes them back', () => {
    const p = parsePatterns('it.acme.*, !*.generated.*  org.x');
    expect(p).toEqual({ include: ['it.acme.*', 'org.x'], exclude: ['*.generated.*'] });
    expect(parsePatterns(formatPatterns(p))).toEqual(p);
    expect(parsePatterns(' ! ,, ')).toEqual({ include: [], exclude: [] });
  });

  it('completes only what every candidate agrees on', () => {
    const pkgs = ['it.acme.web', 'it.acme.web.dto', 'it.acme.batch', 'org.x'];
    expect(completePackage('org', pkgs)).toBe('org.x');
    expect(completePackage('it.acme.w', pkgs)).toBe('it.acme.web');
    // Two branches under it.acme: it stops at the fork instead of choosing one.
    expect(completePackage('it.a', pkgs)).toBe('it.acme.');
    expect(completePackage('it.acme.', pkgs)).toBe('');
    expect(completePackage('!org', pkgs)).toBe('!org.x');
    expect(completePackage('it.*', pkgs)).toBe('');
    expect(completePackage('zzz', pkgs)).toBe('');
  });
});

describe('keepClass', () => {
  it('keeps everything when nothing is set, and rows with no class (threads, the root)', () => {
    expect(keepClass(ctx(), 'java.util.HashMap')).toBe(true);
    expect(keepClass(ctx({ project_only: true }), '')).toBe(true);
  });

  it('project-only keeps the index’s classes, nested ones included, and drops the rest', () => {
    const c = ctx({ project_only: true });
    expect(keepClass(c, 'it.acme.Order$Line')).toBe(true);
    expect(keepClass(c, 'java.util.HashMap')).toBe(false);
  });

  it('project-only does not hide anything while the project’s classes are unknown', () => {
    expect(keepClass(ctx({ project_only: true }, null), 'java.util.HashMap')).toBe(true);
  });

  it('include narrows and exclude wins over it', () => {
    const c = ctx({ include: ['it.acme'], exclude: ['it.acme.util'] }, null);
    expect(keepClass(c, 'it.acme.Order')).toBe(true);
    expect(keepClass(c, 'it.acme.util.Dates')).toBe(false);
    expect(keepClass(c, 'org.other.X')).toBe(false);
  });

  it('counts each restriction for the button', () => {
    expect(filterCount({ project_only: true, include: ['a'], exclude: ['b', 'c'] })).toBe(4);
  });
});
