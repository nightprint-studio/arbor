import { describe, expect, it } from 'vitest';
import { patternMarks, type PatternMark } from './log-pattern-marks';

/** Each mark as `kind:text`, so a test reads as what was coloured. */
const shown = (text: string): string[] => patternMarks(text).map((m: PatternMark) => `${m.kind}:${text.slice(m.from, m.to)}`);

describe('log pattern marks', () => {
  it('colours each conversion by what it prints, and its options quietly', () => {
    const xml = '<PatternLayout pattern="%d{HH:mm:ss.SSS} [%t] %-5level %logger{36} - %msg%n"/>';
    expect(shown(xml)).toEqual([
      'date:%d', 'opt:{HH:mm:ss.SSS}', 'thread:%t', 'level:%-5level', 'logger:%logger', 'opt:{36}', 'msg:%msg', 'dim:%n',
    ]);
  });

  it('reads a Logback <pattern> element, CDATA or not', () => {
    expect(shown('<encoder><pattern>%d %p %c{1}</pattern></encoder>')).toEqual(['date:%d', 'level:%p', 'logger:%c', 'opt:{1}']);
    expect(shown('<pattern><![CDATA[%level %msg]]></pattern>')).toEqual(['level:%level', 'msg:%msg']);
  });

  it('reads Log4j 1.x and the rolling file pattern', () => {
    expect(shown('<param name="ConversionPattern" value="%p [%c] %m%n"/>')).toEqual(['level:%p', 'logger:%c', 'msg:%m', 'dim:%n']);
    expect(shown('<RollingFile filePattern="logs/app-%d{yyyy-MM-dd}-%i.log.gz"/>')).toEqual(['date:%d', 'opt:{yyyy-MM-dd}', 'other:%i']);
  });

  it('marks a property substitution', () => {
    expect(shown('<Console><PatternLayout pattern="${LOG_PATTERN}"/></Console>')).toEqual(['prop:${LOG_PATTERN}']);
  });

  it('looks inside the wrappers that take a pattern', () => {
    // Log4j2's brace form: the first option is a pattern, the rest is styling.
    expect(shown('<P pattern="%highlight{%-5level}{FATAL=red}"/>')).toEqual(['other:%highlight', 'level:%-5level', 'opt:{FATAL=red}']);
    // Logback's parenthesised form.
    expect(shown('<pattern>%highlight(%-5level) %cyan(%logger{15})</pattern>')).toEqual([
      'other:%highlight', 'level:%-5level', 'other:%cyan', 'logger:%logger', 'opt:{15}',
    ]);
  });

  it('leaves a literal percent and text outside a pattern alone', () => {
    expect(shown('<P pattern="100%% %msg"/>')).toEqual(['msg:%msg']);
    expect(shown('<Property name="x">%d is not in a pattern</Property>')).toEqual([]);
  });

  it('never overlaps and stays in order, so a decoration set can be built from it', () => {
    const marks = patternMarks('<P pattern="%d{a}{UTC} %highlight{%X{k}}{x} ${p}"/><pattern>%m</pattern>');
    for (let i = 1; i < marks.length; i++) expect(marks[i].from).toBeGreaterThanOrEqual(marks[i - 1].to);
  });

  it('does not run away on an unclosed brace', () => {
    expect(() => patternMarks('<P pattern="%d{unclosed %msg"/>')).not.toThrow();
  });
});
