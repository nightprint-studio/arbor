import { describe, expect, it } from 'vitest';
import { EditorState, type TransactionSpec } from '@codemirror/state';
import { acceptInlineCompletion, acceptInlineCompletionLine, dismissInlineCompletion, inlineCompletion, inlineOutcomes, inlineSuggestionField, setInlineSuggestion } from './inline-completion';

/** A state with the ghost-text field, holding `text` and a proposal drawn after it. */
function drawn(proposal: string, meta?: unknown) {
  const base = EditorState.create({ doc: 'foo(', selection: { anchor: 4 }, extensions: [inlineCompletion({ source: () => null })] });
  return base.update({ effects: setInlineSuggestion({ text: proposal, meta }, 4) }).state;
}

/** What the handler would be told for `spec` applied to a state showing a proposal. */
function told(state: EditorState, spec: TransactionSpec) {
  const tr = state.update(spec);
  return inlineOutcomes(state, [tr], tr.state);
}

describe('inline completion outcomes', () => {
  it('reports a proposal that is drawn, with the note the source put on it', () => {
    const base = EditorState.create({ doc: 'foo(', selection: { anchor: 4 }, extensions: [inlineCompletion({ source: () => null })] });
    const tr = base.update({ effects: setInlineSuggestion({ text: 'a, b)', meta: { origin: 'model' } }, 4) });
    expect(inlineOutcomes(base, [tr], tr.state)).toEqual([{ kind: 'shown', meta: { origin: 'model' } }]);
  });

  it('reports Esc as dismissed and Tab as accepted, once each', () => {
    /** The least of a view the two commands need: a state, a dispatch, and no plugin. */
    const fakeView = (initial: EditorState) => {
      const view = {
        state: initial,
        told: [] as { kind: string; meta: unknown }[],
        dispatch(spec: TransactionSpec) {
          const tr = view.state.update(spec);
          view.told.push(...inlineOutcomes(view.state, [tr], tr.state));
          view.state = tr.state;
        },
        plugin: () => null,
      };
      return view;
    };
    const accepted = fakeView(drawn('a, b)', 'n'));
    expect(acceptInlineCompletion(accepted as never)).toBe(true);
    expect(accepted.told).toEqual([{ kind: 'accepted', meta: 'n' }]);
    expect(accepted.state.doc.toString()).toBe('foo(a, b)');

    const dismissed = fakeView(drawn('a, b)', 'n'));
    expect(dismissInlineCompletion(dismissed as never)).toBe(true);
    expect(dismissed.told).toEqual([{ kind: 'dismissed', meta: 'n' }]);
    // nothing left to accept or dismiss: no second report
    expect(dismissInlineCompletion(dismissed as never)).toBe(false);
    expect(dismissed.told).toHaveLength(1);
  });

  it('reports an edit that is not the proposal as dismissed', () => {
    const state = drawn('a, b)', 'n');
    expect(told(state, { changes: { from: 4, insert: 'x' }, selection: { anchor: 5 } })).toEqual([{ kind: 'dismissed', meta: 'n' }]);
  });

  it('keeps what is left of a proposal while its first characters are typed, and drops it on anything else', () => {
    const state = drawn('a, b)', 'n');
    const text = (st: EditorState) => st.field(inlineSuggestionField, false)?.text ?? null;
    const typed = state.update({ changes: { from: 4, insert: 'a, ' }, selection: { anchor: 7 } }).state;
    expect(text(typed)).toBe('b)');
    // typing the rest ends it
    expect(text(typed.update({ changes: { from: 7, insert: 'b)' }, selection: { anchor: 9 } }).state)).toBeNull();
    // a character that is not next in the proposal drops it
    expect(text(state.update({ changes: { from: 4, insert: 'z' }, selection: { anchor: 5 } }).state)).toBeNull();
  });

  it('says nothing when the user types out what the proposal showed', () => {
    const state = drawn('a, b)', 'n');
    expect(told(state, { changes: { from: 4, insert: 'a' }, selection: { anchor: 5 } })).toEqual([]);
    expect(told(state, { changes: { from: 4, insert: 'a, ' }, selection: { anchor: 7 } })).toEqual([]);
  });

  it('reports moving the caret away as dismissed', () => {
    const state = drawn('a, b)', 'n');
    expect(told(state, { selection: { anchor: 1 } })).toEqual([{ kind: 'dismissed', meta: 'n' }]);
  });

  it('reports nothing for a transaction that leaves the proposal alone', () => {
    const state = drawn('a, b)', 'n');
    expect(told(state, { effects: [] })).toEqual([]);
  });
});

describe('accepting a block one line at a time', () => {
  const fake = (initial: EditorState) => {
    const view = {
      state: initial,
      dispatch(spec: TransactionSpec) { view.state = view.state.update(spec).state; },
      plugin: () => null,
    };
    return view;
  };
  const block = 'a, b) {\n    this.a = a;\n    this.b = b;\n}';

  it('takes a line with its break and keeps the rest drawn from where the caret lands', () => {
    const v = fake(drawn(block));
    expect(acceptInlineCompletionLine(v as never)).toBe(true);
    expect(v.state.doc.toString()).toBe('foo(a, b) {\n');
    expect(v.state.selection.main.head).toBe(12);
    const left = v.state.field(inlineSuggestionField);
    expect(left?.text).toBe('    this.a = a;\n    this.b = b;\n}');
    expect(left?.pos).toBe(12);
    expect(acceptInlineCompletionLine(v as never)).toBe(true);
    expect(acceptInlineCompletionLine(v as never)).toBe(true);
    expect(v.state.doc.toString()).toBe('foo(a, b) {\n    this.a = a;\n    this.b = b;\n');
    // the last line has no break: the line key leaves it to Tab
    expect(acceptInlineCompletionLine(v as never)).toBe(false);
    expect(acceptInlineCompletion(v as never)).toBe(true);
    expect(v.state.doc.toString().endsWith('}')).toBe(true);
  });

  it('leaves a one-line proposal to Tab', () => {
    const v = fake(drawn('a, b)'));
    expect(acceptInlineCompletionLine(v as never)).toBe(false);
    expect(v.state.doc.toString()).toBe('foo(');
  });

  it('overwrites the closer the editor added when the block carries its own', () => {
    const base = EditorState.create({ doc: 'foo()', selection: { anchor: 4 }, extensions: [inlineCompletion({ source: () => null })] });
    const state = base.update({ effects: setInlineSuggestion({ text: 'a) {\n}', replace: { from: 4, to: 5 }, hideReplaced: true }, 4) }).state;
    const v = fake(state);
    expect(acceptInlineCompletionLine(v as never)).toBe(true);
    expect(v.state.doc.toString()).toBe('foo(a) {\n');
    expect(acceptInlineCompletion(v as never)).toBe(true);
    expect(v.state.doc.toString()).toBe('foo(a) {\n}');
  });

  it('replaces what was typed before the caret when the block is the whole declaration', () => {
    const base = EditorState.create({ doc: '    toString', selection: { anchor: 12 }, extensions: [inlineCompletion({ source: () => null })] });
    const state = base.update({ effects: setInlineSuggestion({ text: '@Override\n    public String toString() {\n    }', replace: { from: 4, to: 12 }, hideReplaced: true }, 12) }).state;
    const v = fake(state);
    expect(acceptInlineCompletion(v as never)).toBe(true);
    expect(v.state.doc.toString()).toBe('    @Override\n    public String toString() {\n    }');
  });
});
