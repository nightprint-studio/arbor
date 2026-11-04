import { EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { describe, expect, it } from 'vitest';
import {
  MARKDOWN_ACTIONS,
  blockFormatOf,
  cycleQuote,
  inlineSpans,
  linkAt,
  markdownFormatState,
  sameFormat,
  NO_FORMAT,
  setHeadingLines,
  spanAt,
  tableSkeleton,
  toggleListKind,
  wrapInline,
} from './markdown-format';

describe('inlineSpans', () => {
  it('finds bold, italic, strike, highlight and code on a line', () => {
    const kinds = inlineSpans('**a** *b* ~~c~~ ==d== `e`').map((s) => s.kind).sort();
    expect(kinds).toEqual(['bold', 'code', 'highlight', 'italic', 'strike']);
  });

  it('reports marker-inclusive offsets and the marker length', () => {
    expect(inlineSpans('x **bold** y')).toEqual([{ kind: 'bold', from: 2, to: 10, markLen: 2 }]);
  });

  it('reads *** as italic around bold', () => {
    const spans = inlineSpans('***x***');
    expect(spans.find((s) => s.kind === 'bold')).toMatchObject({ from: 1, to: 6 });
    expect(spans.find((s) => s.kind === 'italic')).toMatchObject({ from: 0, to: 7 });
  });

  it('does not open emphasis on a bullet or on a spaced multiplication', () => {
    expect(inlineSpans('* item')).toEqual([]);
    expect(inlineSpans('2 * 3 * 4')).toEqual([]);
  });

  it('skips code spans and escapes', () => {
    expect(inlineSpans('`a*b*c`').map((s) => s.kind)).toEqual(['code']);
    expect(inlineSpans('\\*not\\*')).toEqual([]);
  });

  it('ignores an unterminated marker', () => {
    expect(inlineSpans('**never closed')).toEqual([]);
  });
});

describe('spanAt', () => {
  const line = 'a **bold** b';
  it('is active strictly inside, not at the edges, for a caret', () => {
    expect(spanAt(line, 'bold', 6, 6)).toBeDefined();
    expect(spanAt(line, 'bold', 2, 2)).toBeUndefined();
    expect(spanAt(line, 'bold', 10, 10)).toBeUndefined();
  });
  it('accepts a selection that covers the whole span, markers included', () => {
    expect(spanAt(line, 'bold', 2, 10)).toBeDefined();
  });
  it('is not active for another kind', () => {
    expect(spanAt(line, 'italic', 6, 6)).toBeUndefined();
  });
});

describe('linkAt', () => {
  it('finds a link around the caret and ignores images', () => {
    expect(linkAt('see [here](http://x) now', 8, 8)).toMatchObject({ label: 'here' });
    expect(linkAt('![alt](a.png)', 4, 4)).toBeUndefined();
  });
});

describe('wrapInline', () => {
  it('wraps a selection', () => {
    expect(wrapInline('word', '**')).toEqual({ text: '**word**', from: 2, to: 6 });
  });
  it('leaves the caret between the markers on an empty selection', () => {
    expect(wrapInline('', '*')).toEqual({ text: '**', from: 1, to: 1 });
  });
  it('unwraps a selection that includes its markers', () => {
    expect(wrapInline('**word**', '**')).toEqual({ text: 'word', from: 0, to: 4 });
  });
});

describe('blockFormatOf', () => {
  it('reads a heading level, a list kind and a quote', () => {
    expect(blockFormatOf(['## Title']).heading).toBe(2);
    expect(blockFormatOf(['- a', '- b']).list).toBe('bullet');
    expect(blockFormatOf(['1. a', '2. b']).list).toBe('numbered');
    expect(blockFormatOf(['- [ ] a', '- [x] b']).list).toBe('task');
    expect(blockFormatOf(['> a', '> b']).quote).toBe(true);
    expect(blockFormatOf(['> [!tip] hi', '> body']).callout).toBe(true);
  });
  it('is "mixed" (null) when the lines disagree', () => {
    expect(blockFormatOf(['- a', 'plain']).list).toBeNull();
    expect(blockFormatOf(['> a', 'plain']).quote).toBe(false);
  });
  it('is empty for blank lines', () => {
    expect(blockFormatOf(['', '  '])).toEqual({ heading: 0, list: null, quote: false, callout: false });
  });
});

describe('toggleListKind', () => {
  it('adds the kind to lines that lack it, skipping blank ones', () => {
    expect(toggleListKind(['a', '', 'b'], 'bullet')).toEqual(['- a', '', '- b']);
  });
  it('removes it when every line has it', () => {
    expect(toggleListKind(['- a', '- b'], 'bullet')).toEqual(['a', 'b']);
  });
  it('converts between kinds instead of stacking markers', () => {
    expect(toggleListKind(['- a'], 'task')).toEqual(['- [ ] a']);
    expect(toggleListKind(['- [x] a'], 'bullet')).toEqual(['- a']);
    expect(toggleListKind(['1. a'], 'bullet')).toEqual(['- a']);
  });
  it('numbers a list 1…n and removes it again', () => {
    const numbered = toggleListKind(['a', 'b', 'c'], 'numbered');
    expect(numbered).toEqual(['1. a', '2. b', '3. c']);
    expect(toggleListKind(numbered, 'numbered')).toEqual(['a', 'b', 'c']);
  });
  it('keeps indentation and leaves quoted lines alone', () => {
    expect(toggleListKind(['  a'], 'bullet')).toEqual(['  - a']);
    expect(toggleListKind(['> a'], 'bullet')).toEqual(['> a']);
  });
});

describe('setHeadingLines', () => {
  it('sets, replaces and clears a level', () => {
    expect(setHeadingLines(['title'], 2)).toEqual(['## title']);
    expect(setHeadingLines(['# title'], 3)).toEqual(['### title']);
    expect(setHeadingLines(['## title'], 0)).toEqual(['title']);
  });
  it('toggles off the same level only when asked to', () => {
    expect(setHeadingLines(['## t'], 2, true)).toEqual(['t']);
    expect(setHeadingLines(['## t'], 2, false)).toEqual(['## t']);
  });
});

describe('cycleQuote', () => {
  it('plain → quote → callout → plain', () => {
    const quote = cycleQuote(['one', 'two']);
    expect(quote).toEqual(['> one', '> two']);
    const callout = cycleQuote(quote);
    expect(callout).toEqual(['> [!note]', '> one', '> two']);
    expect(cycleQuote(callout)).toEqual(['one', 'two']);
  });
});

describe('tableSkeleton', () => {
  it('has a header, a rule and the asked-for body rows', () => {
    expect(tableSkeleton(2, 1).split('\n')).toEqual([
      '| Column | Column |',
      '| --- | --- |',
      '|   |   |',
    ]);
  });
});

describe('markdownFormatState', () => {
  const stateOf = (doc: string, anchor: number, head = anchor) =>
    EditorState.create({
      doc,
      selection: { anchor, head },
      extensions: [markdown({ base: markdownLanguage })],
    });

  it('follows the caret in and out of bold', () => {
    const doc = 'a **bold** b';
    expect(markdownFormatState(stateOf(doc, 6)).bold).toBe(true);
    expect(markdownFormatState(stateOf(doc, 1)).bold).toBe(false);
  });

  it('reads the block of the current line', () => {
    expect(markdownFormatState(stateOf('## T', 3)).heading).toBe(2);
    expect(markdownFormatState(stateOf('- [ ] x', 2)).list).toBe('task');
    expect(markdownFormatState(stateOf('> q', 2)).quote).toBe(true);
  });

  it('knows it is inside a fenced block, and turns inline formats off there', () => {
    const doc = '```\n**x**\n```';
    const s = markdownFormatState(stateOf(doc, 7));
    expect(s.codeBlock).toBe(true);
    expect(s.bold).toBe(false);
  });

  it('is not a code block outside the fence', () => {
    expect(markdownFormatState(stateOf('text\n\n```\nx\n```', 2)).codeBlock).toBe(false);
  });

  it('compares states structurally', () => {
    expect(sameFormat(NO_FORMAT, { ...NO_FORMAT })).toBe(true);
    expect(sameFormat(NO_FORMAT, { ...NO_FORMAT, bold: true })).toBe(false);
  });
});

describe('MARKDOWN_ACTIONS', () => {
  it('has unique ids and unique chords', () => {
    const ids = MARKDOWN_ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
    const keys = MARKDOWN_ACTIONS.flatMap((a) => (a.key ? [a.key] : []));
    expect(new Set(keys).size).toBe(keys.length);
  });
  it('every toggle reports itself active under the state that means it', () => {
    const on = (id: string, patch: object) =>
      MARKDOWN_ACTIONS.find((a) => a.id === id)?.active?.({ ...NO_FORMAT, ...patch });
    expect(on('bold', { bold: true })).toBe(true);
    expect(on('bold', {})).toBe(false);
    expect(on('task', { list: 'task' })).toBe(true);
    expect(on('bullet', { list: 'task' })).toBe(false);
    expect(on('h2', { heading: 2 })).toBe(true);
    expect(on('codeblock', { codeBlock: true })).toBe(true);
  });
});
