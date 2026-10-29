import { describe, expect, it } from 'vitest';
import { classifyProblem, identifiersOf, markFresh, stemOf, vocabularyFor } from './assistant-voice';

describe('live transcript', () => {
  it('marks everything as fresh on the first piece', () => {
    expect(markFresh('', 'rename the').map((w) => w.fresh)).toEqual([true, true]);
  });

  it('settles the words that were already there and marks only the new tail', () => {
    const words = markFresh('rename the method', 'rename the method to total');
    expect(words.map((w) => w.text)).toEqual(['rename', 'the', 'method', 'to', 'total']);
    expect(words.map((w) => w.fresh)).toEqual([false, false, false, true, true]);
  });

  it('marks a revised word and what follows it as fresh, ignoring case', () => {
    const words = markFresh('Rename the metal', 'rename the method now');
    expect(words.map((w) => w.fresh)).toEqual([false, false, true, true]);
  });

  it('copes with empty text and extra whitespace', () => {
    expect(markFresh('a b', '')).toEqual([]);
    expect(markFresh('', '  a   b ').map((w) => w.text)).toEqual(['a', 'b']);
  });
});

describe('voice problems', () => {
  it('treats silence as something to retry', () => {
    expect(classifyProblem('No speech was heard.').kind).toBe('silent');
  });

  it('sends engine, model and microphone problems to the settings', () => {
    expect(classifyProblem('No speech model is selected. Choose one in Settings › Voice.').kind).toBe('setup');
    expect(classifyProblem('This build has no speech engine.').kind).toBe('setup');
    expect(classifyProblem('Microphone access was denied.').kind).toBe('setup');
  });

  it('shows anything else as it is', () => {
    const p = classifyProblem('The transcription took too long and was stopped.');
    expect(p.kind).toBe('other');
    expect(p.detail).toBe('The transcription took too long and was stopped.');
  });
});

describe('voice vocabulary', () => {
  it('takes the class name from a path', () => {
    expect(stemOf('src/main/java/com/acme/OrderService.java')).toBe('OrderService');
    expect(stemOf('C:\\p\\Foo.java')).toBe('Foo');
    expect(stemOf('a.b')).toBeNull();
    expect(stemOf(null)).toBeNull();
  });

  it('picks identifiers, drops keywords, ranks by use', () => {
    const words = identifiersOf('public void calculateTotal() { return calculateTotal() + taxRate; }');
    expect(words[0]).toBe('calculateTotal');
    expect(words).toContain('taxRate');
    expect(words).not.toContain('public');
    expect(words).not.toContain('return');
  });

  it('puts the active file first and the neighbours apart', () => {
    const v = vocabularyFor({
      projectName: 'payments', activePath: 'x/OrderService.java',
      openPaths: ['x/OrderService.java', 'x/TaxPolicy.java'], selection: 'retryCount',
    });
    expect(v.project).toBe('payments');
    expect(v.active).toEqual(['OrderService', 'retryCount']);
    expect(v.neighbours).toEqual(['TaxPolicy']);
  });
});
