/**
 * The change gutter — a slim bar beside the line numbers for every line that differs from the
 * committed file: green for added, blue for modified, a red notch where lines were removed.
 *
 * Kept apart from the blame bar (`line-annotations.ts`): that one answers *who*, this one answers
 * *whether*. The host computes the hunks (this only draws) and hears about clicks as a
 * {@link ChangeBlock} — the hunk plus where its marker run currently sits, so a click made
 * while the text moved on still names the right lines.
 *
 * Markers are anchored to their line, so typing above a block moves it with the text until the
 * host sends a fresh set.
 */
import { RangeSet, StateEffect, StateField } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { EditorView, gutter, GutterMarker } from '@codemirror/view';

export type ChangeKind = 'added' | 'modified' | 'removed';

/** One changed block, as the backend reports it. */
export interface ChangeHunk {
  kind: ChangeKind;
  /** 1-based first buffer line. */
  start_line: number;
  /** Buffer lines covered; 0 for a removal. */
  line_count: number;
  /** The block as it is in HEAD (empty for an addition). */
  old_text: string;
}

/** A clicked block: the hunk, and the lines its markers occupy *now*. */
export interface ChangeBlock {
  hunk: ChangeHunk;
  /** 1-based, inclusive. For a removal both are the line the gap sits before. */
  fromLine: number;
  toLine: number;
}

class ChangeMarker extends GutterMarker {
  hunk: ChangeHunk;

  constructor(hunk: ChangeHunk) {
    super();
    this.hunk = hunk;
  }

  // Identity, not value: two adjacent blocks of one kind must stay two blocks.
  eq(other: GutterMarker): boolean {
    return other instanceof ChangeMarker && other.hunk === this.hunk;
  }

  toDOM() {
    const el = document.createElement('span');
    el.className = `cm-change-bar cm-change-${this.hunk.kind}`;
    return el;
  }
}

const setChanges = StateEffect.define<RangeSet<GutterMarker>>();

const changeField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(current, tr) {
    for (const effect of tr.effects) if (effect.is(setChanges)) return effect.value;
    return current.map(tr.changes);
  },
});

/** The block whose markers include document line `line`, walking out to its first and last line. */
function blockAt(state: EditorState, line: number): ChangeBlock | null {
  const markers = state.field(changeField, false);
  if (!markers) return null;
  const doc = state.doc;
  const hunkOf = (n: number): ChangeHunk | null => {
    if (n < 1 || n > doc.lines) return null;
    let found: ChangeHunk | null = null;
    const at = doc.line(n).from;
    markers.between(at, at, (_f, _t, v) => {
      if (v instanceof ChangeMarker) found = v.hunk;
      return false;
    });
    return found;
  };
  const hunk = hunkOf(line);
  if (!hunk) return null;
  let fromLine = line;
  let toLine = line;
  while (hunkOf(fromLine - 1) === hunk) fromLine--;
  while (hunkOf(toLine + 1) === hunk) toLine++;
  return { hunk, fromLine, toLine };
}

const changeTheme = EditorView.baseTheme({
  '.cm-change-gutter': { minWidth: '0' },
  '.cm-change-gutter .cm-gutterElement': { padding: '0 1px 0 2px', cursor: 'pointer' },
  '.cm-change-bar': { display: 'block', width: '3px', height: '100%', borderRadius: '1px' },
  '.cm-change-added': { background: 'var(--success)' },
  '.cm-change-modified': { background: 'var(--info)' },
  // A removal has no line of its own: a small red wedge at the top edge of the line that follows.
  '.cm-change-removed': {
    height: '0',
    width: '0',
    marginTop: '0',
    background: 'transparent',
    borderTop: '4px solid transparent',
    borderBottom: '4px solid transparent',
    borderLeft: '5px solid var(--error)',
    borderRadius: '0',
    position: 'relative',
    top: '-4px',
  },
});

/** The extension. Always installed: with no markers the column is empty and collapses. */
export function changeGutterExtension(onClick: (block: ChangeBlock, event: MouseEvent) => void) {
  return [
    changeField,
    gutter({
      class: 'cm-change-gutter',
      markers: (v) => v.state.field(changeField, false) ?? RangeSet.empty,
      domEventHandlers: {
        mousedown(view, lineBlock, event) {
          const line = view.state.doc.lineAt(lineBlock.from).number;
          const block = blockAt(view.state, line);
          if (!block) return false;
          onClick(block, event as MouseEvent);
          return true;
        },
      },
    }),
    changeTheme,
  ];
}

/** Replace the editor's change markers. */
export function pushChangeHunks(view: EditorView, hunks: readonly ChangeHunk[]) {
  const doc = view.state.doc;
  const ranges = hunks.flatMap((hunk) => {
    const first = Math.min(Math.max(hunk.start_line, 1), doc.lines);
    const last = hunk.kind === 'removed' ? first : Math.min(first + hunk.line_count - 1, doc.lines);
    const out = [];
    for (let n = first; n <= last; n++) out.push(new ChangeMarker(hunk).range(doc.line(n).from));
    return out;
  });
  view.dispatch({ effects: setChanges.of(RangeSet.of(ranges, true)) });
}

/** The lines of every block, in document order — for "next / previous change". */
export function changeBlocks(state: EditorState): ChangeBlock[] {
  const markers = state.field(changeField, false);
  if (!markers) return [];
  const seen = new Set<ChangeHunk>();
  const out: ChangeBlock[] = [];
  const iter = markers.iter();
  while (iter.value) {
    const m = iter.value;
    if (m instanceof ChangeMarker && !seen.has(m.hunk)) {
      seen.add(m.hunk);
      const block = blockAt(state, state.doc.lineAt(iter.from).number);
      if (block) out.push(block);
    }
    iter.next();
  }
  return out;
}
