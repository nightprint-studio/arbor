/**
 * Indentation guides for the shared code editor.
 *
 * Draws a faint vertical line at every indent level, and BRIGHTENS the one that bounds the
 * block the caret sits in — the IntelliJ affordance for "which bracket does this block
 * close". Purely visual, computed from leading whitespace (no syntax tree), so it works in
 * any language the editor hosts.
 *
 * Rendering: one line decoration per visible line carrying an inline `background-image` of
 * stacked 1px vertical gradients at `contentPadLeft + level * indentWidth`. Recomputed on
 * doc / viewport / geometry / selection change; a blank line borrows the depth of the block
 * around it so guides run unbroken through empty lines (again, like IntelliJ).
 */

import { EditorView, Decoration, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { RangeSetBuilder, type Extension, type Line } from '@codemirror/state';
import { RAINBOW_COLORS } from './rainbow-brackets';

/** Left padding of `.cm-line` in the shared theme (the gap before the first glyph). Guides are
 *  positioned relative to the line box, so they start after this pad. Keep in sync with `theme.ts`. */
const LINE_PAD_LEFT = 12;

/** The indent depth (in levels) of a line's leading whitespace, given the tab width. A tab counts
 *  as a jump to the next multiple of `tabSize`; spaces count one column each. Returns the number of
 *  WHOLE indent levels, and whether the line is blank (only whitespace) — a blank line has no depth
 *  of its own and inherits the surrounding block's. */
function lineIndent(text: string, tabSize: number): { depth: number; blank: boolean } {
  let col = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === ' ') col++;
    else if (c === '\t') col += tabSize - (col % tabSize);
    else return { depth: Math.floor(col / tabSize), blank: false };
  }
  return { depth: 0, blank: true }; // whitespace-only (or empty)
}

/** Per-line guide depth across the WHOLE document (blank lines resolved to `min(prev, next)`
 *  non-blank depth so guides bridge empty lines). Indexed 1-based like `doc.line(n)`. */
function documentDepths(view: EditorView, tabSize: number): number[] {
  const doc = view.state.doc;
  const raw: { depth: number; blank: boolean }[] = new Array(doc.lines + 1);
  for (let n = 1; n <= doc.lines; n++) {
    raw[n] = lineIndent(doc.line(n).text, tabSize);
  }
  const depth: number[] = new Array(doc.lines + 1).fill(0);
  for (let n = 1; n <= doc.lines; n++) {
    if (!raw[n].blank) {
      depth[n] = raw[n].depth;
      continue;
    }
    // A blank line: the block around it is as deep as the shallower of its non-blank neighbours,
    // so a guide bridges the gap without over-drawing into a shallower sibling block.
    let prev = n - 1;
    while (prev >= 1 && raw[prev].blank) prev--;
    let next = n + 1;
    while (next <= doc.lines && raw[next].blank) next--;
    const p = prev >= 1 ? raw[prev].depth : 0;
    const q = next <= doc.lines ? raw[next].depth : 0;
    depth[n] = Math.min(p, q);
  }
  return depth;
}

/** The active guide level and the line range it spans — the block enclosing the caret. Level is
 *  0-based (the guide at `caretDepth - 1`); the range is the run of lines around the caret that stay
 *  at least that deep. `null` when the caret is at top level (no enclosing guide to highlight). */
function activeBlock(depth: number[], caretLine: number, lineCount: number): { level: number; from: number; to: number } | null {
  const d = depth[caretLine] ?? 0;
  if (d < 1) return null;
  const level = d - 1; // the innermost enclosing block's guide
  let from = caretLine;
  while (from > 1 && (depth[from - 1] ?? 0) > level) from--;
  let to = caretLine;
  while (to < lineCount && (depth[to + 1] ?? 0) > level) to++;
  return { level, from, to };
}

/** Opacity of an inactive guide vs the caret's active-block guide. The active one is fully opaque so
 *  it clearly stands out; the rest are dimmed so the wall of guides stays calm. */
const IDLE_ALPHA = 0.4;
const ACTIVE_ALPHA = 1;

