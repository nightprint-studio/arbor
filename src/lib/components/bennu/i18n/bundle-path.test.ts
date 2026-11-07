import { describe, it, expect } from 'vitest';
import { bundleOf, isI18nBundle } from './bundle-path';

describe('bundleOf', () => {
  it('reads a category file', () => {
    expect(bundleOf('/p/i18n/it/ui.toml')).toEqual({
      root: '/p/i18n',
      lang: 'it',
      category: 'ui',
      document: false,
    });
  });

  it('reads a real Voriz layout, where the tree is nested under presentazione/', () => {
    expect(bundleOf('/v/progetti/voriz/presentazione/i18n/en/ui.toml')?.lang).toBe('en');
  });

  it('does not take the declaring files for translations', () => {
    for (const f of ['languages', 'styles', 'glossary']) {
      expect(isI18nBundle(`/p/i18n/${f}.toml`)).toBe(false);
    }
  });

  it('reads a wiki document as a bundle of its own kind', () => {
    expect(bundleOf('/v/presentazione/i18n/it/wiki/creature/ragno.md')).toEqual({
      root: '/v/presentazione/i18n',
      lang: 'it',
      category: 'wiki',
      document: true,
    });
  });

  it('skips what the engine skips', () => {
    expect(isI18nBundle('/p/i18n/it/wiki/.nota.md')).toBe(false);
    expect(isI18nBundle('/p/i18n/it/.bozze/x.md')).toBe(false);
    // A `.md` straight in a language folder is not loaded; nor is a `.toml` two levels down.
    expect(isI18nBundle('/p/i18n/it/zaino.md')).toBe(false);
    expect(isI18nBundle('/p/i18n/it/wiki/x.toml')).toBe(false);
    expect(isI18nBundle('/p/README.md')).toBe(false);
  });
});
