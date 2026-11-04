/**
 * The caret of the shared code editor: its shape, thickness, blink and colour.
 *
 * Drawn by CodeMirror's `drawSelection` layer, so every cursor — the main one, the secondary ones of
 * a multi-cursor selection, and the drop cursor of a drag — is a `.cm-cursor` element and is styled
 * by the one rule set below. Nothing here touches the editor state: the rules are a theme, rebuilt
 * when the preference changes (see `buildPreferences`).
 *
 * ## Why it is not just "the accent"
 *
 * The current line is tinted with the accent and the selection is the accent too, so an accent caret
 * sat on a wash of its own colour and nearly vanished next to a `);`. The default is therefore the
 * `--editor-caret` token — the theme's foreground nudged toward the accent — drawn two pixels wide
 * with a one-pixel halo in the editor's background colour, which separates it from whatever it is
 * standing on.
 */
import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';

export type CaretStyle = 'line' | 'block' | 'underline';
export type CaretWidth = 'thin' | 'normal' | 'thick';
export type CaretBlink = 'smooth' | 'standard' | 'solid';

export interface CaretPreferences {
  /** `line` (default), `block` (a cell over the character) or `underline`. */
  style?: CaretStyle;
  /** Thickness of the line, and of the underline. A block is one character wide whatever this says. */
  width?: CaretWidth;
  /** `standard` (default, CodeMirror's on/off), `smooth` (fades) or `solid` (never blinks). */
  blink?: CaretBlink;
  /** `auto` (default, the `--editor-caret` token), `accent`, or a `#rrggbb` colour. */
  color?: string;
}

export const CARET_STYLES: readonly CaretStyle[] = ['line', 'block', 'underline'];
export const CARET_WIDTHS: readonly CaretWidth[] = ['thin', 'normal', 'thick'];
export const CARET_BLINKS: readonly CaretBlink[] = ['smooth', 'standard', 'solid'];

/** Pixels of a line caret (and an underline). */
export const CARET_WIDTH_PX: Record<CaretWidth, number> = { thin: 1, normal: 2, thick: 3 };

const HEX = /^#[0-9a-fA-F]{6}$/;

/** The CSS colour a `color` preference means. Anything it does not recognise is `auto`: a hand-edited
 *  config must never end up as an invisible caret. */
export function resolveCaretColor(color?: string): string {
  if (color === 'accent') return 'var(--accent)';
  if (color && HEX.test(color)) return color;
  return 'var(--editor-caret, var(--text-primary))';
}

/** The theme rules for `prefs`, as a plain object (so they can be tested without a view). */
export function caretRules(prefs: CaretPreferences = {}): Record<string, Record<string, string>> {
  const style = prefs.style && CARET_STYLES.includes(prefs.style) ? prefs.style : 'line';
  const px = CARET_WIDTH_PX[prefs.width && prefs.width in CARET_WIDTH_PX ? prefs.width : 'normal'];
  const color = resolveCaretColor(prefs.color);
  // A halo in the editor's own background: on the tinted current line it is what separates the caret
  // from the wash, and on a plain line it is invisible.
  const halo = '0 0 0 1px color-mix(in srgb, var(--bg-base) 60%, transparent)';

  const shape: Record<string, string> =
    style === 'block'
      ? {
          borderLeft: `1ch solid color-mix(in srgb, ${color} 55%, transparent)`,
          marginLeft: '0',
          boxSizing: 'border-box',
          boxShadow: `0 0 0 1px ${color}`,
        }
      : style === 'underline'
        ? {
            borderLeft: 'none',
            borderBottom: `${px}px solid ${color}`,
            marginLeft: '0',
            width: '1ch',
            boxSizing: 'border-box',
            boxShadow: halo,
          }
        : {
            borderLeft: `${px}px solid ${color}`,
            marginLeft: `${-px / 2}px`,
            boxShadow: halo,
          };

  const layer = '&.cm-focused > .cm-scroller > .cm-cursorLayer';
  const blink: Record<string, string> =
    prefs.blink === 'smooth'
      ? { animationTimingFunction: 'ease-in-out' }
      // Zero iterations: the layer's name and duration are set inline by CodeMirror, so `animation: none`
      // would lose to them — but nothing can lose to "never run".
      : prefs.blink === 'solid'
        ? { animationIterationCount: '0' }
        : {};

  return {
    '.cm-cursor, .cm-dropCursor': shape,
    // Every cursor of a multi-cursor selection is the same caret; the secondary ones are not dimmed,
    // because a caret you cannot see is the thing this module exists to prevent.
    '.cm-cursor-secondary': { opacity: '0.9' },
    [layer]: blink,
  };
}

/** The caret as an extension: the rules for `prefs`, scoped to the editor they are installed in. */
export function caretExtension(prefs?: CaretPreferences): Extension {
  return EditorView.theme(caretRules(prefs));
}
