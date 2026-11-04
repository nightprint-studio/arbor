import { describe, expect, it } from 'vitest';
import { cycleQuote, tableSkeleton, toggleLinePrefixes, wrapInline } from './markdown-format';

describe('wrapInline', () => {
  it('wraps a selection', () => {
    expect(wrapInline('word', '**')).toMatchObject({ text: '**word**', from: 2, to: 6 });
  });
  it('leaves the caret between the markers on an empty selection', () => {
    expect(wrapInline('', '*')).toMatchObject({ text: '**', from: 1, to: 1 });
  });
  it('unwraps a selection that includes its markers', () => {
    expect(wrapInline('**word**', '**')).toMatchObject({ text: 'word', from: 0, to: 4 });
  });
  it('unwraps markers sitting just outside the selection', () => {
    expect(wrapInline('word', '**', '**', '**')).toMatchObject({
      text: 'word',
      eatBefore: 2,
      eatAfter: 2,
    });
  });
});

describe('toggleLinePrefixes', () => {
  it('adds the prefix to lines that lack it, skipping blank ones', () => {
    expect(toggleLinePrefixes(['a', '', 'b'], '- ')).toEqual(['- a', '', '- b']);
  });
  it('removes it when every line has it', () => {
    expect(toggleLinePrefixes(['- a', '- b'], '- ')).toEqual(['a', 'b']);
  });
  it('completes a partly prefixed block rather than doubling', () => {
    expect(toggleLinePrefixes(['- a', 'b'], '- ')).toEqual(['- a', '- b']);
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
