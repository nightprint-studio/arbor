/**
 * Injected languages: unescaping with source mapping, detection (hints, context, concatenation,
 * the opt-in name heuristic), the write-back and the decorations.
 */

import { describe, expect, it } from 'vitest';
import { findSites, regexSiteAt, type DetectOptions, type InnerSite } from './detect';
import { highlightInner } from './highlight';
import { scanLiterals } from './literals';
import { literalUnits, sourceRanges, unitsText, escapeForLiteral } from './units';
import { rewriteSite } from './write-back';

const CONTEXT: DetectOptions = { regexDetection: 'context' };
const NAMES: DetectOptions = { regexDetection: 'names' };
const OFF: DetectOptions = { regexDetection: 'off' };

/** The value of the first literal of `src`. */
function valueOf(src: string, host: 'java' | 'js' | 'rust' = 'java'): string {
  const lit = scanLiterals(src, host)[0];
  return unitsText(literalUnits(src, lit, host));
}

const values = (sites: InnerSite[]) => sites.map((s) => [s.lang, unitsText(s.units)]);

describe('literals', () => {
  it('skips comments and char literals', () => {
    const src = `char q = '"'; // "no"\n/* "no" */ String s = "yes";`;
    expect(scanLiterals(src, 'java').map((l) => src.slice(l.start, l.end))).toEqual(['yes']);
  });

  it('reads Java text blocks and Rust raw strings', () => {
    const tb = 'String s = """\n  a\n  """;';
    expect(scanLiterals(tb, 'java')[0]).toMatchObject({ textBlock: true });
    const raw = 'let r = r#"a"b"#;';
    const l = scanLiterals(raw, 'rust')[0];
    expect(raw.slice(l.start, l.end)).toBe('a"b');
    expect(l.raw).toBe(true);
  });

  it('does not take a JS regex literal or a Rust lifetime for a string', () => {
    expect(scanLiterals(`const r = /["']/; const s = "x";`, 'js')).toHaveLength(1);
    expect(scanLiterals(`fn f<'a>(x: &'a str) -> &'a str { "y" }`, 'rust')).toHaveLength(1);
  });
});

describe('unescaping', () => {
  it('undoes Java escapes', () => {
    expect(valueOf('"a\\\\d+\\n\\t\\u0041\\101"')).toBe('a\\d+\n\tAA');
  });

  it('maps every unit back to the characters that wrote it', () => {
    const src = '"\\\\d+"';
    const lit = scanLiterals(src, 'java')[0];
    const units = literalUnits(src, lit, 'java');
    expect(unitsText(units)).toBe('\\d+');
    // `\d` of the value covers the three source characters `\\d`.
    expect(sourceRanges(units, 0, 2)).toEqual([{ from: 1, to: 4 }]);
  });

  it('applies the text-block rules: indentation stripped, trailing spaces dropped, \\s kept', () => {
    const src = 'x = """\n    ab  \n      cd\\s\n    """;';
    expect(valueOf(src)).toBe('ab\n  cd \n');
  });

  it('splits a mapped span where the source is not contiguous', () => {
    const src = 'x = """\n  ab\n  cd\n  """;';
    const lit = scanLiterals(src, 'java')[0];
    const units = literalUnits(src, lit, 'java');
    expect(unitsText(units)).toBe('ab\ncd\n');
    const ranges = sourceRanges(units, 0, 5);
    expect(ranges).toHaveLength(2); // `ab\n` and `cd` are separated by the stripped indent
  });

  it('undoes JS and Rust escapes and leaves raw strings alone', () => {
    expect(valueOf(`"a\\x41\\u{1F600}"`, 'js')).toBe('aA\u{1F600}');
    expect(valueOf(`"a\\u{41}\\n"`, 'rust')).toBe('aA\n');
    expect(valueOf(`r"a\\d"`, 'rust')).toBe('a\\d');
  });

  it('leaves an invalid escape as written', () => {
    expect(valueOf('"\\q"')).toBe('\\q');
  });
});

describe('escaping for a literal', () => {
  const plain = { textBlock: false, raw: false, template: false };

  it('round-trips a value through a Java string', () => {
    const value = 'a\\d+"\n\t';
    const escaped = escapeForLiteral(value, 'java', plain)!;
    expect(valueOf(`"${escaped}"`)).toBe(value);
  });

  it('refuses a raw string that would contain its own closing delimiter', () => {
    expect(escapeForLiteral('a"#b', 'rust', { ...plain, raw: true }, { hashes: 1 })).toBeNull();
    expect(escapeForLiteral('a"b', 'rust', { ...plain, raw: true }, { hashes: 1 })).toBe('a"b');
  });
});

