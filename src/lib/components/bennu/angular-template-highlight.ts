/**
 * Angular template colouring, layered over `lang-html`.
 *
 * `lang-html` knows HTML and nothing of what Angular adds to it, so in a component template the
 * parts that carry the logic come out as plain text: the control-flow blocks (`@if (…) {` … `}`),
 * `{{ interpolation | pipe }}`, and the binding attributes (`[prop]`, `(event)`, `[(model)]`,
 * `*ngIf`, `#ref`). `ngserver` does not paint them either — it answers completion, hover and
 * diagnostics for a template but, as far as the editor has ever seen, sends no semantic tokens for
 * one — so this local layer is the colour they get, with or without a server running.
 *
 * What is coloured, and how conservatively, is `angular-template-tokens.ts`; this file only draws
 * its spans. A decoration layer rather than a Lezer grammar: a real Angular grammar is a package to
 * vendor, and what is wanted is that the file reads, not that it parses.
 *
 * If the server *does* send semantic tokens they are drawn by the editor host as a layer of their
 * own, on top of this one; nothing here has to know.
 *
 * The scan covers the whole document, once per edit, not the viewport: an interpolation or a block
 * header can span lines and a viewport can start in the middle of one. It is linear, bounded by
 * {@link MAX_SCAN_LENGTH}, and skipped altogether when scrolling.
 */

import { Prec, RangeSet, type Range } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { MAX_SCAN_LENGTH, tokenizeAngular, type NgClass } from './angular-template-tokens';

const marks = new Map<NgClass, Decoration>();
function markFor(cls: NgClass): Decoration {
  let mark = marks.get(cls);
  if (!mark) marks.set(cls, (mark = Decoration.mark({ class: `cm-ng-${cls}` })));
  return mark;
}

function decorate(view: EditorView): DecorationSet {
  const doc = view.state.doc;
  if (doc.length > MAX_SCAN_LENGTH) return Decoration.none;
  const ranges: Range<Decoration>[] = [];
  for (const span of tokenizeAngular(doc.toString())) {
    if (span.to > span.from) ranges.push(markFor(span.cls).range(span.from, span.to));
  }
  return RangeSet.of(ranges, true);
}

const angularTemplatePlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = decorate(view); }
    update(u: ViewUpdate) {
      if (u.docChanged) this.decorations = decorate(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

// The same `--syntax-*` variables the Lezer highlight style reads, so an expression in a template
// is the colour of the same thing anywhere else in Bennu.
const angularTemplateTheme = EditorView.baseTheme({
  '.cm-ng-kw': { color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
  '.cm-ng-punct, .cm-ng-op': { color: 'var(--text-secondary)' },
  '.cm-ng-str': { color: 'var(--syntax-string, #6a8759)' },
  '.cm-ng-num, .cm-ng-lit': { color: 'var(--syntax-number, #6897bb)' },
  '.cm-ng-var': { color: 'var(--text-primary)' },
  '.cm-ng-prop': { color: 'var(--syntax-field, #9876aa)' },
  '.cm-ng-call': { color: 'var(--syntax-function, #ffc66d)' },
  '.cm-ng-pipe': { color: 'var(--syntax-builtin, #8888c6)', fontStyle: 'italic' },
  '.cm-ng-bind': { color: 'var(--syntax-field, #9876aa)' },
  '.cm-ng-event': { color: 'var(--syntax-function, #ffc66d)' },
  '.cm-ng-dir': { color: 'var(--syntax-keyword, #cc7832)' },
  '.cm-ng-ref': { color: 'var(--syntax-constant, #9876aa)', fontStyle: 'italic' },
});

/** The extension to put beside `html()` — see the file doc. Highest precedence so its colour is the innermost. */
export const angularTemplateHighlight = [Prec.high(angularTemplatePlugin), angularTemplateTheme];
