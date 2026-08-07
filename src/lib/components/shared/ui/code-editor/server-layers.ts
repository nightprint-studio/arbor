/**
 * Two layers a **provider** supplies and the buffer cannot: where the symbol under the caret occurs,
 * and where the file folds.
 *
 * Both are here rather than in a language descriptor because both are *pushed*. The host makes the
 * backend call — it owns the debounce, the caret, and the knowledge that the answer is still for the
 * file on screen — and hands the result to the editor. The editor's job is to hold it and map it
 * through edits, which is what a `StateField` is for.
 *
 * ## Offsets and staleness
 *
 * Both arrive in **UTF-8 byte offsets** (the provider's coordinate) and are converted by the caller,
 * for the same reason the semantic-token layer does it: only the caller knows which buffer the
 * request was made against. And both are one beat behind the document — the user keeps typing while
 * the request is in flight — so each field maps its ranges through every change rather than dropping
 * them. A fold that follows its text is better than a gutter that empties on each keystroke.
 */

import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { StateEffect, StateField, type Extension, type Range } from '@codemirror/state';
import { foldService } from '@codemirror/language';

// ── document highlight ───────────────────────────────────────────────────────

/** One occurrence of the symbol under the caret, in document (UTF-16) positions. */
export interface HighlightRange {
  from: number;
  to: number;
  /** `read` · `write` · `text`. `text` is what a provider that did not distinguish gives, and it is
   *  the majority — so it must not be styled as a lesser kind of occurrence. */
  kind: string;
}

/** Replace the highlight layer. An empty array clears it. */
export const setDocumentHighlights = StateEffect.define<HighlightRange[]>();

const highlightMarks: Record<string, Decoration> = {
  read: Decoration.mark({ class: 'cm-occurrence cm-occurrence-read' }),
  write: Decoration.mark({ class: 'cm-occurrence cm-occurrence-write' }),
  text: Decoration.mark({ class: 'cm-occurrence' }),
};

function highlightSet(ranges: HighlightRange[], docLength: number): DecorationSet {
  const out: Range<Decoration>[] = [];
  for (const r of ranges) {
    if (r.to > r.from && r.from >= 0 && r.to <= docLength) {
      out.push((highlightMarks[r.kind] ?? highlightMarks.text).range(r.from, r.to));
    }
  }
  out.sort((a, b) => a.from - b.from || a.to - b.to);
  return Decoration.set(out, true);
}

const highlightField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setDocumentHighlights)) {
        return highlightSet(effect.value, tr.state.doc.length);
      }
    }
    // Mapped rather than cleared on an edit: the occurrences of the symbol you are typing are still
    // its occurrences, and clearing them would make the layer flicker on every keystroke.
    return tr.docChanged ? value.map(tr.changes) : value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/**
 * The occurrence-highlight layer.
 *
 * A write is tinted differently from a read, because "where is this assigned" is a different question
 * from "where is this used" and it is the one a mutation bug is found with. A provider that does not
 * distinguish them gets the neutral style — not the read style, which would be a claim it never made.
 */
export function documentHighlights(): Extension {
  return [
    highlightField,
    EditorView.baseTheme({
      '.cm-occurrence': {
        backgroundColor: 'color-mix(in srgb, var(--text-muted, #888) 18%, transparent)',
        borderRadius: '2px',
      },
      '.cm-occurrence-write': {
        backgroundColor: 'color-mix(in srgb, var(--warning, #d6a640) 22%, transparent)',
      },
    }),
  ];
}

// ── folding ──────────────────────────────────────────────────────────────────

/** A foldable region, in document (UTF-16) positions. */
export interface FoldRange {
  /** Where the fold begins — the end of the header line, so what names the region stays visible. */
  from: number;
  to: number;
  /** What to show in place of the folded text; empty for the editor's default. */
  placeholder?: string;
}

/** Replace the fold ranges. An empty array leaves the file unfoldable. */
export const setFoldRanges = StateEffect.define<FoldRange[]>();

/**
 * The ranges, sorted by start.
 *
 * A plain array and not a `RangeSet`: `foldService` asks "what folds on the line starting here", and
 * answering that is a scan for the range whose start is on that line. A hundred items is a hundred
 * entries, so a linear scan per queried line is cheaper than the machinery to avoid it.
 */
const foldField = StateField.define<FoldRange[]>({
  create() {
    return [];
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setFoldRanges)) {
        return [...effect.value].sort((a, b) => a.from - b.from);
      }
    }
    if (!tr.docChanged) return value;
    // Mapped, and a range the edit collapsed is dropped: a fold arrow on a region that no longer
    // spans anything folds nothing.
    const mapped: FoldRange[] = [];
    for (const r of value) {
      const from = tr.changes.mapPos(r.from, -1);
      const to = tr.changes.mapPos(r.to, 1);
      if (to > from) mapped.push({ ...r, from, to });
    }
    return mapped;
  },
});

/**
 * Folding driven by a provider's ranges.
 *
 * Worth having over the local alternatives: a legacy stream mode carries no fold information at all
 * (so a `.rs` file had no fold gutter), and brace matching would find the function bodies and nothing
 * else — where a server folds by *item*: a `use` block, a doc comment, a `#[cfg]`-gated module, a
 * match arm.
 *
 * The caller must also install `codeFolding()` and a fold gutter; this only supplies the ranges.
 */
export function serverFolding(): Extension {
  return [
    foldField,
    foldService.of((state, lineStart, lineEnd) => {
      for (const range of state.field(foldField)) {
        // The fold belongs to the line its start is on. `>= lineStart` and not `> lineStart` because
        // a fold may legitimately begin at the very start of a line (a region marker on its own
        // line), and `<= lineEnd` because it begins at the END of the header line — which IS
        // `lineEnd`.
        if (range.from >= lineStart && range.from <= lineEnd) {
          return range.to > range.from ? { from: range.from, to: range.to } : null;
        }
      }
      return null;
    }),
  ];
}
