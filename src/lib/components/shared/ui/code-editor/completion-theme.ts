/**
 * The look of the completion popup and of the documentation panel beside it.
 *
 * ## Design
 *
 * IntelliJ's New UI popup, read as four columns on a comfortable row:
 *
 *   [icon]  name(Type param, …)   ………   ReturnType   Origin
 *
 * - **Icon**: the SVG set of `completion-icons` — shape and colour by kind, with marks for static,
 *   final, abstract and deprecated. It is the only coloured thing on the row, so the list does not
 *   turn into a rainbow, and it uses the theme's status tokens rather than syntax colours, because
 *   those are defined for every theme.
 * - **Label**: in the code font at the editor's size. The typed characters are drawn in the accent
 *   colour and bold, not underlined. A member the receiver **declares** is bold, an **inherited**
 *   one regular, an **implicit** one (what every object has) dimmed. A deprecated candidate is
 *   struck through and dimmed.
 * - **Signature**: the parameter list, straight after the name, in the regular weight and muted.
 * - **Detail**: the return or field type, right-aligned and muted; the first thing to truncate.
 *   **Origin** (where an inherited member is declared) is quieter still and sits on the right edge.
 * - **Selection**: an accent tint with rounded corners, inset from the popup edge by the list's own
 *   padding.
 *
 * ## Why every selector starts with `.cm-tooltip.cm-tooltip-autocomplete`
 *
 * CodeMirror's base theme styles the popup through that exact double-class selector. A theme rule
 * written as plain `.cm-tooltip-autocomplete > ul > li` is one class less specific and silently
 * loses: the base `padding: 1px 3px`, `line-height: 1.2` and `max-height: 10em` win, and the popup
 * looks cramped however the theme is written. With the same prefix the specificity ties, and a tie
 * goes to the editor theme because it is mounted after the base theme.
 *
 * Keyboard behaviour is untouched: this module is styling only.
 */

import { COMPLETION_ICON_KINDS } from './completion-icons';

/** The popup container. */
const POPUP = '.cm-tooltip.cm-tooltip-autocomplete';
/** One candidate row. The list is `ul[role=listbox]`, and CodeMirror sets that role itself. */
const ROW = `${POPUP} > ul > li`;

/** A row's height, in `em` of the list font, so it follows the user's font scale. At the default
 *  13px it comes to about 24px. */
const ROW_EM = 1.85;
/** How many rows the popup shows before it scrolls. */
const VISIBLE_ROWS = 13;

type StyleRule = Record<string, string>;

/** The per-kind colours, generated from {@link COMPLETION_ICON_KINDS} so a kind's shape, glyph and
 *  colour are declared in one place. Every part of the icon reads `currentColor`. */
const kindRules: Record<string, StyleRule> = Object.fromEntries(
  COMPLETION_ICON_KINDS.map(({ type, color }) => [
    `${POPUP} .cm-completionKindIcon[data-kind="${type}"]`,
    { color },
  ]),
);

