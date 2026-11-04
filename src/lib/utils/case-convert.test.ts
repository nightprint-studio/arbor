import { describe, expect, it } from 'vitest';
import { convertCase, cycleCase, detectCase, nextCase, splitWords } from './case-convert';

describe('splitWords', () => {
  it('splits on separators and on humps', () => {
    expect(splitWords('fooBarBaz')).toEqual(['foo', 'Bar', 'Baz']);
    expect(splitWords('foo_bar-baz qux')).toEqual(['foo', 'bar', 'baz', 'qux']);
    expect(splitWords('FOO_BAR')).toEqual(['FOO', 'BAR']);
  });

  it('keeps an acronym whole and ends it before the next word', () => {
    expect(splitWords('HTTPServer')).toEqual(['HTTP', 'Server']);
    expect(splitWords('parseXMLFile')).toEqual(['parse', 'XML', 'File']);
    expect(splitWords('getURL')).toEqual(['get', 'URL']);
  });

  it('keeps digits with the word they follow', () => {
    expect(splitWords('base64Encode')).toEqual(['base64', 'Encode']);
    expect(splitWords('HTTP2Server')).toEqual(['HTTP2', 'Server']);
    expect(splitWords('2fa')).toEqual(['2', 'fa']);
  });

  it('is unicode-aware', () => {
    expect(splitWords('città_vecchia')).toEqual(['città', 'vecchia']);
    expect(splitWords('ÉcoleNormale')).toEqual(['École', 'Normale']);
  });

  it('finds nothing in a string with no word', () => {
    expect(splitWords('__--')).toEqual([]);
    expect(splitWords('')).toEqual([]);
  });
});

describe('convertCase', () => {
  it('converts every style from every other', () => {
    for (const src of ['fooBarBaz', 'FooBarBaz', 'foo_bar_baz', 'FOO_BAR_BAZ', 'foo-bar-baz']) {
      expect(convertCase(src, 'camel')).toBe('fooBarBaz');
      expect(convertCase(src, 'pascal')).toBe('FooBarBaz');
      expect(convertCase(src, 'snake')).toBe('foo_bar_baz');
      expect(convertCase(src, 'screaming')).toBe('FOO_BAR_BAZ');
      expect(convertCase(src, 'kebab')).toBe('foo-bar-baz');
      expect(convertCase(src, 'title')).toBe('Foo Bar Baz');
    }
  });

  it('turns an acronym into a word, not into one letter each', () => {
    expect(convertCase('HTTPServer', 'snake')).toBe('http_server');
    expect(convertCase('parseXMLFile', 'screaming')).toBe('PARSE_XML_FILE');
    expect(convertCase('http_server', 'pascal')).toBe('HttpServer');
  });

  it('leaves digits on their word', () => {
    expect(convertCase('base64Encode', 'snake')).toBe('base64_encode');
    expect(convertCase('HTTP2Server', 'kebab')).toBe('http2-server');
    expect(convertCase('max_size_2', 'camel')).toBe('maxSize2');
  });

  it('keeps the underscores and dollars at the ends of a name', () => {
    expect(convertCase('_fooBar', 'snake')).toBe('_foo_bar');
    expect(convertCase('foo_bar_', 'camel')).toBe('fooBar_');
    expect(convertCase('$fooBar', 'screaming')).toBe('$FOO_BAR');
  });

  it('is a no-op on a name already in the target style', () => {
    expect(convertCase('fooBar', 'camel')).toBe('fooBar');
    expect(convertCase('foo_bar', 'snake')).toBe('foo_bar');
    expect(convertCase('FOO_BAR', 'screaming')).toBe('FOO_BAR');
  });

  it('upper- and lower-cases the text as it is', () => {
    expect(convertCase('Foo bar_Baz', 'upper')).toBe('FOO BAR_BAZ');
    expect(convertCase('Foo bar_Baz', 'lower')).toBe('foo bar_baz');
  });

  it('handles unicode', () => {
    expect(convertCase('città_vecchia', 'pascal')).toBe('CittàVecchia');
    expect(convertCase('ÉcoleNormale', 'snake')).toBe('école_normale');
    expect(convertCase('straße', 'upper')).toBe('STRASSE');
  });

  it('returns a string with no word unchanged', () => {
    expect(convertCase('__', 'camel')).toBe('__');
    expect(convertCase('', 'snake')).toBe('');
  });
});

describe('detectCase and the cycle', () => {
  it('names the style of an identifier', () => {
    expect(detectCase('fooBar')).toBe('camel');
    expect(detectCase('FooBar')).toBe('pascal');
    expect(detectCase('foo_bar')).toBe('snake');
    expect(detectCase('FOO_BAR')).toBe('screaming');
    expect(detectCase('foo-bar')).toBe('kebab');
    expect(detectCase('Foo_Bar')).toBeNull();
    expect(detectCase('foo bar')).toBeNull();
  });

  it('walks camel → Pascal → snake → SCREAMING → kebab → camel', () => {
    let text = 'fooBarBaz';
    const seen: string[] = [];
    for (let i = 0; i < 5; i++) {
      text = cycleCase(text);
      seen.push(text);
    }
    expect(seen).toEqual(['FooBarBaz', 'foo_bar_baz', 'FOO_BAR_BAZ', 'foo-bar-baz', 'fooBarBaz']);
  });

  it('walks a single word through lower, Capitalised and UPPER', () => {
    expect(cycleCase('foo')).toBe('Foo');
    expect(cycleCase('Foo')).toBe('FOO');
    expect(cycleCase('FOO')).toBe('foo');
  });

  it('starts a mixed spelling at camelCase', () => {
    expect(nextCase('Foo_Bar')).toBe('camel');
    expect(cycleCase('Foo_Bar')).toBe('fooBar');
  });
});
