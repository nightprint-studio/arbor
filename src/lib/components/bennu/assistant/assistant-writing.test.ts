import { describe, expect, it } from 'vitest';
import { WRITING_ACTIONS, writingDraft, writingPrompt } from './assistant-writing';

const c = { file: 'README.md', text: 'Arbor is a Git client.\nIt is fast.' };

describe('what the assistant offers on a passage', () => {
  it('offers the usual things done to prose, and a free question last', () => {
    expect(WRITING_ACTIONS.map((a) => a.id)).toEqual(['improve', 'shorten', 'expand', 'fix', 'translate-en', 'translate-it', 'ask']);
  });

  it('quotes the passage, names the file and asks for an exact edit', () => {
    const p = writingPrompt('shorten', c);
    expect(p).toContain('File: README.md');
    expect(p).toContain('```markdown\nArbor is a Git client.\nIt is fast.\n```');
    expect(p).toContain('edit_file');
    expect(p).toContain('exactly as it appears');
  });

  it('keeps names and code when translating', () => {
    expect(writingPrompt('translate-it', c)).toContain('as they are');
  });

  it('cuts a very long passage and says so', () => {
    expect(writingPrompt('improve', { ...c, text: 'x'.repeat(7000) })).toContain('… (cut)');
  });

  it('starts a free question with what it is about', () => {
    expect(writingDraft(c)).toBe('About "Arbor is a Git client.…" in README.md: ');
    expect(writingDraft({ file: 'a.md', text: 'Short' })).toBe('About "Short" in a.md: ');
  });
});
