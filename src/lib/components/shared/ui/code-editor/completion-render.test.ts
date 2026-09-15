/**
 * The row's reading: which rows name an origin, and which classes the theme styles them by.
 */

import { describe, expect, it } from 'vitest';
import { originText, rowClass, type RichOption } from './completion-render';

const option = (over: Partial<RichOption> = {}): RichOption => ({ label: 'prefix', ...over });

describe('originText', () => {
  /// A member of the receiver itself: repeating the receiver's name on most rows says nothing.
  it('is hidden for a member the receiver declares', () => {
    expect(originText(option({ origin: 'ServiceRoute', standing: 'own' }))).toBeUndefined();
  });

  it('names where an inherited or implicit member comes from', () => {
    expect(originText(option({ origin: 'Animal', standing: 'inherited' }))).toBe('Animal');
    expect(originText(option({ origin: 'Object', standing: 'implicit' }))).toBe('Object');
  });

  /// A provider that says nothing about standing keeps the column as it always was.
  it('is shown whenever a provider without standing names an origin', () => {
    expect(originText(option({ origin: 'List' }))).toBe('List');
    expect(originText(option())).toBeUndefined();
  });
});

describe('rowClass', () => {
  it('carries the standing and the deprecated mark', () => {
    expect(rowClass(option({ standing: 'own' }))).toBe('cm-completion-own');
    expect(rowClass(option({ standing: 'implicit', isDeprecated: true }))).toBe('cm-completion-implicit cm-completion-deprecated');
  });

  it('is empty for a plain row', () => {
    expect(rowClass(option())).toBe('');
  });
});
