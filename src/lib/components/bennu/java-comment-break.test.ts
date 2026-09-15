/**
 * Enter inside a Java comment, as the lines it writes. `|` marks the caret on a single line; the result
 * is that line split the way the editor splits it (trailing whitespace before the caret dropped).
 */

import { describe, expect, it } from 'vitest';
import { commentBreak, isBlockCommentClosed } from './java-comment-break';

function enter(line: string, opts: { openerAt?: number; block?: boolean; closed?: boolean } = {}): string | null {
  const caret = line.indexOf('|');
  const lineBefore = line.slice(0, caret);
  const lineAfter = line.slice(caret + 1);
  const brk = commentBreak({
    lineBefore,
    lineAfter,
    openerAt: opts.openerAt ?? -1,
    block: opts.block ?? true,
    closed: opts.closed ?? false,
  });
  if (!brk) return null;
  const moved = lineAfter.trimStart();
  const close = brk.close === null ? '' : `\n${brk.close}`;
  return `${lineBefore.trimEnd()}\n${brk.lead}|${moved}${close}`;
}

describe('Javadoc and block comments', () => {
  it('closes a Javadoc just opened, with the caret on a starred line', () => {
    expect(enter('    /**|', { openerAt: 4 })).toBe('    /**\n     * |\n     */');
    expect(enter('/*|', { openerAt: 0 })).toBe('/*\n * |\n */');
  });

  it('does not close a comment that is already closed', () => {
    expect(enter('    /**|', { openerAt: 4, closed: true })).toBe('    /**\n     * |');
  });

  it('continues a starred line aligned', () => {
    expect(enter('     * Returns the total.|')).toBe('     * Returns the total.\n     * |');
  });

  it('splits a line and carries the rest after the star', () => {
    expect(enter('     * Returns |the total.')).toBe('     * Returns\n     * |the total.');
  });

  it('aligns the closer when the caret is right before it', () => {
    expect(enter('    /** Total. |*/', { openerAt: 4, closed: true })).toBe('    /** Total.\n     |*/');
  });

  it('keeps the margin of a comment written without stars', () => {
    expect(enter('       plain text|')).toBe('       plain text\n       |');
  });
});

describe('line comments', () => {
  it('carries split text into a new line comment', () => {
    expect(enter('    // first |second', { block: false, openerAt: 4 })).toBe('    // first\n    // |second');
  });

  it('leaves a break at the end of a line comment to code', () => {
    expect(enter('    // done|', { block: false, openerAt: 4 })).toBeNull();
  });
});

describe('isBlockCommentClosed', () => {
  it('is closed by a `*/` before any other comment opens', () => {
    expect(isBlockCommentClosed(' text */\nclass A {}')).toBe(true);
    expect(isBlockCommentClosed('\nclass A {}\n/** other */')).toBe(false);
    expect(isBlockCommentClosed('\nclass A {}')).toBe(false);
  });
});
