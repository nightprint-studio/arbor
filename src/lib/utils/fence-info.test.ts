import { describe, expect, it, vi } from 'vitest';
import Prism from 'prismjs';
import { LanguageDescription } from '@codemirror/language';

import { fenceGrammarName, fenceLanguage } from './fence-info';
import './prism-languages/patina';

// The fence list reaches for the code-editor barrel (a Svelte component lives behind it) and for the LSP-backed
// completion hooks, none of which the vocabulary under test needs. Stand-ins keep the real list loadable in Node.
vi.mock('$lib/components/shared/ui/code-editor', async () => {
  const { StreamLanguage } = await import('@codemirror/language');
  const flat = { token: (s: { skipToEnd(): void }) => (s.skipToEnd(), null) };
  return { sqlLanguage: () => StreamLanguage.define(flat), javascriptStream: flat };
});
vi.mock('$lib/components/bennu/lsp-lang', () => ({ backendCompletionSource: () => null, backendHoverSource: () => null }));
// The Patina descriptor renders its comments through the markdown editor (DOM widgets, Tauri); the fences need none of it.
vi.mock('$lib/utils/comment-markdown', () => ({ commentMarkdown: () => [] }));

const { MARKDOWN_FENCE_LANGUAGES } = await import('$lib/components/bennu/markdown-fences');

describe('fenceLanguage', () => {
  it('lowercases the tag and keeps only the first word', () => {
    expect(fenceLanguage('ptn')).toBe('ptn');
    expect(fenceLanguage('Patina')).toBe('patina');
    expect(fenceLanguage('  PTN  ')).toBe('ptn');
    expect(fenceLanguage('ptn title="x"')).toBe('ptn');
    expect(fenceLanguage('ptn{1,3}')).toBe('ptn');
    expect(fenceLanguage('ptn,linenos')).toBe('ptn');
  });

  it('has no language for an empty info string', () => {
    expect(fenceLanguage('')).toBeNull();
    expect(fenceLanguage('   ')).toBeNull();
    expect(fenceLanguage(null)).toBeNull();
    expect(fenceLanguage(undefined)).toBeNull();
  });
});

describe('fenceGrammarName', () => {
  it('maps the ptn alias to patina and leaves the others alone', () => {
    expect(fenceGrammarName('ptn')).toBe('patina');
    expect(fenceGrammarName('PTN title="x"')).toBe('patina');
    expect(fenceGrammarName('Patina')).toBe('patina');
    expect(fenceGrammarName('rust')).toBe('rust');
    expect(fenceGrammarName('')).toBeNull();
  });

  it('names a grammar that Prism really has', () => {
    for (const info of ['ptn', 'patina', 'Patina', 'ptn title="x"']) {
      expect(Prism.languages[fenceGrammarName(info)!]).toBeDefined();
    }
  });
});

describe('the Prism grammar for Patina', () => {
  const types = (code: string) =>
    Prism.tokenize(code, Prism.languages.patina)
      .filter((t): t is Prism.Token => typeof t !== 'string')
      .map((t) => [t.type, String(t.content)] as const);

  it('colours the shapes the stream mode colours', () => {
    const out = types('fn spawn(n: int) { let x = Mood::Calm; BlockPos::new(1, 2); block!("tnt") } // done');
    expect(out).toContainEqual(['keyword', 'fn']);
    expect(out).toContainEqual(['function', 'spawn']);
    // A primitive has its own class (7 Nov 2026), apart from the ordinary types around it.
    expect(out).toContainEqual(['primitive-type', 'int']);
    // An ordinary type keeps the class-name look (its token is `class-name-type`, aliased).
    expect(types('fn f(p: Player, n: int)')).toContainEqual(['class-name-type', 'Player']);
    expect(out).toContainEqual(['constant', 'Calm']);
    expect(out).toContainEqual(['function-call', 'new']);
    expect(out).toContainEqual(['builtin', 'block']);
    expect(out).toContainEqual(['string', '"tnt"']);
    expect(out).toContainEqual(['comment', '// done']);
  });

  it('gives an attribute its `#` and name as an `attribute` token and the arguments their own', () => {
    const flat = (toks: Array<string | Prism.Token>): Array<readonly [string, string]> =>
      toks.flatMap((t): Array<readonly [string, string]> =>
        typeof t === 'string'
          ? []
          : Array.isArray(t.content)
            ? flat(t.content)
            // The alias counts as the class too: `app.css` styles `.token.attribute`, whichever of the two names it came by.
            : [[t.type, String(t.content)] as const, ...(t.alias ? [t.alias].flat().map((a) => [a, String(t.content)] as const) : [])],
      );
    const out = flat(Prism.tokenize('#[on(DecorateChunk, decoration: "dungeon_doors")]\nfn f() {}', Prism.languages.patina));
    expect(out).toContainEqual(['attribute', '#']);
    expect(out).toContainEqual(['attribute', 'on']);
    expect(out).toContainEqual(['string', '"dungeon_doors"']);
    expect(out).toContainEqual(['class-name-type', 'DecorateChunk']);
    expect(out.filter(([k]) => k === 'attribute')).toHaveLength(2);
    expect(flat(Prism.tokenize('#[server]\n#[protected]', Prism.languages.patina)).filter(([k]) => k === 'attribute').map(([, x]) => x))
      .toEqual(['#', 'server', '#', 'protected']);
    expect(flat(Prism.tokenize('#![allow]', Prism.languages.patina))).toContainEqual(['attribute', '#!']);
  });

  it('reads `if (` as a keyword, not a call, and a duration as a number', () => {
    const out = types('if (ready) { wait(2s) }');
    expect(out).toContainEqual(['keyword', 'if']);
    expect(out).toContainEqual(['number', '2s']);
  });
});

describe('the markdown source view resolves the fence tag', () => {
  const resolve = (name: string) => LanguageDescription.matchLanguageName(MARKDOWN_FENCE_LANGUAGES, name, true);

  it('knows ptn, patina and any case', () => {
    for (const tag of ['ptn', 'patina', 'Patina', 'PTN']) {
      expect(resolve(tag)?.name).toBe('patina');
    }
  });

  it('does not steal other languages', () => {
    expect(resolve('rust')?.name).toBe('rust');
    expect(resolve('nonsense-lang')).toBeNull();
  });
});
