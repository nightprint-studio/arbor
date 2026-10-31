import { describe, expect, it } from 'vitest';
import { javaKeywordsAt, positionAt } from './java-keywords';

const body = (ret: string, text: string) => `package p;\n\n@Bean\nclass A {\n    ${ret} f() {\n        ${text}`;

describe('javaKeywordsAt — the reported case', () => {
  it('`re` in a method that returns a value opens on return, with no module words and no `record`', () => {
    const k = javaKeywordsAt(body('AttributeValidator', ''), 're')!;
    expect(k[0]).toBe('return');
    expect(k).not.toContain('requires');
    expect(k.indexOf('record')).toBeGreaterThan(k.indexOf('return'));
  });
  it('in a void method `return` comes after if/for/while', () => {
    const k = javaKeywordsAt(body('void', ''), '')!;
    expect(k.indexOf('return')).toBeGreaterThan(k.indexOf('while'));
    expect(k[0]).toBe('if');
  });
  it('`ret` finds return alone', () => {
    expect(javaKeywordsAt(body('int', ''), 'ret')).toEqual(['return']);
  });
  it('a constructor returns nothing', () => {
    const k = javaKeywordsAt('class A {\n    A() {\n        ', '')!;
    expect(k[0]).toBe('if');
  });
});

describe('javaKeywordsAt — other positions', () => {
  it('a class body offers member modifiers, not statements', () => {
    const k = javaKeywordsAt('class A {\n    ', 'pri')!;
    expect(k).toEqual(['private']);
    expect(javaKeywordsAt('class A {\n    ', 'ret')).toEqual([]);
    expect(javaKeywordsAt('class A {\n    ', 'req')).toEqual([]);
  });
  it('a modifier already typed leaves the member words', () => {
    expect(javaKeywordsAt('class A {\n    public ', 'st')).toContain('static');
  });
  it('the top level offers package/import/modifiers/types, and module words only in module-info', () => {
    expect(javaKeywordsAt('', 'pub')).toEqual(['public']);
    expect(javaKeywordsAt('', 'req')).toEqual([]);
    expect(javaKeywordsAt('', 'req', { moduleInfo: true })).toEqual(['requires']);
  });
  it('inside an expression only expression words are offered', () => {
    const k = javaKeywordsAt(body('int', 'int x = '), 're')!;
    expect(k).toEqual([]);
    expect(javaKeywordsAt(body('int', 'int x = '), 'ne')).toEqual(['new']);
  });
  it('else only right after the block of an if, catch/finally after a try', () => {
    expect(javaKeywordsAt(body('void', 'if (x) {\n            a();\n        }\n        '), 'el')).toEqual(['else']);
    expect(javaKeywordsAt(body('void', 'a();\n        '), 'el')).toEqual([]);
    expect(javaKeywordsAt(body('void', 'try {\n            a();\n        }\n        '), 'c')).toContain('catch');
  });
  it('yield only inside a switch', () => {
    expect(javaKeywordsAt(body('int', ''), 'yi')).toEqual([]);
    expect(javaKeywordsAt(body('int', 'int x = switch (y) {\n            case 1 -> {\n                '), 'yi')).toEqual(['yield']);
  });
});

describe('positionAt', () => {
  it('a return in a lambda block counts as returning a value', () => {
    const p = positionAt('class A {\n    void f() {\n        list.map(x -> {\n            ')!;
    expect(p.where).toBe('statement');
    expect(p.returnsValue).toBe(true);
  });
  it('an anonymous class body is a class body', () => {
    expect(positionAt('class A {\n    void f() {\n        run(new Runnable() {\n            ')?.where).toBe('class');
  });
  it('braces inside strings and comments do not count', () => {
    const p = positionAt('class A {\n    // {{{\n    String s = "}}}";\n    ')!;
    expect(p.where).toBe('class');
  });
  it('a stray closing brace is "cannot tell"', () => {
    expect(positionAt('}\n}\n')).toBeNull();
  });
});

describe('the name of a declaration', () => {
  const body = (tail: string) => `class A {\n  void f() {\n    ${tail}`;
  it('is not offered any keyword after a type or after `val`', () => {
    for (const tail of ['val ', 'String ', 'Map<String, List<Integer>> ', 'val x ', 'final Order ']) {
      expect(javaKeywordsAt(body(tail), '')).toEqual([]);
    }
  });
  it('leaves the words that start an expression or a statement alone', () => {
    for (const tail of ['return ', 'throw ', 'else ', 'new ']) {
      expect(javaKeywordsAt(body(tail), '')?.length ?? 0).toBeGreaterThan(0);
    }
  });
});
