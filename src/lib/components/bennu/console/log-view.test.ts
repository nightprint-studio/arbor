import { describe, expect, it } from 'vitest';
import type { LogPiece } from '$lib/types/log';
import {
  abbreviate, buildView, DEFAULT_LOG_VIEW, entryText, frameClass, hideMdc, isOwnClass, ruleFromLine, shortenThread,
  transformPieces, type LogViewRule, type LogViewSettings, type ViewLine, type ViewOptions,
} from './log-view';

const piece = (text: string, token?: LogPiece['token']): LogPiece => (token ? { text, token } : { text });

/** `2026-11-02 09:14:03.511 INFO [main] com.acme.Foo - hello` as the scanner would cut it. */
function logLine(message: string, over: Partial<ViewLine> = {}, logger = 'com.acme.dao.Foo', level = 'INFO'): ViewLine {
  const pieces = [
    piece('09:14:03.511', 'timestamp'), piece(' '), piece(level, 'level'), piece(' '),
    piece('[main]', 'thread'), piece(' '), piece(logger, 'logger'), piece(' - '), piece(message),
  ];
  return { text: pieces.map((p) => p.text).join(''), stream: 'out', level: level.toLowerCase() as ViewLine['level'], pieces, ...over };
}
const frame = (cls: string) => ({ text: `\tat ${cls}.run(X.java:1)`, stream: 'out' as const, level: 'error' as const });

const view = (lines: ViewLine[], settings: Partial<LogViewSettings> = {}, o: Partial<ViewOptions> = {}) =>
  buildView(lines, {
    settings: { ...DEFAULT_LOG_VIEW, clean: true, ...settings },
    own: ['com.acme'],
    query: '',
    openFolds: new Set(),
    openHidden: new Set(),
    ...o,
  });

describe('columns', () => {
  const pieces = logLine('hello').pieces!;
  const text = (s: Partial<LogViewSettings>) =>
    transformPieces(pieces, { ...DEFAULT_LOG_VIEW, ...s }).map((p) => p.text).join('');

  it('leaves a line alone with the default time, thread hidden', () => {
    expect(text({ thread: 'full', logger: 'full' })).toBe('09:14:03.511 INFO [main] com.acme.dao.Foo - hello');
  });
  it('drops the timestamp with the space after it', () => {
    expect(text({ time: 'hide', thread: 'full', logger: 'full' })).toBe('INFO [main] com.acme.dao.Foo - hello');
  });
  it('drops the thread and the logger without leaving their separators behind', () => {
    expect(text({ thread: 'hide', logger: 'hide' })).toBe('09:14:03.511 INFO hello');
  });
  it('abbreviates the logger and shortens the level to a letter', () => {
    expect(text({ logger: 'abbr', level: 'letter', thread: 'full' })).toBe('09:14:03.511 I [main] c.a.d.Foo - hello');
  });
  it('keeps only the class name', () => {
    expect(text({ logger: 'last', thread: 'hide' })).toBe('09:14:03.511 INFO Foo - hello');
  });
  it('shortens a Tomcat thread name', () => {
    expect(shortenThread('[http-nio-8080-exec-3]')).toBe('[exec-3]');
    expect(shortenThread('main')).toBe('main');
  });
  it('abbreviates a dotted name', () => {
    expect(abbreviate('org.hibernate.SQL')).toBe('o.h.SQL');
    expect(abbreviate('Plain')).toBe('Plain');
  });
});

describe('frames', () => {
  it('reads the class of a frame, with or without a module prefix', () => {
    expect(frameClass('\tat com.acme.Foo.bar(Foo.java:3)')).toBe('com.acme.Foo');
    expect(frameClass('\tat java.base/java.lang.Thread.run(Thread.java:833)')).toBe('java.lang.Thread');
    expect(frameClass('Caused by: x.Y: no')).toBeNull();
  });
  it('matches whole packages only', () => {
    expect(isOwnClass('com.acme.Foo', ['com.acme'])).toBe(true);
    expect(isOwnClass('com.acmeish.Foo', ['com.acme'])).toBe(false);
  });
});

