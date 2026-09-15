/**
 * The kind icons of the completion popup: a small SVG set in the spirit of IntelliJ's New UI.
 *
 * ## Design
 *
 * One 16×16 glyph per kind: a tinted body with a letter in it, coloured by kind. Two things carry
 * the kind, so neither has to carry it alone:
 *
 * - **Shape.** A *member* (method, field, variable, parameter) is a circle, a *type* (class,
 *   interface, enum, record, annotation) is a rounded square, a *package* is a folder. In the dark
 *   theme `--accent` and `--info` are close, and the shape is what tells a method from a class.
 * - **Colour.** Grouped rather than numbered: callables use the accent, stored values the warning
 *   amber, classes and records the info blue, interfaces green, and grammar (keywords, plain words)
 *   is muted so it recedes behind the symbols. Only theme tokens, so it follows light and dark.
 *
 * On top of the body, the modifiers a reader scans for:
 *
 * - **static**: a solid dot in the top-right corner;
 * - **final**: a small lock in the bottom-right corner;
 * - **abstract**: the body outlined with a dashed stroke and no fill, since nothing is there yet;
 * - **deprecated**: a diagonal stroke across the whole icon.
 *
 * A constant is a field that is static and final, and draws as one: the same `f` with both marks.
 *
 * ## Why markup strings
 *
 * The icons are built from a fixed table and no provider text reaches them, so a string is safe.
 * It is also what makes them testable without a DOM: the editor turns the string into nodes, and
 * the tests read the string. Colour comes from CSS (`currentColor` plus the classes styled in
 * `completion-theme`), so the markup has no colour in it.
 */

/** The outline an icon's body is drawn with. */
export type IconShape = 'circle' | 'square' | 'folder';

/** The modifiers the icon can mark. */
export type IconModifier = 'static' | 'abstract' | 'final';

/** How one completion `type` is drawn. */
export interface CompletionIconKind {
  /** The CodeMirror completion `type` this entry draws. */
  type: string;
  /** The letter inside the body. Empty for a shape that speaks for itself, like the folder. */
  glyph: string;
  /** A theme colour expression, applied as the icon's `color`. */
  color: string;
  shape: IconShape;
  /** The glyph's size in icon units, when the default 9 does not fit (a two-character glyph). */
  glyphSize?: number;
  /** Modifiers the kind always has, marked even when the provider does not say so. */
  implied?: readonly IconModifier[];
}

/** A violet made from two theme tokens, so annotations get their own hue without a hard-coded colour. */
const ANNOTATION_HUE = 'color-mix(in srgb, var(--accent) 50%, var(--error))';

/**
 * Every completion type a product emits: CodeMirror's standard set, the finer kinds a language
 * server can tell apart (interface, enum, record, parameter, constructor), and Bennu's
 * `annotation`, `template` and `generate`.
 */
export const COMPLETION_ICON_KINDS: readonly CompletionIconKind[] = [
  { type: 'method',      glyph: 'm',  color: 'var(--accent)',         shape: 'circle' },
  { type: 'function',    glyph: 'm',  color: 'var(--accent)',         shape: 'circle' },
  { type: 'constructor', glyph: 'c',  color: 'var(--accent)',         shape: 'circle' },
  { type: 'property',    glyph: 'f',  color: 'var(--warning)',        shape: 'circle' },
  { type: 'constant',    glyph: 'f',  color: 'var(--warning)',        shape: 'circle', implied: ['static', 'final'] },
  { type: 'variable',    glyph: 'v',  color: 'var(--text-secondary)', shape: 'circle' },
  { type: 'parameter',   glyph: 'p',  color: 'var(--text-secondary)', shape: 'circle' },
  { type: 'class',       glyph: 'c',  color: 'var(--info)',           shape: 'square' },
  { type: 'interface',   glyph: 'i',  color: 'var(--success)',        shape: 'square' },
  { type: 'enum',        glyph: 'e',  color: 'var(--warning)',        shape: 'square' },
  { type: 'record',      glyph: 'r',  color: 'var(--info)',           shape: 'square' },
  { type: 'type',        glyph: 'T',  color: 'var(--info)',           shape: 'square' },
  { type: 'annotation',  glyph: '@',  color: ANNOTATION_HUE,          shape: 'square' },
  { type: 'namespace',   glyph: '',   color: 'var(--text-secondary)', shape: 'folder' },
  { type: 'keyword',     glyph: 'k',  color: 'var(--text-muted)',     shape: 'square' },
  { type: 'template',    glyph: '{}', color: 'var(--text-secondary)', shape: 'square', glyphSize: 7 },
  { type: 'generate',    glyph: '+',  color: 'var(--success)',        shape: 'square', glyphSize: 11 },
  { type: 'text',        glyph: '≡',  color: 'var(--text-muted)',     shape: 'square' },
];

