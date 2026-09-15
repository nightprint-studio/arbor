/**
 * Enter in Java code, as text: `|` marks the caret before the key and after it. The break is applied
 * the way the editor applies it — trailing whitespace before the caret dropped, the moved text's
 * leading whitespace dropped, `{|}` split onto three lines — with a 4-space unit.
 */

import { describe, expect, it } from 'vitest';
import { javaIndentFor, javaLineBreak, type JavaIndentStyle } from './java-indent';

const STYLE: JavaIndentStyle = { unit: 4, tabSize: 4, indentCaseBody: true };

function enter(src: string, style: JavaIndentStyle = STYLE): string {
  const caret = src.indexOf('|');
  const before = src.slice(0, caret);
  const rest = src.slice(caret + 1);
  const eol = rest.indexOf('\n');
  const lineAfter = eol < 0 ? rest : rest.slice(0, eol);
  const tail = eol < 0 ? '' : rest.slice(eol);
  const brk = javaLineBreak(before, lineAfter, style);
  if (!brk) throw new Error('not a code break');
  const moved = lineAfter.slice(brk.trimAfter);
  const close = brk.closeIndent === null ? '' : `\n${' '.repeat(brk.closeIndent)}`;
  return `${before.slice(0, before.length - brk.trimBefore)}\n${' '.repeat(brk.caretIndent)}|${close}${moved}${tail}`;
}

describe('braces', () => {
  it('indents one level after an opening brace', () => {
    expect(enter('class A {|')).toBe('class A {\n    |');
  });

  it('puts the closing brace on its own line at the outer level', () => {
    expect(enter('class A {\n    void m() {|}\n}')).toBe('class A {\n    void m() {\n        |\n    }\n}');
  });

  it('keeps the outer level after a closing brace', () => {
    expect(enter('class A {\n    void m() {\n    }|')).toBe('class A {\n    void m() {\n    }\n    |');
  });

  it('indents a block from the line its statement starts on, not from a continuation line', () => {
    expect(enter('    if (a &&\n            b) {|')).toBe('    if (a &&\n            b) {\n        |');
  });

  it('dedents a closing brace typed on a body line', () => {
    expect(javaIndentFor('class A {\n    void m() {\n        foo();\n', '}', STYLE)).toBe(4);
  });

  it('ignores braces in strings, character literals and comments', () => {
    expect(enter('class A {\n    String s = "{"; char c = \'{\'; // {|')).toBe(
      'class A {\n    String s = "{"; char c = \'{\'; // {\n    |');
  });
});

describe('statements', () => {
  it('keeps the statement level after a semicolon', () => {
    expect(enter('class A {\n    void m() {\n        foo();|')).toBe('class A {\n    void m() {\n        foo();\n        |');
  });

  it('continues an unfinished statement two levels in', () => {
    expect(enter('    int total = price *|')).toBe('    int total = price *\n            |');
    expect(enter('    Runnable r = () ->|')).toBe('    Runnable r = () ->\n            |');
  });

  it('keeps the continuation level along a call chain', () => {
    expect(enter('    list.stream()\n            .map(x -> x)|')).toBe('    list.stream()\n            .map(x -> x)\n            |');
  });

  it('returns to the statement level once a multi-line call ends', () => {
    expect(enter('class A {\n    void m() {\n        foo(\n                a,\n                b);|'))
      .toBe('class A {\n    void m() {\n        foo(\n                a,\n                b);\n        |');
  });

  it('keeps an annotation and what it annotates at the same level', () => {
    expect(enter('class A {\n    @Override|')).toBe('class A {\n    @Override\n    |');
    expect(enter('class A {\n    @Named("x") @Inject|')).toBe('class A {\n    @Named("x") @Inject\n    |');
  });
});

