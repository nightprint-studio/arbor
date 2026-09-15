/**
 * The completion icon set: which kind draws as what, and which marks go on top.
 *
 * What is pinned is the reading, not the pixels: that a member and a type never share a shape,
 * that a type nobody declared still gets an icon, and that the modifiers a reader scans for —
 * static, final, abstract, deprecated — each leave a mark.
 */

import { describe, expect, it } from 'vitest';
import {
  COMPLETION_ICON_KINDS, completionIconMarkup, describeCompletionIcon,
} from './completion-icons';

describe('describeCompletionIcon', () => {
  /// CodeMirror's own vocabulary: a provider that sends only these must never fall back.
  it('draws every standard CodeMirror type', () => {
    for (const type of ['method', 'function', 'class', 'interface', 'variable', 'constant', 'type', 'enum', 'property', 'keyword', 'namespace', 'text']) {
      expect(describeCompletionIcon(type).kind.type).toBe(type);
    }
  });

  it('draws members as circles and types as squares', () => {
    for (const member of ['method', 'constructor', 'property', 'variable', 'parameter']) {
      expect(describeCompletionIcon(member).kind.shape).toBe('circle');
    }
    for (const type of ['class', 'interface', 'enum', 'record', 'annotation']) {
      expect(describeCompletionIcon(type).kind.shape).toBe('square');
    }
  });

  it('tells a class from an interface by colour', () => {
    expect(describeCompletionIcon('class').kind.color).not.toBe(describeCompletionIcon('interface').kind.color);
  });

  it('falls back to a plain icon for a type nobody declared', () => {
    expect(describeCompletionIcon('something-new').kind.type).toBe('text');
    expect(describeCompletionIcon(undefined).kind.type).toBe('text');
  });

  it('marks static and final, in a fixed order', () => {
    expect(describeCompletionIcon('property', ['final', 'static']).marks).toEqual(['static', 'final']);
    expect(describeCompletionIcon('method').marks).toEqual([]);
  });

  /// A constant is a static final field, whether or not the provider spells the modifiers out.
  it('gives a constant the marks of a static final field', () => {
    const constant = describeCompletionIcon('constant');
    expect(constant.marks).toEqual(['static', 'final']);
    expect(constant.kind.glyph).toBe(describeCompletionIcon('property').kind.glyph);
  });

  it('reads abstract and deprecated', () => {
    const icon = describeCompletionIcon('method', ['abstract'], true);
    expect(icon.abstract).toBe(true);
    expect(icon.deprecated).toBe(true);
  });

  it('declares each type once', () => {
    const types = COMPLETION_ICON_KINDS.map((k) => k.type);
    expect(new Set(types).size).toBe(types.length);
  });
});

describe('completionIconMarkup', () => {
  it('is one SVG with the glyph in it', () => {
    const svg = completionIconMarkup(describeCompletionIcon('method'));
    expect(svg.startsWith('<svg')).toBe(true);
    expect(svg.match(/<svg/g)).toHaveLength(1);
    expect(svg).toContain('>m</text>');
  });

  /// Colour is the theme's job: markup that carried a colour would ignore light and dark.
  it('carries no colour of its own', () => {
    expect(completionIconMarkup(describeCompletionIcon('class', ['static'], true))).not.toMatch(/fill="#|stroke="#|var\(/);
  });

  it('draws one element per mark', () => {
    const plain = completionIconMarkup(describeCompletionIcon('property'));
    const constant = completionIconMarkup(describeCompletionIcon('property', ['static', 'final']));
    expect(plain).not.toContain('cm-ki-mark');
    expect(constant.match(/cm-ki-mark/g)).toHaveLength(2);
  });

  it('dashes an abstract body and strikes a deprecated one', () => {
    expect(completionIconMarkup(describeCompletionIcon('method', ['abstract']))).toContain('cm-ki-abstract');
    expect(completionIconMarkup(describeCompletionIcon('method', [], true))).toContain('cm-ki-strike');
    expect(completionIconMarkup(describeCompletionIcon('method'))).not.toContain('cm-ki-strike');
  });

  it('draws a package as a folder with no letter', () => {
    const svg = completionIconMarkup(describeCompletionIcon('namespace'));
    expect(svg).toContain('<path');
    expect(svg).not.toContain('<text');
  });
});
