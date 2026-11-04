import { describe, expect, it } from 'vitest';
import { linkKey, scanTags, scanWikilinks } from './markdown-vault';

describe('scanWikilinks', () => {
  it('reads a plain link and its visible span', () => {
    const [w] = scanWikilinks('see [[Crash all avvio]] now');
    expect(w).toMatchObject({ target: 'Crash all avvio', heading: null, alias: null, embed: false });
    const line = 'see [[Crash all avvio]] now';
    expect(line.slice(w.textFrom, w.textTo)).toBe('Crash all avvio');
    expect(line.slice(w.from, w.to)).toBe('[[Crash all avvio]]');
  });

  it('shows the alias and hides the target half', () => {
    const line = '[[Note#Part|the part]]';
    const [w] = scanWikilinks(line);
    expect(w).toMatchObject({ target: 'Note', heading: 'Part', alias: 'the part' });
    expect(line.slice(w.textFrom, w.textTo)).toBe('the part');
  });

  it('keeps target and heading visible when there is no alias', () => {
    const line = '[[Note#Part]]';
    const [w] = scanWikilinks(line);
    expect(line.slice(w.textFrom, w.textTo)).toBe('Note#Part');
  });

  it('recognises embeds and several links on a line', () => {
    const found = scanWikilinks('![[img.png]] and [[b]]');
    expect(found.map((w) => [w.target, w.embed])).toEqual([
      ['img.png', true],
      ['b', false],
    ]);
  });

  it('ignores an empty target and unbalanced brackets', () => {
    expect(scanWikilinks('[[]] [[open')).toEqual([]);
  });
});

describe('scanTags', () => {
  it('finds tags, including nested ones and non-ASCII', () => {
    expect(scanTags('a #bug and #progetti/città, (#x_1)').map((t) => t.tag)).toEqual([
      'bug',
      'progetti/città',
      'x_1',
    ]);
  });

  it('is not fooled by headings, numbers or mid-word hashes', () => {
    expect(scanTags('# Title')).toEqual([]);
    expect(scanTags('issue #123 and a#b')).toEqual([]);
  });

  it('reports the span including the hash', () => {
    const line = 'x #tag';
    const [t] = scanTags(line);
    expect(line.slice(t.from, t.to)).toBe('#tag');
  });
});

describe('linkKey', () => {
  it('matches the index resolver: trim, drop .md, lowercase', () => {
    expect(linkKey('  Nota.md ')).toBe('nota');
    expect(linkKey('NOTA')).toBe('nota');
  });
});