describe('braceless control statements', () => {
  it('indents the body of an if, for, while and else one level', () => {
    expect(enter('    if (x)|')).toBe('    if (x)\n        |');
    expect(enter('    for (int i = 0; i < n; i++)|')).toBe('    for (int i = 0; i < n; i++)\n        |');
    expect(enter('    while (running)|')).toBe('    while (running)\n        |');
    expect(enter('    if (x)\n        a();\n    else|')).toBe('    if (x)\n        a();\n    else\n        |');
  });

  it('counts else-if as one level', () => {
    expect(enter('    } else if (y)|')).toBe('    } else if (y)\n        |');
  });

  it('returns to the outer level after the single body statement', () => {
    expect(enter('class A {\n    void m() {\n        if (x)\n            foo();|'))
      .toBe('class A {\n    void m() {\n        if (x)\n            foo();\n        |');
  });

  it('moves text split off after a header into the body', () => {
    expect(enter('    if (x) |return;')).toBe('    if (x)\n        |return;');
  });

  it('keeps a brace on its own line at the header level', () => {
    expect(javaIndentFor('    if (x)\n', '{', STYLE)).toBe(4);
  });
});

describe('parentheses', () => {
  it('continues inside an open argument list', () => {
    expect(enter('    foo(a,|')).toBe('    foo(a,\n            |');
    expect(enter('    foo(a, |b);')).toBe('    foo(a,\n            |b);');
  });

  it('indents a lambda body inside a call from the argument line', () => {
    expect(enter('    list.forEach(x -> {|});')).toBe('    list.forEach(x -> {\n        |\n    });');
  });
});

describe('switch', () => {
  const SWITCH = 'class A {\n    void m() {\n        switch (x) {\n';

  it('indents one level after a colon label', () => {
    expect(enter(`${SWITCH}        case 1:|`)).toBe(`${SWITCH}        case 1:\n            |`);
    expect(enter(`${SWITCH}        default:|`)).toBe(`${SWITCH}        default:\n            |`);
  });

  it('keeps the case body level after a statement', () => {
    expect(enter(`${SWITCH}        case 1:\n            foo();|`)).toBe(`${SWITCH}        case 1:\n            foo();\n            |`);
  });

  it('outdents a label typed in the body to the switch level', () => {
    expect(javaIndentFor(`${SWITCH}        case 1:\n            foo();\n`, 'case 2:', STYLE)).toBe(8);
  });

  it('indents the labels when the case body is not indented', () => {
    const flat = { ...STYLE, indentCaseBody: false };
    expect(javaIndentFor(SWITCH, 'case 1:', flat)).toBe(12);
    expect(enter(`${SWITCH}            case 1:|`, flat)).toBe(`${SWITCH}            case 1:\n            |`);
  });

  it('indents one level after an arrow label', () => {
    expect(enter(`${SWITCH}        case 1 ->|`)).toBe(`${SWITCH}        case 1 ->\n            |`);
  });

  it('does not take a ternary colon for a label', () => {
    expect(enter(`${SWITCH}        case 1 -> y = a ? b :|`)).toBe(`${SWITCH}        case 1 -> y = a ? b :\n                |`);
  });
});

describe('lists', () => {
  it('keeps enum constants and array elements at one level', () => {
    expect(enter('enum Color {\n    RED,|')).toBe('enum Color {\n    RED,\n    |');
    expect(enter('    int[] a = {\n        1,|')).toBe('    int[] a = {\n        1,\n        |');
  });
});

describe('whitespace', () => {
  it('drops trailing whitespace before the caret', () => {
    expect(enter('class A {\n    int x;   |')).toBe('class A {\n    int x;\n    |');
  });

  it('computes a blank line from the context instead of copying it', () => {
    expect(enter('class A {\n    void m() {\n            |')).toBe('class A {\n    void m() {\n\n        |');
  });

  it('writes tabs as tab-size columns', () => {
    const tabs = { ...STYLE, tabSize: 4 };
    expect(javaIndentFor('class A {\n\tvoid m() {\n', '', tabs)).toBe(8);
  });
});

describe('outside code', () => {
  it('leaves strings, text blocks and block comments to the caller', () => {
    expect(javaLineBreak('    String s = "ab', 'c";', STYLE)).toBeNull();
    expect(javaLineBreak('    String s = """\n        ab', '', STYLE)).toBeNull();
    expect(javaLineBreak('    /** doc', '', STYLE)).toBeNull();
  });
});