describe('detection', () => {
  it('honours a language comment, an inline comment and @Language', () => {
    expect(values(findSites('// language=lua\nString s = "print(1)";', 'java', OFF))).toEqual([['lua', 'print(1)']]);
    expect(values(findSites('f(/* language=css */ "a{b:c}");', 'java', OFF))).toEqual([['css', 'a{b:c}']]);
    expect(values(findSites('@Language("html") String s = "<b>";', 'java', OFF))).toEqual([['html', '<b>']]);
    expect(values(findSites('// language=RegExp\nString s = "\\\\d+";', 'java', OFF))).toEqual([['regex', '\\d+']]);
  });

  it('applies a hint to the first literal after it only', () => {
    const src = '// language=json\nString a = "{}", b = "{}";';
    expect(findSites(src, 'java', OFF)).toHaveLength(1);
  });

  it('colours SQL by hint but leaves JPQL to the framework extension', () => {
    expect(values(findSites('// language=PostgreSQL\nString s = "select 1";', 'java', CONTEXT))).toEqual([['sql', 'select 1']]);
    expect(findSites('// language=JPAQL\nString s = "select o from O o";', 'java', CONTEXT)).toEqual([]);
  });

  it('recognises regex arguments by context', () => {
    const src = [
      'Pattern.compile("a+");',
      's.matches("b+");',
      's.replaceAll("c+", "$1");',
      's.split("\\\\s+");',
      '@Pattern(regexp = "d+") String f;',
    ].join('\n');
    expect(values(findSites(src, 'java', CONTEXT)).map((v) => v[1])).toEqual(['a+', 'b+', 'c+', '\\s+', 'd+']);
  });

  it('does not take a replacement, a matcher argument or a Splitter input for a pattern', () => {
    expect(findSites('s.replaceAll("x", "$1");', 'java', CONTEXT)).toHaveLength(1);
    expect(findSites('matcher.replaceAll("$1");', 'java', CONTEXT)).toEqual([]);
    expect(findSites('splitter.split("a.b");', 'java', CONTEXT)).toEqual([]);
    expect(findSites('s.split(",");', 'java', CONTEXT)).toEqual([]);
    expect(findSites('log.info("a+");', 'java', CONTEXT)).toEqual([]);
  });

  it('is silent when detection is off', () => {
    expect(findSites('Pattern.compile("a+");', 'java', OFF)).toEqual([]);
  });

  it('guesses from a variable name only at the names level, and not for a date format', () => {
    const src = 'String emailRegex = "a+@b";\nString pattern = "dd/MM/yyyy";';
    expect(findSites(src, 'java', CONTEXT)).toEqual([]);
    expect(values(findSites(src, 'java', NAMES))).toEqual([['regex', 'a+@b']]);
  });

  it('joins a concatenation into one site and marks a fragment as partial', () => {
    const [site] = findSites('Pattern.compile("^\\\\d+" + "\\\\w$");', 'java', CONTEXT);
    expect(unitsText(site.units)).toBe('^\\d+\\w$');
    expect(site.partial).toBe(false);
    const [open] = findSites('Pattern.compile("^a" + suffix);', 'java', CONTEXT);
    expect(open.partial).toBe(true);
  });

  it('reads the flags the call passes', () => {
    const [j] = findSites('Pattern.compile("a", Pattern.CASE_INSENSITIVE | Pattern.DOTALL);', 'java', CONTEXT);
    expect(j.flags).toBe('is');
    const [s] = findSites('new RegExp("a", "gi");', 'js', CONTEXT);
    expect(s.flags).toBe('gi');
  });

  it('recognises JS and Rust constructors', () => {
    expect(values(findSites('const r = new RegExp("a+");', 'js', CONTEXT))).toEqual([['regex', 'a+']]);
    expect(values(findSites('let r = Regex::new(r"a+").unwrap();', 'rust', CONTEXT))).toEqual([['regex', 'a+']]);
  });

  it('opens any literal under the caret as a regex when asked, recognised or not', () => {
    const src = 'log.info("a+");';
    const at = src.indexOf('a+');
    expect(regexSiteAt(src, 'java', at, at, CONTEXT)).toMatchObject({ origin: 'manual' });
    expect(regexSiteAt(src, 'java', 0, 0, CONTEXT)).toBeNull();
  });
});

describe('write-back', () => {
  /** `marked` has a `|` where the caret is; returns the source after writing `value` into the literal there. */
  function edit(marked: string, value: string, host: 'java' | 'js' | 'rust' = 'java'): string | null {
    const caret = marked.indexOf('|');
    const src = marked.replace('|', '');
    const site = regexSiteAt(src, host, caret, caret, CONTEXT)!;
    const e = rewriteSite(src, site, value);
    return e ? src.slice(0, e.from) + e.text + src.slice(e.to) : null;
  }

  it('re-escapes for a Java string', () => {
    expect(edit('Pattern.compile("|a");', '\\d+"')).toBe('Pattern.compile("\\\\d+\\"");');
  });

  it('collapses a concatenation into one literal', () => {
    expect(edit('Pattern.compile("|a" + "b");', 'xy')).toBe('Pattern.compile("xy");');
  });

  it('writes a Rust raw string verbatim and refuses what it cannot hold', () => {
    expect(edit('Regex::new(r#"|a"#);', '\\d"', 'rust')).toBe('Regex::new(r#"\\d""#);');
    expect(edit('Regex::new(r#"|a"#);', 'a"#', 'rust')).toBeNull();
  });

  it('keeps a text block indented and closing where it was', () => {
    const out = edit('p = Pattern.compile("""\n    |a\n    """);', 'x\n\\d\n');
    expect(out).toBe('p = Pattern.compile("""\n    x\n    \\\\d\n    """);');
  });
});

describe('highlighting an injected language', () => {
  it('colours Lua through the stream mode', () => {
    const spans = highlightInner('lua', 'local x = 1', { flavour: 'java', flags: '' });
    expect(spans.some((s) => s.cls === 'keyword' && s.from === 0)).toBe(true);
    expect(spans.some((s) => s.cls === 'number')).toBe(true);
  });

  it('routes regex to the pattern highlighter', () => {
    const spans = highlightInner('regex', '\\d+', { flavour: 'java', flags: '' });
    expect(spans.map((s) => s.cls)).toEqual(['type', 'operator']);
  });
});