describe('buildView', () => {
  it('folds framework frames into one row and keeps the user frames', () => {
    const lines = [
      { text: 'java.lang.IllegalStateException: no', stream: 'out' as const, level: 'error' as const },
      frame('org.spring.A'), frame('org.spring.B'), frame('com.acme.dao.Foo'), frame('java.lang.Thread'),
    ];
    const entries = view(lines);
    expect(entries.map((e) => e.kind)).toEqual(['line', 'fold', 'line', 'fold']);
    expect(entryText(entries[1])).toBe('▸ 2 framework frames');
  });
  it('opens a fold on request, keeping its frames after the marker', () => {
    const lines = [frame('org.spring.A'), frame('org.spring.B')];
    const entries = view(lines, {}, { openFolds: new Set([0]) });
    expect(entries.map((e) => e.kind)).toEqual(['fold', 'line', 'line']);
  });
  it('folds nothing when no package is known to be the user’s', () => {
    expect(view([frame('org.spring.A')], {}, { own: [] }).map((e) => e.kind)).toEqual(['line']);
  });
  it('collapses identical consecutive lines into a count', () => {
    const entries = view([logLine('pool'), logLine('pool'), logLine('pool'), logLine('other')]);
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({ kind: 'line', count: 3 });
  });
  it('does not collapse when the setting is off', () => {
    expect(view([logLine('a'), logLine('a')], { dedupe: false })).toHaveLength(2);
  });
  it('hides what a rule hides and says how many', () => {
    const rule: LogViewRule = { on: true, field: 'logger', op: 'starts', value: 'org.hibernate', action: 'hide', color: '' };
    const entries = view([logLine('a'), logLine('b', {}, 'org.hibernate.SQL'), logLine('c', {}, 'org.hibernate.type.X'), logLine('d')], { rules: [rule] });
    expect(entries.map((e) => e.kind)).toEqual(['line', 'hidden', 'line']);
    expect(entryText(entries[1])).toBe('▸ 2 hidden lines');
  });
  it('dims and highlights without hiding', () => {
    const rules: LogViewRule[] = [
      { on: true, field: 'message', op: 'contains', value: 'slow', action: 'dim', color: '' },
      { on: true, field: 'message', op: 'contains', value: 'user=', action: 'highlight', color: '#f0f' },
    ];
    const [a, b] = view([logLine('slow query'), logLine('GET user=x')], { rules });
    expect(a).toMatchObject({ dim: true });
    expect(b).toMatchObject({ color: '#f0f' });
  });
  it('ignores a disabled rule and a broken regex', () => {
    const off: LogViewRule = { on: false, field: 'message', op: 'contains', value: 'a', action: 'hide', color: '' };
    const bad: LogViewRule = { on: true, field: 'message', op: 'regex', value: '(', action: 'hide', color: '' };
    expect(view([logLine('a')], { rules: [off, bad] })).toHaveLength(1);
  });
  it('filters by level, but never the program’s own status lines', () => {
    const meta: ViewLine = { text: 'Process finished', stream: 'meta' };
    const entries = view([logLine('d', {}, 'x.Y', 'DEBUG'), meta, logLine('w', {}, 'x.Y', 'WARN')], { min_level: 'warn' });
    expect(entries.map((e) => e.kind)).toEqual(['hidden', 'line', 'line']);
  });
  it('filters by text', () => {
    const entries = view([logLine('alpha'), logLine('beta')], {}, { query: 'BET' });
    expect(entries.filter((e) => e.kind === 'line')).toHaveLength(1);
  });
  it('shows lines as written when the clean view is off, but still filters', () => {
    const l = logLine('hello');
    const entries = view([l, logLine('dbg', {}, 'x.Y', 'DEBUG')], { clean: false, min_level: 'info' });
    expect(entries[0]).toMatchObject({ kind: 'line', text: l.text });
    expect(entries[1]).toMatchObject({ kind: 'hidden' });
  });
});

describe('ruleFromLine', () => {
  const l = logLine('x', {}, 'org.hibernate.SQL');
  it('builds a logger rule, a package rule and a thread rule', () => {
    expect(ruleFromLine(l, 'logger', 'hide')).toMatchObject({ field: 'logger', op: 'starts', value: 'org.hibernate.SQL' });
    expect(ruleFromLine(l, 'package', 'dim')).toMatchObject({ value: 'org.hibernate.SQL', action: 'dim' });
    expect(ruleFromLine(l, 'thread', 'hide')).toMatchObject({ field: 'thread', op: 'equals', value: 'main' });
  });
  it('has nothing to say about a line without a logger', () => {
    expect(ruleFromLine({ text: 'plain', stream: 'out' }, 'logger', 'hide')).toBeNull();
  });
});

describe('hideMdc', () => {
  // `| MDC:[tenant=acme,trace_id=abc,user=bob] | hello`, as the scanner cuts it.
  const mdcLine = (): LogPiece[] => [
    piece('INFO', 'level'), piece(' | MDC:['), piece('tenant', 'mdckey'), piece('='), piece('acme', 'mdc'),
    piece(','), piece('trace_id', 'mdckey'), piece('='), piece('abc', 'mdc'),
    piece(','), piece('user', 'mdckey'), piece('='), piece('bob', 'mdc'), piece('] | hello'),
  ];
  const text = (ps: LogPiece[]) => ps.map((p) => p.text).join('');
  const settings = (over: Partial<LogViewSettings>): LogViewSettings => ({ ...DEFAULT_LOG_VIEW, ...over });

  it('removes one entry with its comma, wherever it sits', () => {
    expect(text(hideMdc(mdcLine(), settings({ hidden_mdc: ['trace_id'] })))).toBe('INFO | MDC:[tenant=acme,user=bob] | hello');
    expect(text(hideMdc(mdcLine(), settings({ hidden_mdc: ['tenant'] })))).toBe('INFO | MDC:[trace_id=abc,user=bob] | hello');
    expect(text(hideMdc(mdcLine(), settings({ hidden_mdc: ['user'] })))).toBe('INFO | MDC:[tenant=acme,trace_id=abc] | hello');
  });

  it('removes every entry and the empty wrapper the layout leaves', () => {
    expect(text(hideMdc(mdcLine(), settings({ mdc: 'hide' })))).toBe('INFO | hello');
  });

  it('leaves a line alone when nothing is hidden', () => {
    const line = mdcLine();
    expect(hideMdc(line, settings({}))).toBe(line);
  });
});
