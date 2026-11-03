import { describe, expect, it } from 'vitest';
import { continues, readLogLine } from './log-file-marks';

const slice = (text: string, from: number, to: number) => text.slice(from, to);

describe('readLogLine', () => {
  it('reads a Tomcat line on a localised JVM', () => {
    const text = '03-Nov-2026 13:11:14.490 GRAVE [main] org.apache.catalina.core.StandardContext.listenerStart Exception';
    const { level, marks } = readLogLine(text);
    expect(level).toBe('error');
    const byKind = Object.fromEntries(marks.map((m) => [m.kind, slice(text, m.from, m.to)]));
    expect(byKind.ts).toBe('03-Nov-2026 13:11:14.490');
    expect(byKind.level).toBe('GRAVE');
    expect(byKind.thread).toBe('[main]');
    expect(byKind.logger).toBe('org.apache.catalina.core.StandardContext.listenerStart');
  });

  it('reads Maven and logback shapes', () => {
    expect(readLogLine('[ERROR] Failed to execute goal').level).toBe('error');
    expect(readLogLine('2026-08-05 12:33:01 WARN  c.a.Order - retrying').level).toBe('warn');
  });

  it('does not take a word in a sentence for a level', () => {
    expect(readLogLine('Error handling enabled').level).toBeNull();
    expect(readLogLine('the config is in a.b.INFO.txt').level).toBeNull();
  });

  it('knows a stack trace belongs to the line above', () => {
    expect(continues('\tat com.acme.Order.total(Order.java:118)')).toBe(true);
    expect(continues('Caused by: java.lang.NullPointerException')).toBe(true);
    expect(continues('... 23 more')).toBe(true);
    expect(continues('03-Nov-2026 13:11:14.490 INFO [main] a.B.c x')).toBe(false);
  });
});
