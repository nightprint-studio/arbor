/**
 * Markdown documentation as the hover card reads it — the tree, not the DOM.
 */

import { describe, expect, it, vi } from 'vitest';

// The parser touches no DOM and no grammar; the highlighter is only reached when rendering.
vi.mock('$lib/utils/highlight', () => ({ highlightToDom: () => null }));

import { parseInline, parseMarkdown, type MdInline } from './hover-markdown';

const text = (t: string): MdInline => ({ t: 'text', text: t });

describe('parseMarkdown', () => {
  it('keeps a fenced example as a code block with its language', () => {
    const doc = 'Appends `x`.\n\n```ptn\nlet a = 7 / 2    // 3\n```';
    expect(parseMarkdown(doc)).toEqual([
      { t: 'p', inlines: [text('Appends '), { t: 'code', text: 'x' }, text('.')] },
      { t: 'code', lang: 'ptn', text: 'let a = 7 / 2    // 3' },
    ]);
  });

  it('lets a fence close a paragraph with no blank line before it', () => {
    // Patina's `///` writes examples straight under the prose.
    const blocks = parseMarkdown('Truncates toward zero.\n```ptn\nlet b = -7 / 2\n```');
    expect(blocks.map((b) => b.t)).toEqual(['p', 'code']);
  });

  it('keeps an unterminated fence rather than losing it', () => {
    expect(parseMarkdown('```rust\nfn f()')).toEqual([{ t: 'code', lang: 'rust', text: 'fn f()' }]);
  });

  it('reads headings, rules and paragraphs', () => {
    const blocks = parseMarkdown('# Examples\n\nOne\ntwo.\n\n---\n\nThree');
    expect(blocks).toEqual([
      { t: 'heading', inlines: [text('Examples')] },
      { t: 'p', inlines: [text('One two.')] },
      { t: 'hr' },
      { t: 'p', inlines: [text('Three')] },
    ]);
  });

  it('reads bullet and ordered lists, with nesting and continuation lines', () => {
    const [list] = parseMarkdown('- one\n  still one\n- two\n  - nested');
    expect(list).toEqual({
      t: 'list', ordered: false, start: 1,
      items: [
        [{ t: 'p', inlines: [text('one still one')] }],
        [
          { t: 'p', inlines: [text('two')] },
          { t: 'list', ordered: false, start: 1, items: [[{ t: 'p', inlines: [text('nested')] }]] },
        ],
      ],
    });
    const [ordered] = parseMarkdown('3. c\n4. d');
    expect(ordered).toMatchObject({ t: 'list', ordered: true, start: 3 });
  });

  it('drops reference definitions, which are targets and not text', () => {
    expect(parseMarkdown('See [`Vec`].\n\n[`Vec`]: std::vec::Vec')).toEqual([
      { t: 'p', inlines: [text('See '), { t: 'link', children: [{ t: 'code', text: 'Vec' }] }, text('.')] },
    ]);
  });

  it('reads a block quote', () => {
    expect(parseMarkdown('> careful\n> here')).toEqual([
      { t: 'quote', blocks: [{ t: 'p', inlines: [text('careful here')] }] },
    ]);
  });
});

describe('parseInline', () => {
  it('reads strong, emphasis and strikethrough', () => {
    expect(parseInline('**a** *b* ~~c~~')).toEqual([
      { t: 'strong', children: [text('a')] }, text(' '),
      { t: 'em', children: [text('b')] }, text(' '),
      { t: 'del', children: [text('c')] },
    ]);
  });

  it('does not read underscores inside a name as emphasis', () => {
    expect(parseInline('call set_if_air_now here')).toEqual([text('call set_if_air_now here')]);
  });

  it('does not look for marks inside code', () => {
    expect(parseInline('`a*b*c`')).toEqual([{ t: 'code', text: 'a*b*c' }]);
  });

  it('renders a link as its label and drops the target', () => {
    expect(parseInline('[the docs](https://example.com "t")')).toEqual([
      { t: 'link', children: [text('the docs')] },
    ]);
  });

  it('leaves a bracketed word in prose alone', () => {
    expect(parseInline('a [note] here')).toEqual([text('a [note] here')]);
  });

  it('keeps generics and escapes as written, and decodes entities', () => {
    expect(parseInline('Option<int> \\*x\\* &lt;T&gt;')).toEqual([
      text('Option<int> '), text('*'), text('x'), text('*'), text(' <T>'),
    ]);
  });

  it('turns a soft newline into a space and two trailing spaces into a break', () => {
    expect(parseInline('a\nb  \nc')).toEqual([text('a b'), { t: 'br' }, text('c')]);
  });
});