const KIND_BY_TYPE: ReadonlyMap<string, CompletionIconKind> = new globalThis.Map(
  COMPLETION_ICON_KINDS.map((k) => [k.type, k] as const),
);

/** What a type no product declared is drawn as. An empty slot would read as a broken row. */
const FALLBACK_KIND = KIND_BY_TYPE.get('text')!;

/** One icon, resolved: the kind and what to draw on top of it. */
export interface CompletionIcon {
  kind: CompletionIconKind;
  /** The corner marks, in drawing order. */
  marks: ('static' | 'final')[];
  /** Drawn outlined and dashed. */
  abstract: boolean;
  /** Drawn struck through. */
  deprecated: boolean;
}

/** Resolve the icon for a completion `type`, its modifiers and its deprecated flag. */
export function describeCompletionIcon(
  type: string | undefined,
  modifiers: readonly string[] = [],
  deprecated = false,
): CompletionIcon {
  const kind = (type && KIND_BY_TYPE.get(type)) || FALLBACK_KIND;
  const has = new globalThis.Set<string>([...(kind.implied ?? []), ...modifiers]);
  return {
    kind,
    marks: (['static', 'final'] as const).filter((m) => has.has(m)),
    abstract: has.has('abstract'),
    deprecated,
  };
}

/** The body outline per shape, as the opening of an SVG element (the caller closes it). */
const BODY_OPEN: Record<IconShape, string> = {
  circle: '<circle cx="8" cy="8" r="6.4"',
  square: '<rect x="1.6" y="1.6" width="12.8" height="12.8" rx="3.4"',
  folder: '<path d="M1.6 4.4c0-1 .8-1.8 1.8-1.8h3l1.6 1.6h4.6c1 0 1.8.8 1.8 1.8v6.2c0 1-.8 1.8-1.8 1.8H3.4c-1 0-1.8-.8-1.8-1.8z"',
};

const MARK_MARKUP: Record<'static' | 'final', string> = {
  static: '<circle class="cm-ki-mark" cx="13" cy="3" r="2.6"/>',
  final:
    '<path class="cm-ki-shackle" d="M11.3 11.2V9.9a1.45 1.45 0 0 1 2.9 0v1.3"/>' +
    '<rect class="cm-ki-mark" x="10.1" y="11" width="5.3" height="4.4" rx="1"/>',
};

const XML_ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };

/** The icon as SVG markup. Colourless: the theme styles its classes. */
export function completionIconMarkup(icon: CompletionIcon): string {
  const { kind } = icon;
  const bodyClass = icon.abstract ? 'cm-ki-body cm-ki-abstract' : 'cm-ki-body';
  const body = `${BODY_OPEN[kind.shape]} class="${bodyClass}"/>`;
  const glyph = kind.glyph
    ? `<text class="cm-ki-glyph" x="8" y="8.5" font-size="${kind.glyphSize ?? 9}" ` +
      `text-anchor="middle" dominant-baseline="central">` +
      `${kind.glyph.replace(/[&<>]/g, (c) => XML_ESCAPES[c])}</text>`
    : '';
  const marks = icon.marks.map((m) => MARK_MARKUP[m]).join('');
  const strike = icon.deprecated ? '<line class="cm-ki-strike" x1="2.4" y1="13.6" x2="13.6" y2="2.4"/>' : '';
  return (
    '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">' +
    `${body}${glyph}${marks}${strike}</svg>`
  );
}
