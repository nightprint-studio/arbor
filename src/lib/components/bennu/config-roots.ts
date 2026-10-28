/**
 * Colour a YAML or `.properties` file by the **root** of each key.
 *
 * An `application.yml` is a handful of top-level trees (`spring`, `server`, `management`, the
 * project's own) and the eye has to find where one ends and the next starts through indentation
 * alone. Here every key takes the colour of the tree it belongs to — `spring.datasource.url` and
 * `spring.jpa.show-sql` share one, `server.port` has another — so a block reads as a block. The same
 * goes for a `.properties` file, where the root is the first segment of the dotted key.
 *
 * The colour comes from the root's NAME, not from its position: `spring` is the same colour in every
 * file and every session, which is what lets it be learnt. Eight hues from the editor's own syntax
 * palette, so the file still reads as part of the theme; two roots can share one, which costs
 * nothing, since the tree is also told apart by where it is.
 *
 * Only keys are coloured — values, comments and separators keep the language's own colours, and the
 * root key itself is heavier than the keys under it.
 */

import { RangeSetBuilder, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { propertyKeyOf } from './config-path';

export type ConfigKind = 'yaml' | 'properties';

/** Hues for roots, in the editor's syntax variables with the theme's own fallbacks. */
const HUES = 8;

/** A stable index for a root name. */
export function hueOf(root: string): number {
  let h = 5381;
  for (let i = 0; i < root.length; i++) h = ((h << 5) + h + root.charCodeAt(i)) | 0;
  return Math.abs(h) % HUES;
}

/** Past this many lines the file is a data dump, not a configuration, and is left alone. */
const MAX_LINES = 20_000;

const marks = Array.from({ length: HUES }, (_, i) => ({
  key: Decoration.mark({ class: `cm-cfg-root cm-cfg-root-${i}` }),
  root: Decoration.mark({ class: `cm-cfg-root cm-cfg-root-${i} cm-cfg-root-head` }),
}));

function yamlDecorations(state: EditorView['state']): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  let root = '';
  for (let n = 1; n <= state.doc.lines; n++) {
    const line = state.doc.line(n);
    const m = /^(\s*)(-\s+)?("[^"]*"|'[^']*'|[^\s:#'"-][^:#]*?|-[^\s:#][^:#]*?)(\s*):(?:\s|$)/.exec(line.text);
    if (!m) continue;
    const lead = m[1].length;
    const keyStart = line.from + lead + (m[2]?.length ?? 0);
    const keyEnd = keyStart + m[3].length;
    // A top-level key starts a tree; a dash before it (`- key:`) never does.
    if (lead === 0 && m[2] === undefined) root = m[3].replace(/^["']|["']$/g, '');
    if (!root) continue;
    const hue = marks[hueOf(root)];
    b.add(keyStart, keyEnd, lead === 0 && m[2] === undefined ? hue.root : hue.key);
  }
  return b.finish();
}

function propertiesDecorations(state: EditorView['state']): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  for (let n = 1; n <= state.doc.lines; n++) {
    const line = state.doc.line(n);
    const key = propertyKeyOf(line.text);
    if (!key) continue;
    const start = line.from + (line.text.length - line.text.trimStart().length);
    const root = key.split('.')[0];
    const hue = marks[hueOf(root)];
    const rootEnd = start + root.length;
    b.add(start, rootEnd, hue.root);
    if (start + key.length > rootEnd) b.add(rootEnd, start + key.length, hue.key);
  }
  return b.finish();
}

const rootTheme = EditorView.baseTheme({
  '.cm-cfg-root, .cm-cfg-root span': { transition: 'none' },
  // Specific enough to beat the language's own token class on the nested span.
  '.cm-cfg-root-0, .cm-cfg-root-0 span': { color: 'var(--syntax-keyword, #cc7832)' },
  '.cm-cfg-root-1, .cm-cfg-root-1 span': { color: 'var(--syntax-type, #4d9be6)' },
  '.cm-cfg-root-2, .cm-cfg-root-2 span': { color: 'var(--syntax-function, #ffc66d)' },
  '.cm-cfg-root-3, .cm-cfg-root-3 span': { color: 'var(--syntax-string, #6a8759)' },
  '.cm-cfg-root-4, .cm-cfg-root-4 span': { color: 'var(--syntax-field, #9876aa)' },
  '.cm-cfg-root-5, .cm-cfg-root-5 span': { color: 'var(--syntax-number, #6897bb)' },
  '.cm-cfg-root-6, .cm-cfg-root-6 span': { color: 'var(--syntax-constant, #4ec9b0)' },
  '.cm-cfg-root-7, .cm-cfg-root-7 span': { color: 'var(--syntax-annotation, #d16d9e)' },
  '.cm-cfg-root-head, .cm-cfg-root-head span': { fontWeight: '700' },
});

/** The extension: root-coloured keys for a YAML or `.properties` buffer. */
export function configRootColors(kind: ConfigKind): Extension {
  const build = kind === 'yaml' ? yamlDecorations : propertiesDecorations;
  const field = StateField.define<DecorationSet>({
    create: (state) => (state.doc.lines > MAX_LINES ? Decoration.none : build(state)),
    update: (value, tr) => {
      if (!tr.docChanged) return value;
      return tr.state.doc.lines > MAX_LINES ? Decoration.none : build(tr.state);
    },
    provide: (f) => EditorView.decorations.from(f),
  });
  return [field, rootTheme];
}
