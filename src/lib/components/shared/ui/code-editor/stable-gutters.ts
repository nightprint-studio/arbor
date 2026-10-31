/**
 * Gutters that do not shrink back.
 *
 * The marker gutters (host icons, change bars, flags, annotations) hold nothing while there is nothing to mark, so
 * their column is zero wide — and while typing, what they mark comes and goes: a half-written line stops being a
 * method that can be run, a diagnostic is raised and cleared, and with each the whole text jumps sideways. The line
 * numbers already hold a width that does not depend on the file (see `stableGutterTheme`); this does the same for every
 * other column: once one has been as wide as its marks need, it keeps that width until the document is replaced.
 *
 * Grows only. A column that was never used stays at nothing, so an editor that does not use a gutter still pays
 * no space for it.
 */

import { ViewPlugin, type EditorView, type ViewUpdate } from '@codemirror/view';

/** The gutter columns that are not the line numbers, which keep a width of their own. */
const MARK_GUTTERS = '.cm-gutters > .cm-gutter:not(.cm-lineNumbers)';

export const stableGutters = ViewPlugin.fromClass(
  class {
    /** The widest each column has been, by its element. */
    private widest = new WeakMap<HTMLElement, number>();
    private generation = 0;

    constructor(private readonly view: EditorView) {
      this.measure();
    }

    update(update: ViewUpdate) {
      // the document replaced wholesale (another file in the same view): what the last one needed is not owed to this one
      if (update.docChanged && replacedWhole(update)) {
        this.generation++;
        this.view.dom.querySelectorAll<HTMLElement>(MARK_GUTTERS).forEach((el) => {
          this.widest.delete(el);
          el.style.minWidth = '';
        });
      }
      if (update.docChanged || update.viewportChanged || update.geometryChanged || update.transactions.length) this.measure();
    }

    private measure() {
      const view = this.view;
      const generation = this.generation;
      view.requestMeasure({
        key: this,
        read: () => [...view.dom.querySelectorAll<HTMLElement>(MARK_GUTTERS)].map((el) => [el, el.getBoundingClientRect().width] as const),
        write: (columns) => {
          if (generation !== this.generation) return;
          for (const [el, width] of columns) {
            if (width > (this.widest.get(el) ?? 0)) {
              this.widest.set(el, width);
              el.style.minWidth = `${Math.ceil(width)}px`;
            }
          }
        },
      });
    }
  },
);

/** Whether the update's changes cover (nearly) the whole of the previous document. */
function replacedWhole(update: ViewUpdate): boolean {
  let replaced = 0;
  update.changes.iterChangedRanges((fromA, toA) => {
    replaced += toA - fromA;
  });
  const before = update.startState.doc.length;
  return before > 0 && replaced >= before * 0.9;
}
