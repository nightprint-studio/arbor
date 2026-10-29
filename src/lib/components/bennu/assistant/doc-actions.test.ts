import { describe, expect, it } from 'vitest';
import { actionsFor, docPrompt } from './doc-actions';
import type { DocTarget } from './doc-comments';

const comment: DocTarget = { line: 10, kind: 'comment', endLine: 12, declLine: 13, declaration: 'public BigDecimal total(Order o) {' };
const missing: DocTarget = { line: 16, kind: 'undocumented', endLine: 16, declLine: 16, declaration: 'public void send(String to) {' };

describe('the comment button', () => {
  it('offers to refine a comment, and to write one where there is none', () => {
    expect(actionsFor(comment).map((a) => a.id)).toEqual(['improve', 'review', 'shorten', 'translate-en', 'translate-it']);
    expect(actionsFor(missing).map((a) => a.id)).toEqual(['write']);
  });

  it('names the file, the lines and the declaration, so nobody has to type them', () => {
    const p = docPrompt('review', comment, 'src/Order.java', 'java');
    expect(p).toContain('File: src/Order.java');
    expect(p).toContain('lines 10–12');
    expect(p).toContain('public BigDecimal total(Order o) {');
    expect(p).toContain('Check the documentation comment against the code');
  });

  it('carries the language\'s convention and limits the edit to the comment', () => {
    expect(docPrompt('write', missing, 'A.java', 'java')).toContain('Javadoc');
    expect(docPrompt('write', missing, 'a.rs', 'rust')).toContain('rustdoc');
    expect(docPrompt('write', missing, 'a.rs', 'rust')).toContain('Change only the comment');
  });
});
