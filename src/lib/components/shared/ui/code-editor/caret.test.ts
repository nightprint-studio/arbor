import { describe, expect, it } from 'vitest';
import { caretRules, resolveCaretColor } from './caret';

describe('resolveCaretColor', () => {
  it('defaults to the theme token, falling back to the foreground', () => {
    expect(resolveCaretColor()).toBe('var(--editor-caret, var(--text-primary))');
    expect(resolveCaretColor('auto')).toBe('var(--editor-caret, var(--text-primary))');
  });

  it('knows the accent and a #rrggbb colour', () => {
    expect(resolveCaretColor('accent')).toBe('var(--accent)');
    expect(resolveCaretColor('#ff8800')).toBe('#ff8800');
  });

  it('never turns a bad value into an invisible caret', () => {
    expect(resolveCaretColor('red')).toBe('var(--editor-caret, var(--text-primary))');
    expect(resolveCaretColor('#fff')).toBe('var(--editor-caret, var(--text-primary))');
    expect(resolveCaretColor('url(x)')).toBe('var(--editor-caret, var(--text-primary))');
  });
});

describe('caretRules', () => {
  const cursor = (p = {}) => caretRules(p)['.cm-cursor, .cm-dropCursor'];

  it('is a two pixel line with a halo by default', () => {
    expect(cursor().borderLeft).toBe('2px solid var(--editor-caret, var(--text-primary))');
    expect(cursor().marginLeft).toBe('-1px');
    expect(cursor().boxShadow).toContain('var(--bg-base)');
  });

  it('follows the width', () => {
    expect(cursor({ width: 'thin' }).borderLeft).toContain('1px');
    expect(cursor({ width: 'thick' }).borderLeft).toContain('3px');
  });

  it('draws a block one character wide, and an underline that keeps the thickness', () => {
    expect(cursor({ style: 'block' }).borderLeft).toContain('1ch');
    const underline = cursor({ style: 'underline', width: 'thick' });
    expect(underline.borderBottom).toBe('3px solid var(--editor-caret, var(--text-primary))');
    expect(underline.borderLeft).toBe('none');
    expect(underline.width).toBe('1ch');
  });

  it('only changes the blink when asked to', () => {
    const layer = '&.cm-focused > .cm-scroller > .cm-cursorLayer';
    expect(caretRules()[layer]).toEqual({});
    expect(caretRules({ blink: 'smooth' })[layer].animationTimingFunction).toBe('ease-in-out');
    expect(caretRules({ blink: 'solid' })[layer].animationIterationCount).toBe('0');
  });
});
