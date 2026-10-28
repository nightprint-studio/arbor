import { describe, expect, it, vi } from 'vitest';
import { EditorSelection, EditorState } from '@codemirror/state';
import { CompletionContext, type CompletionSource } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { hasMultipleCursors, multiCursor, quietWithMultipleCursors } from './multi-cursor';
import { expandOnTab } from './emmet';

const DOC = 'ul>li\nul>li\nul>li';

function state(selection: EditorSelection): EditorState {
  return EditorState.create({ doc: DOC, selection, extensions: [multiCursor()] });
}

const one = state(EditorSelection.single(5));
const three = state(EditorSelection.create([EditorSelection.cursor(5), EditorSelection.cursor(11), EditorSelection.cursor(17)]));

describe('multi-cursor', () => {
  it('keeps every range of a selection once enabled (off, CodeMirror collapses it to the main one)', () => {
    const plain = EditorState.create({
      doc: DOC,
      selection: EditorSelection.create([EditorSelection.cursor(1), EditorSelection.cursor(2)]),
    });
    expect(plain.selection.ranges).toHaveLength(1);
    expect(three.selection.ranges).toHaveLength(3);
  });

  it('knows when there is more than one cursor', () => {
    expect(hasMultipleCursors(one)).toBe(false);
    expect(hasMultipleCursors(three)).toBe(true);
  });

  it('a completion source answers with one cursor and says nothing with several', () => {
    const answer = { from: 0, options: [{ label: 'x' }] };
    const source = vi.fn<CompletionSource>(() => answer);
    const quiet = quietWithMultipleCursors(source);

    expect(quiet(new CompletionContext(one, 5, false))).toBe(answer);
    expect(quiet(new CompletionContext(three, 5, false))).toBeNull();
    expect(source).toHaveBeenCalledTimes(1);
  });

  it('Emmet leaves Tab alone with several cursors, so it indents instead', () => {
    const view = { state: three, dispatch: vi.fn() } as unknown as EditorView;
    expect(expandOnTab(view)).toBe(false);
    expect(view.dispatch).not.toHaveBeenCalled();
  });

  it('Emmet still expands with one cursor', () => {
    const dispatch = vi.fn();
    const view = { state: one, dispatch } as unknown as EditorView;
    expect(expandOnTab(view)).toBe(true);
    expect(dispatch).toHaveBeenCalledTimes(1);
  });
});
