import { describe, expect, it } from 'vitest';
import { actionPrompt, actionsFor, askDraft, lineOfByte, lineOfOffset, type ActionContext } from './assistant-actions';

const base: ActionContext = {
  file: 'src/Order.java', language: 'java', fromLine: 12, toLine: 12,
  selection: null, lineText: '        return helper(o);', problems: [], declaration: null, undocumented: false,
};

const ids = (c: ActionContext) => actionsFor(c).map((a) => a.id);

describe('what the margin mark offers', () => {
  it('offers to fix a problem first, with the explanation beside it', () => {
    const c = { ...base, problems: [{ severity: 'error', message: 'cannot find symbol helper' }] };
    expect(ids(c).slice(0, 2)).toEqual(['fix', 'explain-problem']);
    expect(actionsFor(c)[0].label).toBe('Fix this error');
    expect(actionsFor({ ...c, problems: [{ severity: 'warning', message: 'unused' }] })[0].label).toBe('Fix this warning');
  });

  it('offers what a selection is for', () => {
    expect(ids({ ...base, selection: 'a + b', toLine: 14 })).toEqual(['explain', 'bugs', 'improve', 'tests', 'ask']);
  });

  it('offers what a declaration is for, and documentation only when it has none', () => {
    const decl = { ...base, declaration: 'public BigDecimal total(Order o) {', lineText: 'public BigDecimal total(Order o) {' };
    expect(ids(decl)).toEqual(['explain', 'tests', 'usages', 'ask']);
    expect(ids({ ...decl, undocumented: true })).toEqual(['explain', 'tests', 'usages', 'document', 'ask']);
  });

  it('offers to explain a plain line, and only to ask about a blank one', () => {
    expect(ids(base)).toEqual(['explain', 'ask']);
    expect(ids({ ...base, lineText: '   ' })).toEqual(['ask']);
  });

  it('keeps the problem actions in front when there is a selection too', () => {
    const c = { ...base, selection: 'x', problems: [{ severity: 'error', message: 'm' }] };
    expect(ids(c)).toEqual(['fix', 'explain-problem', 'explain', 'bugs', 'improve', 'tests', 'ask']);
  });
});

describe('what each choice asks', () => {
  it('names the file and the lines and quotes the code, so nothing has to be typed', () => {
    const p = actionPrompt('explain', { ...base, selection: 'int x = 1;', fromLine: 3, toLine: 5 });
    expect(p).toContain('File: src/Order.java');
    expect(p).toContain('lines 3–5');
    expect(p).toContain('```java\nint x = 1;\n```');
    expect(p).toContain('Do not change anything');
  });

  it('puts the editor\'s own message in a fix request and tells the assistant to edit', () => {
    const p = actionPrompt('fix', { ...base, problems: [{ severity: 'error', message: 'cannot find symbol helper' }] });
    expect(p).toContain('- error: cannot find symbol helper');
    expect(p).toContain('edit_file');
  });

  it('asks for tests in the project\'s own conventions', () => {
    expect(actionPrompt('tests', base)).toContain('follow their conventions');
  });

  it('cuts a very long selection and says so', () => {
    const p = actionPrompt('explain', { ...base, selection: 'x'.repeat(7000) });
    expect(p).toContain('cut: 1000 more characters');
  });

  it('starts a free question with where it is about', () => {
    expect(askDraft({ ...base, fromLine: 8, toLine: 9 })).toBe('About src/Order.java, lines 8–9: ');
    expect(askDraft(base)).toBe('About src/Order.java, line 12: ');
  });
});

describe('finding the line of an offset', () => {
  const text = 'ab\nèx\n😀y\nlast';

  it('counts lines to a UTF-16 offset', () => {
    expect(lineOfOffset(text, 0)).toBe(1);
    expect(lineOfOffset(text, 3)).toBe(2);
    expect(lineOfOffset(text, text.length)).toBe(4);
  });

  it('counts lines to a UTF-8 byte offset, with multi-byte characters in between', () => {
    // bytes: a b \n = 3, è x \n = 2+1+1 = 4 (→ 7), 😀 y \n = 4+1+1 = 6 (→ 13), then "last"
    expect(lineOfByte(text, 0)).toBe(1);
    expect(lineOfByte(text, 4)).toBe(2);
    expect(lineOfByte(text, 8)).toBe(3);
    expect(lineOfByte(text, 14)).toBe(4);
  });
});