/** A hex colour (`#rrggbb`) with an alpha channel appended as `#rrggbbaa` (8-digit hex — supported by
 *  the Chromium WebView). */
function withAlpha(hex: string, a: number): string {
  const aa = Math.round(Math.max(0, Math.min(1, a)) * 255).toString(16).padStart(2, '0');
  return `${hex}${aa}`;
}

/** Build the inline `background-image` drawing `count` guides, each TINTED with the same depth hue as
 *  the bracket that opens its block ({@link RAINBOW_COLORS}), so a guide and its `{ … }` match. The
 *  guide at `activeLevel` (the caret's block) is fully opaque; the rest are dimmed. Each line is a 1px
 *  vertical gradient at an INTEGER x (a fractional x antialiases a 1px line into near-invisibility). */
function guideBackground(count: number, activeLevel: number, indentPx: number): string {
  if (count <= 0) return '';
  const layers: string[] = [];
  for (let i = 0; i < count; i++) {
    const x = Math.round(LINE_PAD_LEFT + i * indentPx);
    const hue = RAINBOW_COLORS[i % RAINBOW_COLORS.length];
    const col = withAlpha(hue, i === activeLevel ? ACTIVE_ALPHA : IDLE_ALPHA);
    layers.push(
      `linear-gradient(90deg, transparent ${x}px, ${col} ${x}px, ${col} ${x + 1}px, transparent ${x + 1}px)`,
    );
  }
  return layers.join(', ');
}

const guideTheme = EditorView.baseTheme({
  // The guide layers paint UNDER the glyphs; keep them from tiling or shifting layout.
  '.cm-indent-line': { backgroundRepeat: 'no-repeat', backgroundClip: 'border-box' },
});

/** The indent-guides extension: a viewport-scoped decoration plugin plus its theme. */
/** Above this line count we skip guides — the per-edit whole-document depth scan isn't worth it on a
 *  giant file, and missing guides there is a pure cosmetic loss. */
const MAX_LINES = 20_000;

export function indentGuides(): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      /** Per-line indent depth, cached and rebuilt ONLY on a doc change — so a caret move (which
       *  only re-highlights the active block) never re-scans the whole document. */
      depths: number[];
      constructor(view: EditorView) {
        this.depths = documentDepths(view, view.state.tabSize);
        this.decorations = this.build(view);
      }
      update(u: ViewUpdate) {
        if (u.docChanged) this.depths = documentDepths(u.view, u.view.state.tabSize);
        if (u.docChanged || u.viewportChanged || u.geometryChanged || u.selectionSet) {
          this.decorations = this.build(u.view);
        }
      }
      build(view: EditorView): DecorationSet {
        const tabSize = view.state.tabSize;
        // Before the view has real font metrics `defaultCharacterWidth` can be 0 — fall back to a
        // sane monospace width so guides still draw, then a later geometry update re-runs with the
        // measured width. (Returning nothing on a 0 measure is why the guides never appeared.)
        const indentPx = (view.defaultCharacterWidth || 7.2) * tabSize;
        if (view.state.doc.lines > MAX_LINES) return Decoration.none;

        const depth = this.depths;
        const caretLine = view.state.doc.lineAt(view.state.selection.main.head).number;
        const active = activeBlock(depth, caretLine, view.state.doc.lines);

        const builder = new RangeSetBuilder<Decoration>();
        for (const { from, to } of view.visibleRanges) {
          let line: Line = view.state.doc.lineAt(from);
          while (line.from <= to) {
            const count = depth[line.number] ?? 0;
            const activeLevel =
              active && line.number >= active.from && line.number <= active.to ? active.level : -1;
            const bg = guideBackground(count, activeLevel, indentPx);
            if (bg) {
              builder.add(
                line.from,
                line.from,
                Decoration.line({ class: 'cm-indent-line', attributes: { style: `background-image: ${bg}` } }),
              );
            }
            if (line.to + 1 > view.state.doc.length) break;
            line = view.state.doc.lineAt(line.to + 1);
          }
        }
        return builder.finish();
      }
    },
    { decorations: (v) => v.decorations },
  );
  return [plugin, guideTheme];
}
