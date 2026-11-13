import { describe, expect, it } from 'vitest';
import { messageAsText, questionsAsText } from './assistant-copy';
import type { MessageBlock, QuestionSet } from './assistant-types';

const set: QuestionSet = {
  id: 'qs-1',
  questions: [
    { id: 'q1', header: 'Binari', text: 'Come li lancia?', multi: false, options: [{ label: 'Già costruiti', description: 'Li cerca in `target/`.' }, { label: 'Li costruisce' }] },
    { id: 'q2', header: 'Compilare', text: 'Posso compilare?', multi: false, options: [{ label: 'Sì' }, { label: 'No' }] },
  ],
};

describe('questionsAsText', () => {
  it('numbers several questions and lists their options', () => {
    expect(questionsAsText(set)).toBe(
      '1. **Binari** — Come li lancia?\n   - Già costruiti — Li cerca in `target/`.\n   - Li costruisce\n\n' +
      '2. **Compilare** — Posso compilare?\n   - Sì\n   - No',
    );
  });

  it('adds the answer once given, and does not number a single question', () => {
    const one: QuestionSet = { id: 'qs-2', questions: [set.questions[1]], answers: { q2: ['Sì'] } };
    expect(questionsAsText(one)).toBe('**Compilare** — Posso compilare?\n- Sì\n- No\nAnswer: Sì');
  });
});

describe('messageAsText', () => {
  it('keeps text and questions in order and leaves receipts out', () => {
    const blocks: MessageBlock[] = [
      { type: 'tool', call: { id: 't', kind: 'read', label: 'Read file', detail: '', ok: true } },
      { type: 'text', text: 'Ho letto la specifica.' },
      { type: 'questions', set: { id: 'qs-3', questions: [set.questions[1]] } },
    ] as MessageBlock[];
    expect(messageAsText(blocks)).toBe('Ho letto la specifica.\n\n**Compilare** — Posso compilare?\n- Sì\n- No');
  });
});
