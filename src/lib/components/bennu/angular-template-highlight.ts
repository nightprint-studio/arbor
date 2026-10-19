/**
 * Angular template colouring, layered over `lang-html`.
 *
 * `lang-html` knows HTML and nothing of what Angular adds to it, so in a component template the
 * parts that carry the logic come out as plain text: the control-flow blocks (`@if (…) {` … `}`),
 * `{{ interpolation }}`, and the binding attributes (`[prop]`, `(event)`, `[(model)]`, `*ngIf`,
 * `#ref`). `ngserver` does not paint them either — it answers completion, hover and diagnostics for
 * a template but sends no semantic tokens for one — so this is the only colour they will get.
 *
 * A decoration layer, not a grammar: a real Angular Lezer grammar is a package to vendor, and what
 * is wanted here is that the file reads, not that it parses. Regexes over the visible lines keep it
 * cheap and stateless, at the cost of not understanding a block spread across lines — acceptable,
 * because every construct coloured here opens and closes on one line.
 *
 * Harmless on a non-Angular `.html`: `@if` and `{{ }}` mean nothing else in markup, and the same
 * `{{ }}` colouring is right for the other mustache-style templates people keep in `.html` anyway.
 */

import { RangeSet, type Range } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';

const keyword = Decoration.mark({ class: 'cm-ng-keyword' });
const expression = Decoration.mark({ class: 'cm-ng-expr' });
const binding = Decoration.mark({ class: 'cm-ng-binding' });

/** `@if (cond) {`, `} @else if (cond) {`, `@for (item of items; track item.id) {`, `@case (x) {`… */
const BLOCK_HEADER =
  /@(?:else if|if|else|for|empty|switch|case|default|defer|placeholder|loading|error|let)\b(\s*\([^)\n]*\))?/g;
/** A block's closing brace — only when it opens the line, or text with braces in it would match. */
const BLOCK_CLOSE = /^\s*(\})/;
const INTERPOLATION = /(\{\{)([\s\S]*?)(\}\})/g;
/** `[prop]`, `[(model)]`, `(event)`, `*ngIf`, `#ref`, as an attribute name inside a tag. */
const BINDING_ATTR = /(?<=[\s"'])(?:\[\(?[\w.@:-]+\)?\]|\([\w.@:-]+\)|\*[A-Za-z]\w*|#[\w-]+)(?=[=\s>/])/g;

function decorate(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const doc = view.state.doc;
  let lastLine = -1;

  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = doc.lineAt(pos);
      pos = line.to + 1;
      if (line.number === lastLine) continue;
      lastLine = line.number;
      const text = line.text;
      const at = line.from;

      for (const m of text.matchAll(BLOCK_HEADER)) {
        const start = at + m.index!;
        const head = m[0].length - (m[1]?.length ?? 0);
        ranges.push(keyword.range(start, start + head));
        if (m[1]) ranges.push(expression.range(start + head, start + m[0].length));
      }
      const close = BLOCK_CLOSE.exec(text);
      if (close) ranges.push(keyword.range(at + close[0].length - 1, at + close[0].length));

      for (const m of text.matchAll(INTERPOLATION)) {
        const start = at + m.index!;
        ranges.push(keyword.range(start, start + 2));
        if (m[2]) ranges.push(expression.range(start + 2, start + 2 + m[2].length));
        ranges.push(keyword.range(start + m[0].length - 2, start + m[0].length));
      }
      for (const m of text.matchAll(BINDING_ATTR)) {
        ranges.push(binding.range(at + m.index!, at + m.index! + m[0].length));
      }
    }
  }
  return RangeSet.of(ranges, true);
}

const angularTemplatePlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = decorate(view); }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) this.decorations = decorate(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

const angularTemplateTheme = EditorView.baseTheme({
  '.cm-ng-keyword': { color: 'var(--syntax-keyword)' },
  '.cm-ng-expr': { color: 'var(--syntax-field)' },
  '.cm-ng-binding': { color: 'var(--syntax-function)' },
});

/** The extension to put beside `html()` — see the file doc. */
export const angularTemplateHighlight = [angularTemplatePlugin, angularTemplateTheme];
