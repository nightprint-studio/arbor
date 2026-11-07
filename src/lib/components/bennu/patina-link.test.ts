import { describe, it, expect } from 'vitest';
import { patinaLinkRange } from './patina-link';

/** The text the link covers for a pointer on the `|` of `line` (the `|` is removed first). */
function linked(marked: string): string | null {
  const rel = marked.indexOf('|');
  const text = marked.replace('|', '');
  const r = patinaLinkRange(text, rel);
  return r ? text.slice(r.from, r.to) : null;
}

describe('patinaLinkRange: one segment, never the chain', () => {
  it('a method call is two links: the receiver and the method', () => {
    expect(linked('g|en.set_if_air(x)')).toBe('gen');
    expect(linked('gen.set_|if_air(x)')).toBe('set_if_air');
    expect(linked('gen.set_if_a|ir(x)')).toBe('set_if_air');
  });
  it('at the dot, the name on one side or the other — never both', () => {
    expect(linked('gen|.set_if_air(x)')).toBe('gen');
    expect(linked('gen.|set_if_air(x)')).toBe('set_if_air');
    expect(linked('a.|.b')).toBeNull(); // between two dots
    expect(linked('a..|b')).toBe('b');
    expect(linked('a|..b')).toBe('a');
  });
  it('an associated function: the segment on either side of `::`', () => {
    expect(linked('Blo|ckPos::new(1, 2)')).toBe('BlockPos');
    expect(linked('BlockPos::n|ew(1, 2)')).toBe('new');
    expect(linked('BlockPos|::new(1, 2)')).toBe('BlockPos');
    expect(linked('BlockPos::|new(1, 2)')).toBe('new');
  });
  it('a catalog path is as many links as segments', () => {
    expect(linked('ui::pa|usa::RIPRENDI')).toBe('pausa');
    expect(linked('ui::pausa::RIPR|ENDI')).toBe('RIPRENDI');
    expect(linked('u|i::pausa::RIPRENDI')).toBe('ui');
  });
  it('a form keeps its bang, and `!=` is not one', () => {
    expect(linked('bl|ock!("tnt")')).toBe('block!');
    expect(linked('a| != b')).toBe('a');
  });
  it('no link on a number, on spaces or on punctuation', () => {
    expect(linked('let x = 1|2;')).toBeNull();
    expect(linked('let x =| 5')).toBeNull();
    expect(linked('f(| )')).toBeNull();
  });
  it('no link inside a string or a comment', () => {
    expect(linked('say("mod:ke|y")')).toBeNull();
    expect(linked('let s = "a gen.set|_x b";')).toBeNull();
    expect(linked('let a = 1; // gen.se|t_x')).toBeNull();
    expect(linked('/// the gen.s|et')).toBeNull();
  });
  it('the string argument of a form links as a whole, hover and click alike', () => {
    expect(linked('ui!("pausa", "ripr|endi")')).toBe('riprendi');
    expect(linked('ui!("pa|usa", "riprendi")')).toBe('pausa');
    expect(linked('t!("mod:k|ey")')).toBe('mod:key');
    expect(linked('t!("mod:|key")')).toBe('mod:key');
    expect(linked('let b = block!("tn|t");')).toBe('tnt');
    expect(linked('item!("pa|la") + sound!("x")')).toBe('pala');
  });
  it('a plain string, or one outside the form\'s parentheses, stays without a link', () => {
    expect(linked('say("he|llo")')).toBeNull();
    expect(linked('let s = "mod:k|ey";')).toBeNull();
    expect(linked('t!("a") + say("mod:k|ey")')).toBeNull();
    expect(linked('t!(f("mod:k|ey"))')).toBeNull();
    expect(linked('// t!("mod:k|ey")')).toBeNull();
    expect(linked('t!("")|')).toBeNull();
  });
  it('code around a string still links, and an escaped quote does not end it', () => {
    expect(linked('say("hi").se|nd()')).toBe('send');
    expect(linked('f("a\\"b gen") + g|en')).toBe('gen');
    expect(linked('f("a\\"b ge|n") + gen')).toBeNull();
  });
});
