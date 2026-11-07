import { describe, it, expect } from 'vitest';
import { hoverLanguages } from './hover-languages';

describe('hoverLanguages', () => {
  it('colours a Patina signature and its doc blocks', () => {
    expect(hoverLanguages('/p/mods/hill.ptn')).toEqual({ codeLanguage: 'patina', signatureLanguage: 'patina' });
  });
  it('leaves other languages as they were', () => {
    expect(hoverLanguages('/p/src/main.rs')).toEqual({ codeLanguage: 'rust' });
    expect(hoverLanguages('/p/A.java')).toEqual({ codeLanguage: 'java' });
  });
});
