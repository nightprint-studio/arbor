/**
 * The look of the completion popup and of the documentation panel beside it.
 *
 * ## Design
 *
 * IntelliJ's New UI popup, read as four columns on a comfortable row:
 *
 *   [kind badge]  label (typed part in accent)   signature / type ………   Origin
 *
 * - **Kind badge**: a small tinted chip with one letter (`m`, `f`, `c`, …), coloured by kind.
 *   It is the only coloured thing on the row, so the list does not turn into a rainbow. It uses
 *   the theme's status tokens rather than syntax colours, because those are defined for every
 *   theme. **Members** (method, field, constant, variable) are round and **declarations** (class,
 *   interface, enum, type, package, keyword, …) are rounded squares. In the dark theme `--accent`
 *   and `--info` are close, so the shape is what separates a method from a class at a glance.
 * - **Label**: in the code font at the editor's size. The typed characters are drawn in the accent
 *   colour and bold, not underlined. A deprecated candidate is struck through and dimmed.
 * - **Detail**: the signature or type fills the middle of the row. It is right-aligned and muted,
 *   and is the first thing to truncate. **Origin** (where a member is declared) is quieter still and
 *   sits on the right edge.
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

/** The popup container. */
const POPUP = '.cm-tooltip.cm-tooltip-autocomplete';
/** One candidate row. The list is `ul[role=listbox]`, and CodeMirror sets that role itself. */
const ROW = `${POPUP} > ul > li`;

/** A row's height, in `em` of the list font, so it follows the user's font scale. At the default
 *  13px it comes to about 24px. */
const ROW_EM = 1.85;
/** How many rows the popup shows before it scrolls. */
const VISIBLE_ROWS = 13;

type BadgeShape = 'member' | 'declaration';

interface CompletionKindStyle {
  /** The CodeMirror completion `type`: `cm-completionIcon-<type>`. */
  type: string;
  /** The glyph inside the badge. */
  glyph: string;
  /** A theme colour expression. It tints the badge only, never the label. */
  color: string;
  shape: BadgeShape;
}

/** A violet made from two theme tokens, so it follows the theme instead of being hard-coded. It
 *  gives annotations their own hue without adding a colour the palette does not have. */
const ANNOTATION_HUE = 'color-mix(in srgb, var(--accent) 50%, var(--error))';

/**
 * One row per completion type any product emits: CodeMirror's standard set, plus Bennu's
 * `annotation` and `generate`.
 *
 * The colours group kinds instead of numbering them. Callables use the accent, stored values the
 * warning amber, types the info blue, and a row that writes new code uses success green. Grammar
 * (keywords, plain words) is muted so it recedes behind the symbols.
 */
export const COMPLETION_KINDS: readonly CompletionKindStyle[] = [
  { type: 'method',     glyph: 'm',  color: 'var(--accent)',         shape: 'member' },
  { type: 'function',   glyph: 'm',  color: 'var(--accent)',         shape: 'member' },
  { type: 'property',   glyph: 'f',  color: 'var(--warning)',        shape: 'member' },
  { type: 'constant',   glyph: '#',  color: 'var(--warning)',        shape: 'member' },
  { type: 'variable',   glyph: 'v',  color: 'var(--text-secondary)', shape: 'member' },
  { type: 'class',      glyph: 'c',  color: 'var(--info)',           shape: 'declaration' },
  { type: 'interface',  glyph: 'i',  color: 'var(--info)',           shape: 'declaration' },
  { type: 'enum',       glyph: 'e',  color: 'var(--info)',           shape: 'declaration' },
  { type: 'type',       glyph: 't',  color: 'var(--info)',           shape: 'declaration' },
  { type: 'namespace',  glyph: 'p',  color: 'var(--text-secondary)', shape: 'declaration' },
  { type: 'annotation', glyph: '@',  color: ANNOTATION_HUE,          shape: 'declaration' },
  { type: 'generate',   glyph: '+',  color: 'var(--success)',        shape: 'declaration' },
  { type: 'keyword',    glyph: 'k',  color: 'var(--text-muted)',     shape: 'declaration' },
  { type: 'text',       glyph: '≡',  color: 'var(--text-muted)',     shape: 'declaration' },
];

const SHAPE_RADIUS: Record<BadgeShape, string> = {
  member: '50%',
  declaration: 'var(--radius-sm)',
};

type StyleRule = Record<string, string>;

/** The per-kind rules. Each kind sets `--cm-kind` and the badge glyph, and the one generic badge
 *  rule turns that variable into its colour and tint. They are generated from
 *  {@link COMPLETION_KINDS} so a kind's letter and colour cannot drift apart. */
const kindRules: Record<string, StyleRule> = Object.fromEntries(
  COMPLETION_KINDS.flatMap(({ type, glyph, color, shape }) => [
    [`${POPUP} .cm-completionIcon-${type}`, { '--cm-kind': color, borderRadius: SHAPE_RADIUS[shape] }],
    [`${POPUP} .cm-completionIcon-${type}::after`, { content: `'${glyph}'` }],
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

  // ── Kind badge ──
  [`${POPUP} .cm-completionIcon`]: {
    '--cm-kind': 'var(--text-muted)',
    flex: '0 0 auto',
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
    boxSizing: 'border-box',
    // The font size is set first, and the `em` sizes below are relative to it: a ~16px badge.
    fontSize: '0.72em',
    width: '1.75em', height: '1.75em',
    padding: '0', opacity: '1',
    fontFamily: 'var(--font-code)', fontWeight: '700', lineHeight: '1',
    borderRadius: 'var(--radius-sm)',
    color: 'var(--cm-kind)',
    backgroundColor: 'color-mix(in srgb, var(--cm-kind) 18%, transparent)',
    boxShadow: 'inset 0 0 0 1px color-mix(in srgb, var(--cm-kind) 32%, transparent)',
  },
  // A type no product declared still gets a badge. An empty chip reads as a broken row.
  [`${POPUP} .cm-completionIcon::after`]: { content: "'·'" },
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
  [`${ROW}.cm-completion-deprecated .cm-completionIcon`]: { opacity: '0.55' },

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
