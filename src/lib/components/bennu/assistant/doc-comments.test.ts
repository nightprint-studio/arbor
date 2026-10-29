import { describe, expect, it } from 'vitest';
import { docLanguageOf, docTargets } from './doc-comments';

const JAVA = [
  '/* License */',                                            // 1
  'package a;',                                               // 2
  '',                                                         // 3
  '/**',                                                      // 4
  ' * An order.',                                             // 5
  ' */',                                                      // 6
  'public class Order {',                                     // 7
  '    private int total;',                                    // 8
  '',                                                         // 9
  '    /** The amount due. */',                               // 10
  '    @Override',                                            // 11
  '    public BigDecimal total(Order o) {',                   // 12
  '        return helper(o);',                                // 13
  '    }',                                                    // 14
  '',                                                         // 15
  '    public void send(String to) throws IOException {',     // 16
  '        mailer.send(to);',                                 // 17
  '    }',                                                    // 18
  '}',                                                        // 19
].join('\n');

describe('where documentation help is offered', () => {
  it('puts a button on every comment that documents a declaration', () => {
    const t = docTargets(JAVA, 'java', 1).filter((x) => x.kind === 'comment');
    expect(t.map((x) => x.line)).toEqual([4, 10]);
    expect(t[0]).toMatchObject({ endLine: 6, declLine: 7 });
    expect(t[1]).toMatchObject({ endLine: 10, declLine: 12 });
    expect(t[1].declaration).toBe('public BigDecimal total(Order o) {');
  });

  it('finds the comment above an annotation that runs over several lines', () => {
    const src = [
      '/** The mail endpoint. */',
      '@RestController',
      '@RequestMapping(value = "mail",',
      '                method = RequestMethod.POST)',
      'public class MailController {',
      '}',
    ].join('\n');
    const t = docTargets(src, 'java', 1);
    expect(t).toHaveLength(1);
    expect(t[0]).toMatchObject({ line: 1, declLine: 5, declaration: 'public class MailController {' });
  });

  it('finds the comment above a field, or above anything else', () => {
    const src = ['class A {', '    /** The total. */', '    private int total;', '}'].join('\n');
    expect(docTargets(src, 'java', 1)[0]).toMatchObject({ line: 2, declLine: 3, declaration: 'private int total;' });
  });

  it('ignores a banner before the package line', () => {
    const src = ['/**', ' * Copyright.', ' */', 'package a;', 'class A {}'].join('\n');
    expect(docTargets(src, 'java', 1)).toHaveLength(0);
  });

  it('ignores a banner that documents nothing', () => {
    const t = docTargets(JAVA, 'java', 1);
    expect(t.some((x) => x.line === 1)).toBe(false);
  });

  it('offers to write a comment only on the undocumented declaration the caret is on', () => {
    expect(docTargets(JAVA, 'java', 16).find((x) => x.kind === 'undocumented')?.line).toBe(16);
    expect(docTargets(JAVA, 'java', 17).some((x) => x.kind === 'undocumented')).toBe(false);
    expect(docTargets(JAVA, 'java', 12).some((x) => x.kind === 'undocumented')).toBe(false);
  });

  it('does not mistake a statement for a declaration', () => {
    const src = ['class A {', '    void f() {', '        return compute(x);', '    }', '}'].join('\n');
    expect(docTargets(src, 'java', 3).some((x) => x.kind === 'undocumented')).toBe(false);
  });

  it('reads Rust doc runs and attributes between the comment and the item', () => {
    const rs = ['/// Adds.', '/// Two numbers.', '#[inline]', 'pub fn add(a: i32, b: i32) -> i32 {', '    a + b', '}', '', 'fn bare() {}'].join('\n');
    const t = docTargets(rs, 'rust', 8);
    expect(t.filter((x) => x.kind === 'comment')).toHaveLength(1);
    expect(t.find((x) => x.kind === 'comment')).toMatchObject({ line: 1, endLine: 2, declLine: 4 });
    expect(t.find((x) => x.kind === 'undocumented')?.line).toBe(8);
  });

  it('knows the languages it handles', () => {
    expect(docLanguageOf('src/A.java')).toBe('java');
    expect(docLanguageOf('lib.rs')).toBe('rust');
    expect(docLanguageOf('pom.xml')).toBeNull();
    expect(docLanguageOf(null)).toBeNull();
  });
});