/** Spread into the editor theme (`EditorView.theme`). */
export const completionThemeSpec: Record<string, StyleRule> = {
  // ── Container ──
  [POPUP]: {
    backgroundColor: 'var(--bg-elevated)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--radius-lg)',
    boxShadow: 'var(--shadow-lg)',
    overflow: 'hidden',
  },
  [`${POPUP} > ul`]: {
    fontFamily: 'var(--font-code)',
    fontSize: 'var(--font-size-md)',
    padding: '4px',
    minWidth: 'min(360px, 90vw)',
    maxWidth: 'min(760px, 92vw)',
    // The 8px is the list's own top and bottom padding.
    maxHeight: `calc(${VISIBLE_ROWS} * ${ROW_EM}em + 8px)`,
    scrollbarWidth: 'thin',
  },

  // ── Row ──
  [ROW]: {
    display: 'flex', alignItems: 'center', gap: '8px',
    minHeight: `${ROW_EM}em`, boxSizing: 'border-box',
    padding: '0 10px 0 5px',
    lineHeight: '1.3',
    borderRadius: 'var(--radius-sm)',
    color: 'var(--text-primary)',
  },
  [`${ROW}[aria-selected]`]: {
    backgroundColor: 'color-mix(in srgb, var(--accent) 24%, transparent)',
    color: 'var(--text-primary)',
  },
  // While the list waits for a fresh answer, CodeMirror greys the selection out. Keep the tint and
  // soften it instead, so the highlight does not flash grey on every keystroke.
  [`${POPUP}.cm-tooltip-autocomplete-disabled > ul > li[aria-selected]`]: {
    backgroundColor: 'color-mix(in srgb, var(--accent) 12%, transparent)',
    color: 'var(--text-primary)',
  },

  // ── Kind icon ──
  // In `em` of the list font, so it follows the user's font scale: about 17px at the default 13px.
  [`${POPUP} .cm-completionKindIcon`]: {
    flex: '0 0 auto', display: 'block',
    width: '1.3em', height: '1.3em',
    color: 'var(--text-muted)',
  },
  [`${POPUP} .cm-completionKindIcon svg`]: {
    display: 'block', width: '100%', height: '100%', overflow: 'visible',
  },
  [`${POPUP} .cm-ki-body`]: {
    fill: 'color-mix(in srgb, currentColor 16%, transparent)',
    stroke: 'currentColor', strokeWidth: '1.2',
  },
  // Abstract: nothing is there yet, so the body is outlined and left empty.
  [`${POPUP} .cm-ki-abstract`]: { fill: 'transparent', strokeDasharray: '2.2 1.6' },
  [`${POPUP} .cm-ki-glyph`]: {
    fill: 'currentColor', stroke: 'none',
    fontFamily: 'var(--font-code)', fontWeight: '700',
  },
  // The static dot and the final lock sit on a ring of the popup's own background, so they read as
  // a mark on the icon rather than part of its outline.
  [`${POPUP} .cm-ki-mark`]: {
    fill: 'currentColor', stroke: 'var(--bg-elevated)', strokeWidth: '1.4', paintOrder: 'stroke',
  },
  [`${ROW}[aria-selected] .cm-ki-mark`]: {
    stroke: 'color-mix(in srgb, var(--accent) 24%, var(--bg-elevated))',
  },
  [`${POPUP} .cm-ki-shackle`]: { fill: 'none', stroke: 'currentColor', strokeWidth: '1.2' },
  [`${POPUP} .cm-ki-strike`]: {
    stroke: 'var(--text-secondary)', strokeWidth: '1.5', strokeLinecap: 'round',
  },
  ...kindRules,

  // ── Label ──
  // The label is what is being chosen, so it keeps its width. It gives way only when it alone
  // would push everything else out of a full-width popup.
  [`${POPUP} .cm-completionLabel`]: {
    flex: '0 0 auto', maxWidth: '65%',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'pre',
    color: 'var(--text-primary)',
  },
  [`${POPUP} .cm-completionMatchedText`]: {
    color: 'var(--accent)', fontWeight: '700', textDecoration: 'none',
  },
  // Standing: what the receiver declares is what is most likely being looked for; what every
  // object has recedes.
  [`${ROW}.cm-completion-own .cm-completionLabel`]: { fontWeight: '600' },
  [`${ROW}.cm-completion-implicit .cm-completionLabel`]: { color: 'var(--text-secondary)' },
  [`${ROW}.cm-completion-implicit .cm-completionKindIcon`]: { opacity: '0.7' },

  // ── Signature ──
  // Part of the name, so it follows the label with no gap (the row's `gap` is cancelled), and it
  // gives way before the label does.
  [`${POPUP} .cm-completionSignature`]: {
    flex: '0 1 auto', minWidth: '0', marginLeft: '-8px',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'pre',
    color: 'var(--text-secondary)',
  },

  // ── Detail + origin ──
  [`${POPUP} .cm-completionDetail`]: {
    flex: '1 1 auto', minWidth: '0', margin: '0 0 0 12px',
    textAlign: 'right',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
    fontStyle: 'normal', fontSize: '0.9em',
    color: 'var(--text-muted)',
  },
  [`${ROW}[aria-selected] .cm-completionDetail`]: { color: 'var(--text-secondary)' },
  [`${POPUP} .cm-completionOrigin`]: {
    flex: '0 0 auto', maxWidth: '14em', marginLeft: 'auto', paddingLeft: '10px',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
    fontSize: '0.85em',
    color: 'var(--text-muted)', opacity: '0.75',
  },

  // ── Deprecated ──
  // Still offered, since it exists and you may be reading old code. Struck through, the same mark
  // the editor puts on a deprecated symbol in the text.
  [`${ROW}.cm-completion-deprecated .cm-completionLabel`]: {
    textDecoration: 'line-through', color: 'var(--text-muted)',
  },
  [`${ROW}.cm-completion-deprecated .cm-completionKindIcon`]: { opacity: '0.55' },

  // ── The documentation panel beside the list ──
  //
  // The same card the hover tooltip draws, from the same backend answer (see Bennu's
  // `completion-item`). Its size is capped so it cannot outgrow the popup it hangs off: a panel
  // taller than the screen is one whose top you cannot read.
  '.cm-tooltip.cm-completionInfo': {
    backgroundColor: 'var(--bg-elevated)',
    border: '1px solid var(--border)', borderRadius: 'var(--radius-lg)',
    boxShadow: 'var(--shadow-lg)',
    padding: '0', margin: '0 6px',
    maxWidth: 'min(480px, 60vw)', maxHeight: '360px', overflow: 'auto',
    overscrollBehavior: 'contain',
    fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-sm)', lineHeight: '1.5',
    color: 'var(--text-primary)',
    scrollbarWidth: 'thin',
  },
  '.cm-completionInfo .cm-hc-info': { padding: '10px 12px 11px' },
  '.cm-completionInfo .cm-hc-info-sig': {
    fontFamily: 'var(--font-code)', color: 'var(--text-primary)', whiteSpace: 'pre-wrap',
  },
  '.cm-completionInfo .cm-hc-info-meta': {
    color: 'var(--text-muted)', fontSize: '0.9em', marginTop: '2px',
  },
  // The member a `generate` row will write, shown before it is accepted.
  '.cm-completionInfo .cm-hc-generate': {
    fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-xs)',
    color: 'var(--text-primary)', margin: '0', whiteSpace: 'pre',
    overflowX: 'auto',
  },
  '.cm-completionInfo .cm-hc-doc': {
    marginTop: '8px', paddingTop: '8px', borderTop: '1px solid var(--border-subtle)',
    color: 'var(--text-secondary, var(--text-primary))',
  },
  '.cm-completionInfo .cm-hc-doc code': {
    fontFamily: 'var(--font-code)', fontSize: '0.95em',
    padding: '0 3px', borderRadius: '3px',
    backgroundColor: 'var(--bg-overlay)',
  },
};
