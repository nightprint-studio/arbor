import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { useBlockRange, docCommentRange, patinaExtraFold } from './patina-fold';

const st = (s: string) => EditorState.create({ doc: s });

describe('useBlockRange', () => {
  it('piega i use consecutivi dopo la parola chiave', () => {
    const s = st('use a::b;\nuse c::d;\n\nfn x() {}\n');
    const r = useBlockRange(s)!;
    expect(s.sliceDoc(0, r.from)).toBe('use');
    expect(s.sliceDoc(r.from, r.to)).toBe(' a::b;\nuse c::d;');
  });
  it('un use su una riga sola non si piega', () => {
    expect(useBlockRange(st('use a::b;\nfn x() {}\n'))).toBeNull();
  });
  it('un use multiriga conta come un blocco', () => {
    const s = st('use a::{\n  b,\n  c\n};\nfn x() {}');
    const r = useBlockRange(s)!;
    expect(s.sliceDoc(r.from, r.to)).toBe(' a::{\n  b,\n  c\n};');
  });
  it('commenti e righe vuote fra use non interrompono, il codice si', () => {
    const s = st('// h\nuse a;\n\n// c\nuse b;\nfn f() {}\nuse z;\n');
    const r = useBlockRange(s)!;
    expect(s.sliceDoc(r.to - 6, r.to)).toBe('use b;');
  });
  it('senza `;` (Patina) il blocco finisce con l\'ultimo use, non al primo `;` del file', () => {
    const s = st('use a::b\nuse c::{d, e}\n\nfn x() { let y = 1; }\nfn z() { let w = 2; }\n');
    const r = useBlockRange(s)!;
    expect(s.sliceDoc(r.from, r.to)).toBe(' a::b\nuse c::{d, e}');
  });
  it('un use multiriga senza `;` si chiude alla graffa', () => {
    const s = st('use a::{\n  b,\n  c\n}\nuse d::e\nfn x() { 1; }');
    const r = useBlockRange(s)!;
    expect(s.sliceDoc(r.from, r.to)).toBe(' a::{\n  b,\n  c\n}\nuse d::e');
  });
  it('senza use: null', () => expect(useBlockRange(st('fn f() {}'))).toBeNull());
});

describe('docCommentRange', () => {
  const s = st('/// a\n/// b\n/// c\nfn f() {}\n//// no\n//// no\n');
  it('solo sulla testa della serie', () => {
    expect(docCommentRange(s, 0)).toEqual({ from: 5, to: 17 });
    expect(docCommentRange(s, 6)).toBeNull();
  });
  it('///// o //// non sono doc', () => expect(docCommentRange(s, s.doc.line(5).from)).toBeNull());
  it('una riga sola: niente', () => expect(docCommentRange(st('/// a\nfn x() {}'), 0)).toBeNull());
});

describe('patinaExtraFold', () => {
  it('risponde sulla riga del use e su quella del commento', () => {
    const s = st('use a;\nuse b;\n/// x\n/// y\nfn f() {}');
    expect(patinaExtraFold(s, 0, 6)).not.toBeNull();
    expect(patinaExtraFold(s, s.doc.line(3).from, s.doc.line(3).to)).not.toBeNull();
    expect(patinaExtraFold(s, s.doc.line(5).from, s.doc.line(5).to)).toBeNull();
  });
});
