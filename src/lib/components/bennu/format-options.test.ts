import { describe, expect, it } from 'vitest';
import { FORMAT_GROUPS, FORMAT_OPTIONS, optionFor, optionsOf } from './format-options';
import {
  DEFAULT_FORMAT_STYLE, diffFromDefaults, effectiveStyle, mergeLayers, originOf, sameValue,
  styleBelowProject, type StyleLayers,
} from './format-layers';

const KNOWN_ENUM_VALUES = new Set([
  'preserve', 'same_line', 'next_line',            // BraceStyle
  'never', 'fill', 'chop', 'always',               // WrapPolicy
  'own_line',                                      // AnnotationPlacement (+ preserve, same_line)
  'first', 'last', 'mixed',                        // StaticImports
  'auto', 'lf', 'crlf',                            // LineEnding
]);

describe('the option catalogue', () => {
  it('has unique keys', () => {
    const keys = FORMAT_OPTIONS.map((o) => o.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('covers exactly the keys of the style, no more and no fewer', () => {
    const catalogued = FORMAT_OPTIONS.map((o) => o.key).sort();
    expect(catalogued).toEqual(Object.keys(DEFAULT_FORMAT_STYLE).sort());
  });

  it('only uses declared groups, and every group has options', () => {
    const groups = new Set(FORMAT_GROUPS.map((g) => g.id));
    expect(groups.size).toBe(FORMAT_GROUPS.length);
    for (const o of FORMAT_OPTIONS) expect(groups.has(o.group)).toBe(true);
    for (const g of FORMAT_GROUPS) expect(optionsOf(g.id).length).toBeGreaterThan(0);
  });

  it('offers only known enum strings, and always the default one', () => {
    for (const o of FORMAT_OPTIONS) {
      if (o.kind !== 'enum') continue;
      const values = o.choices.map((c) => c.value);
      expect(new Set(values).size).toBe(values.length);
      for (const v of values) expect(KNOWN_ENUM_VALUES.has(v)).toBe(true);
      expect(values).toContain(DEFAULT_FORMAT_STYLE[o.key]);
    }
  });

  it('has defaults of the right kind and inside the declared range', () => {
    for (const o of FORMAT_OPTIONS) {
      const v = DEFAULT_FORMAT_STYLE[o.key];
      if (o.kind === 'bool') expect(typeof v).toBe('boolean');
      if (o.kind === 'stringlist') expect(Array.isArray(v)).toBe(true);
      if (o.kind === 'int') {
        expect(typeof v).toBe('number');
        expect(v as number).toBeGreaterThanOrEqual(o.min);
        expect(v as number).toBeLessThanOrEqual(o.max);
      }
    }
  });

  it('points dependsOn at boolean options', () => {
    for (const o of FORMAT_OPTIONS) {
      if (!o.dependsOn) continue;
      expect(optionFor(o.dependsOn)?.kind).toBe('bool');
    }
  });
});

describe('diffFromDefaults', () => {
  it('is empty for the defaults', () => {
    expect(diffFromDefaults(DEFAULT_FORMAT_STYLE)).toEqual({});
  });

  it('names only the keys that differ, lists included', () => {
    const style = { ...DEFAULT_FORMAT_STYLE, indent_width: 2, import_groups: ['*'] };
    expect(diffFromDefaults(style)).toEqual({ indent_width: 2, import_groups: ['*'] });
  });

  it('round-trips through mergeLayers', () => {
    const style = { ...DEFAULT_FORMAT_STYLE, use_tabs: true, wrap_chain: 'chop' as const };
    expect(mergeLayers(DEFAULT_FORMAT_STYLE, diffFromDefaults(style))).toEqual(style);
  });
});

describe('mergeLayers', () => {
  it('lets later layers win and ignores empty ones', () => {
    const merged = mergeLayers(
      DEFAULT_FORMAT_STYLE, { indent_width: 2, line_width: 100 }, null, { indent_width: 8 }, undefined,
    );
    expect(merged.indent_width).toBe(8);
    expect(merged.line_width).toBe(100);
    expect(merged.use_tabs).toBe(false);
  });

  it('does not mutate the base', () => {
    mergeLayers(DEFAULT_FORMAT_STYLE, { indent_width: 2 });
    expect(DEFAULT_FORMAT_STYLE.indent_width).toBe(4);
  });
});

describe('layer precedence', () => {
  const layers = (over: Partial<StyleLayers> = {}): StyleLayers => ({
    defaults: DEFAULT_FORMAT_STYLE,
    global: { ...DEFAULT_FORMAT_STYLE, indent_width: 3 },
    detected: { indent_width: 5, line_width: 99 },
    editorconfig: { indent_width: 6, use_tabs: true },
    project: null,
    ...over,
  });

  it('puts editorconfig over global, and ignores detection unless auto_detect is on', () => {
    const s = effectiveStyle(layers());
    expect(s.indent_width).toBe(6);
    expect(s.line_width).toBe(120);
  });

  it('applies detection under editorconfig when auto_detect is on', () => {
    const s = effectiveStyle(layers({ global: { ...DEFAULT_FORMAT_STYLE, auto_detect: true } }));
    expect(s.line_width).toBe(99);
    expect(s.indent_width).toBe(6);
  });

  it('lets the project win over everything', () => {
    const s = effectiveStyle(layers({ project: { indent_width: 2 } }));
    expect(s.indent_width).toBe(2);
    expect(styleBelowProject(layers({ project: { indent_width: 2 } })).indent_width).toBe(6);
  });

  it('skips editorconfig when use_editorconfig is off', () => {
    const s = effectiveStyle(layers({ global: { ...DEFAULT_FORMAT_STYLE, use_editorconfig: false } }));
    expect(s.indent_width).toBe(4);
  });

  it('names the layer a value comes from', () => {
    const l = layers({ project: { line_width: 80 } });
    expect(originOf('line_width', l)).toBe('project');
    expect(originOf('indent_width', l)).toBe('editorconfig');
    expect(originOf('max_blank_lines', l)).toBe('default');
    const detecting = layers({ global: { ...DEFAULT_FORMAT_STYLE, auto_detect: true } });
    expect(originOf('line_width', detecting)).toBe('detected');
    expect(originOf('auto_detect', detecting)).toBe('global');
  });
});

describe('sameValue', () => {
  it('compares lists by element', () => {
    expect(sameValue(['a', 'b'], ['a', 'b'])).toBe(true);
    expect(sameValue(['a', 'b'], ['b', 'a'])).toBe(false);
    expect(sameValue(1, 1)).toBe(true);
  });
});
