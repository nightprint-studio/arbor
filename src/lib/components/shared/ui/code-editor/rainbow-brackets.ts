/**
 * Rainbow brackets for the shared code editor.
 *
 * Colours every `()`/`[]`/`{}` by its NESTING DEPTH (matching open/close get the same
 * hue), so a block visually tells you which bracket it closes — the IntelliJ / VS Code
 * "Rainbow Brackets" affordance. Language-agnostic: a single left-to-right character scan
 * that tracks string / char / line-comment / block-comment state (so a `)` inside `"…"` or
 * `/* … *\/` is neither coloured nor allowed to skew the depth), then emits one mark
 * decoration per real bracket.
 *
 * The decoration set is rebuilt in a `StateField` on every doc change (CodeMirror renders
 * only the visible marks, so a full-document set is fine), and left untouched on pure
 * scrolls. A hard size cap keeps a pathologically large buffer from scanning on each
 * keystroke — above it, brackets simply aren't tinted (no correctness risk).
 */

import { EditorView, Decoration, type DecorationSet } from '@codemirror/view';
import { StateField, RangeSetBuilder, type Extension, type Text } from '@codemirror/state';

/** The depth palette: matching open/close brackets at nesting depth `d` wear `RAINBOW_COLORS[d % N]`.
 *  Exported so other chrome (the indentation guides) can tint a block's guide the SAME hue as the
 *  bracket that opens it — one source of truth for the block colours. */
export const RAINBOW_COLORS = [
  '#e6b422', // gold
  '#4fc1ff', // sky
  '#c586c0', // orchid
  '#6cc26c', // green
  '#e78a4e', // orange
  '#9aa0f5', // periwinkle
];

/** Number of distinct depth hues before the colours cycle. */
const DEPTH_COLORS = RAINBOW_COLORS.length;

/** Above this document size we stop tinting brackets — a full re-scan per keystroke on a
 *  multi-megabyte buffer isn't worth it, and an untinted bracket is a pure cosmetic loss. */
const MAX_CHARS = 400_000;

/** Cached mark decorations, one per depth bucket, reused across every bracket at that depth. */
const DEPTH_MARK = Array.from({ length: DEPTH_COLORS }, (_, i) =>
  Decoration.mark({ class: `cm-rainbow cm-rainbow-${i}` }),
);

/** Scanner state — which "skip region" (if any) the cursor is currently inside. Plain numeric
 *  constants (not a `const enum`, which `isolatedModules` disallows). */
const CODE = 0;
const LINE_COMMENT = 1;
const BLOCK_COMMENT = 2;
const DOUBLE_STRING = 3;
const SINGLE_STRING = 4;
type Ctx = 0 | 1 | 2 | 3 | 4;

/** Build the bracket decoration set for `doc`: one depth-coloured mark per real bracket
 *  (skipping strings / chars / comments). Returns an empty set past the size cap. */
function buildDecorations(doc: Text): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  if (doc.length === 0 || doc.length > MAX_CHARS) {
    return builder.finish();
  }
  const text = doc.toString();
  let ctx: Ctx = CODE;
  let depth = 0;
  const n = text.length;

  for (let i = 0; i < n; i++) {
    const c = text[i];
    switch (ctx) {
      case LINE_COMMENT:
        if (c === '\n') ctx = CODE;
        break;
      case BLOCK_COMMENT:
        if (c === '*' && text[i + 1] === '/') {
          ctx = CODE;
          i++; // consume the '/'
        }
        break;
      case DOUBLE_STRING:
        if (c === '\\') i++; // skip an escaped char
        else if (c === '"') ctx = CODE;
        break;
      case SINGLE_STRING:
        if (c === '\\') i++;
        else if (c === "'") ctx = CODE;
        break;
      case CODE:
        if (c === '/' && text[i + 1] === '/') {
          ctx = LINE_COMMENT;
          i++;
        } else if (c === '/' && text[i + 1] === '*') {
          ctx = BLOCK_COMMENT;
          i++;
        } else if (c === '"') {
          ctx = DOUBLE_STRING;
        } else if (c === "'") {
          ctx = SINGLE_STRING;
        } else if (c === '(' || c === '[' || c === '{') {
          // The opener wears its own depth; then we descend.
          builder.add(i, i + 1, DEPTH_MARK[depth % DEPTH_COLORS]);
          depth++;
        } else if (c === ')' || c === ']' || c === '}') {
          // The closer wears the matching opener's depth (so a pair shares a colour).
          if (depth > 0) depth--;
          builder.add(i, i + 1, DEPTH_MARK[depth % DEPTH_COLORS]);
        }
        break;
    }
  }
  return builder.finish();
}

/** The colour theme, built from {@link RAINBOW_COLORS} so palette + classes never drift. A bracket the
 *  caret matches (from `bracketMatching`) still gets its bold underline — the rainbow colour and the
 *  match highlight compose (colour from us, weight/box from matching). */
const rainbowThemeSpec: Record<string, { color?: string; fontWeight?: string }> = {
  '.cm-rainbow': { fontWeight: '600' },
};
RAINBOW_COLORS.forEach((c, i) => {
  rainbowThemeSpec[`.cm-rainbow-${i}`] = { color: c };
});
const rainbowTheme = EditorView.baseTheme(rainbowThemeSpec);

/** The rainbow-brackets extension: a doc-driven decoration field plus its colour theme. */
export function rainbowBrackets(): Extension {
  const field = StateField.define<DecorationSet>({
    create: (state) => buildDecorations(state.doc),
    update: (deco, tr) => (tr.docChanged ? buildDecorations(tr.state.doc) : deco),
    provide: (f) => EditorView.decorations.from(f),
  });
  return [field, rainbowTheme];
}
