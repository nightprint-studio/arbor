import { describe, it, expect } from 'vitest';
import { StringStream } from '@codemirror/language';

import { batch } from './batch-lang';

/**
 * Tokenise one line the way `StreamLanguage` does — a real {@link StringStream}, not a stand-in.
 *
 * The whole risk in a stream mode is a token that consumes nothing (the editor hangs) or one that
 * consumes the wrong span, and both are properties of the stream's own bookkeeping. A fake would
 * be testing the fake.
 */
function tokens(line: string): Array<[string, string | null]> {
  const state = batch.startState!(2);
  const stream = new StringStream(line, 2, 2, undefined);
  const out: Array<[string, string | null]> = [];
  let guard = 0;
  while (!stream.eol()) {
    const before = stream.pos;
    const style = batch.token(stream, state);
    expect(stream.pos, `stalled at ${before} in ${JSON.stringify(line)}`).toBeGreaterThan(before);
    out.push([line.slice(before, stream.pos), style]);
    stream.start = stream.pos;
    if (++guard > 200) throw new Error('runaway');
  }
  return out;
}

/** The style of the first token whose text is exactly `text`. */
function styleOf(line: string, text: string): string | null | undefined {
  return tokens(line).find(([t]) => t === text)?.[1];
}

describe('the batch mode', () => {
  it('reads both comment spellings, and only at the start of a command', () => {
    expect(tokens(':: build the war')).toEqual([[':: build the war', 'comment']]);
    expect(tokens('REM build the war')).toEqual([['REM build the war', 'comment']]);
    // Case-insensitive: half the files in an old tree shout.
    expect(tokens('rem quiet')).toEqual([['rem quiet', 'comment']]);
    // `::` in the middle of a line is a drive-letter-ish token, not a comment that swallows the
    // rest of the command.
    expect(styleOf('echo a::b', 'a::b')).toBe(null);
  });

  it('marks a label as the one definition batch has', () => {
    expect(tokens(':build')).toEqual([[':build', 'def']]);
    // `goto` names one, and the name is an argument rather than a label declaration.
    expect(styleOf('goto build', 'goto')).toBe('keyword');
  });

  it('colours the expansions, including the ones people forget exist', () => {
    expect(styleOf('echo %JAVA_HOME%', '%JAVA_HOME%')).toBe('variable-2');
    expect(styleOf('echo %1', '%1')).toBe('variable-2');
    expect(styleOf('echo %*', '%*')).toBe('variable-2');
    expect(styleOf('cd %~dp0', '%~dp0')).toBe('variable-2');
    expect(styleOf('echo !LATE!', '!LATE!')).toBe('variable-2');
  });

  it('ends a string at the line when it is unterminated, rather than eating the file', () => {
    expect(tokens('echo "hello world"')).toContainEqual(['"hello world"', 'string']);
    const unterminated = tokens('echo "hello');
    expect(unterminated[unterminated.length - 1]).toEqual(['"hello', 'string']);
  });

  it('knows that `&` and `|` begin a new command', () => {
    // `copy` is a builtin at the start of a command and a word anywhere else — so this is the
    // test that the operator reset the state.
    expect(styleOf('echo a & copy b c', 'copy')).toBe('builtin');
    expect(styleOf('echo copy', 'copy')).toBe(null);
  });

  it('reads `@` as a modifier and keeps the command after it a command', () => {
    const t = tokens('@echo off');
    expect(t[0]).toEqual(['@', 'operator']);
    expect(t[1]).toEqual(['echo', 'builtin']);
  });

  it('marks switches, which on a `for` are the whole meaning of the line', () => {
    expect(styleOf('for /f "tokens=1" %%a in (x) do echo %%a', '/f')).toBe('attribute');
    expect(styleOf('xcopy /s /e src dst', '/e')).toBe('attribute');
  });

  it('advances on every input it is given', () => {
    // The one failure mode that is not a wrong colour but a frozen editor.
    for (const line of ['', '  ', '%', '!', '"', '^', '()', '>>>', '/', '::', '@', '%%~']) {
      expect(() => tokens(line)).not.toThrow();
    }
  });
});
