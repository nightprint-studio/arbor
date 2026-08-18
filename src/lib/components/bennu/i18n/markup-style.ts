/**
 * `styles.toml` → CSS, for the preview.
 *
 * ## What a preview owes and what it does not
 *
 * The engine draws the real thing, with the project's fonts, at the project's sizes, on the
 * project's background. This cannot and should not try to be that: a faithful absolute rendering
 * inside a 200px panel would be a 48-point heading pushing everything else off screen, and a
 * preview that has to be scrolled to be read has stopped answering the question it was opened for.
 *
 * What it owes is the **distinctions**: that `$warning` is not the same colour as `$hint`, that
 * `$title` is bigger than the text around it, that `$dim.italic` is both. Those are relative facts,
 * they are what a translator or a designer is checking, and they survive being scaled to fit.
 *
 * So colour and weight and decoration are honoured literally, and **size is relative** — see
 * {@link SIZE_FLOOR}.
 *
 * ## Tolerant on purpose
 *
 * Every field is a string as somebody wrote it, in a file no schema guards. An unrecognised value
 * yields *nothing* for that field rather than a guess: a style whose colour cannot be read still
 * previews with its weight, and the text stays legible. Painting `color: undefined-ish` or falling
 * back to red would invent a distinction the project does not have.
 */

import type { StyleDecl } from '$lib/ipc/bennu/i18n';

/** The resolved appearance of a span — only the fields something actually set. */
export interface Appearance {
  color?: string;
  fontWeight?: number;
  /** In `em`, relative to the panel's text — see {@link SIZE_FLOOR}. */
  fontSizeEm?: number;
  textDecorationLine?: string;
}

/**
 * The smallest declared size in the stylesheet renders at 1em, and everything else in proportion,
 * clamped to {@link SIZE_MAX_EM}.
 *
 * Which is the only way a *relative* impression can be given without knowing the engine's base
 * size — that number is not in `styles.toml` and is not ours to assume. It also degrades correctly:
 * a stylesheet whose styles are all one size previews them all at 1em, which is exactly right,
 * because in that stylesheet size is not a distinction anybody is drawing.
 */
const SIZE_FLOOR = 1;
/** Past this a heading stops being "bigger" and starts being a layout problem. */
const SIZE_MAX_EM = 2.1;

const WEIGHTS: Record<string, number> = {
  light: 300,
  normal: 400,
  medium: 500,
  bold: 700,
  black: 900,
};

const DECORATIONS: Record<string, string> = {
  none: 'none',
  underline: 'underline',
  line_through: 'line-through',
};

/**
 * A stylesheet, resolved once so the preview can look a style up by name.
 *
 * Built per view rather than per span: the size scale depends on *every* declared size, so the
 * sheet has to be seen whole before any one style can be rendered.
 */
export class StyleSheet {
  private readonly byName = new Map<string, Appearance>();

  constructor(styles: readonly StyleDecl[]) {
    const sizes = styles.map((s) => parseSize(s.size)).filter((n): n is number => n !== null);
    // The smallest declared size is the baseline; with none declared, nothing scales.
    const base = sizes.length ? Math.min(...sizes) : 0;
    for (const s of styles) this.byName.set(s.name, appearanceOf(s, base));
  }

  /** The appearance of one name, or `undefined` when the stylesheet does not declare it. */
  get(name: string): Appearance | undefined {
    return this.byName.get(name);
  }

  has(name: string): boolean {
    return this.byName.has(name);
  }

  /**
   * A `$a.b.c{…}` chain, merged left to right over `inherited`.
   *
   * Each style overrides **only the fields it sets** — the engine's rule, and the reason
   * `$red.bold` works at all — so `$bold` inside `$red{…}` is bold *and* red. A name the sheet does
   * not have contributes nothing and does not interrupt the chain: it is already reported as a
   * problem, and dropping the rest of the chain over it would make one typo look like several.
   */
  chain(names: readonly string[], inherited: Appearance = {}): Appearance {
    let out = inherited;
    for (const n of names) {
      const next = this.byName.get(n);
      if (next) out = { ...out, ...next };
    }
    return out;
  }
}

/** One style's fields as CSS, sizes relative to `base` (0 = no scaling). */
function appearanceOf(style: StyleDecl, base: number): Appearance {
  const out: Appearance = {};

  const color = parseColor(style.color);
  if (color) out.color = color;

  const weight = style.weight?.trim().toLowerCase();
  if (weight) {
    if (WEIGHTS[weight] !== undefined) out.fontWeight = WEIGHTS[weight];
    // A numeric weight is legal CSS and plausibly what somebody wrote; anything else is dropped.
    else if (/^[1-9]00$/.test(weight)) out.fontWeight = Number(weight);
  }

  const size = parseSize(style.size);
  if (size !== null && base > 0) {
    out.fontSizeEm = Math.min(SIZE_MAX_EM, Math.max(SIZE_FLOOR, size / base));
  }

  const decoration = style.decoration?.trim().toLowerCase();
  if (decoration && DECORATIONS[decoration]) out.textDecorationLine = DECORATIONS[decoration];

  return out;
}

/** A point size as a number, or `null` when it is not one. */
function parseSize(raw: string | null): number | null {
  if (!raw) return null;
  const n = Number(raw.trim());
  return Number.isFinite(n) && n > 0 ? n : null;
}

/**
 * Whatever the colour was written as, as something CSS accepts — or `null`.
 *
 * Three forms are real, because the catalogue hands the value over exactly as the file has it:
 * a hex string, a colour name, and an inline table of channels. The table is the one that needs
 * work, and its channels may be floats (`0.0`–`1.0`, how a renderer thinks) or bytes (`0`–`255`,
 * how a designer thinks). They are told apart the only way they can be: a channel set containing
 * anything above 1 is bytes.
 */
export function parseColor(raw: string | null): string | null {
  if (!raw) return null;
  const v = raw.trim();
  if (!v) return null;

  if (/^#[0-9a-f]{3,8}$/i.test(v)) return v;
  // A bare word: a CSS colour keyword, most likely, and CSS ignores what it does not know — which
  // is the failure we want, since the alternative is inventing a colour.
  if (/^[a-z]+$/i.test(v)) return v.toLowerCase();
  // Already a function call somebody wrote out.
  if (/^(rgb|rgba|hsl|hsla)\(/i.test(v)) return v;

  const table = parseChannels(v);
  if (table) return table;

  return null;
}

/** `{ r = 1.0, g = 0.5, b = 0.2, a = 0.8 }` → `rgb(…)`. `null` when it is not that. */
function parseChannels(v: string): string | null {
  if (!(v.startsWith('{') && v.endsWith('}'))) return null;
  const channels: Record<string, number> = {};
  for (const part of v.slice(1, -1).split(',')) {
    const [key, value] = part.split('=');
    if (!key || value === undefined) continue;
    const n = Number(value.trim());
    if (Number.isFinite(n)) channels[key.trim().toLowerCase()] = n;
  }
  const { r, g, b, a } = channels;
  if (r === undefined || g === undefined || b === undefined) return null;

  const bytes = [r, g, b].some((c) => c > 1);
  const to255 = (c: number) => Math.max(0, Math.min(255, Math.round(bytes ? c : c * 255)));
  const rgb = `${to255(r)} ${to255(g)} ${to255(b)}`;
  // Alpha is a fraction in both conventions; a table written in bytes still means 0–1 here, unless
  // it plainly does not.
  if (a === undefined || a >= 1) return `rgb(${rgb})`;
  return `rgb(${rgb} / ${Math.max(0, a)})`;
}

/** An {@link Appearance} as an inline `style` attribute — empty string when it sets nothing. */
export function styleAttr(a: Appearance): string {
  const parts: string[] = [];
  if (a.color) parts.push(`color:${a.color}`);
  if (a.fontWeight) parts.push(`font-weight:${a.fontWeight}`);
  if (a.fontSizeEm) parts.push(`font-size:${a.fontSizeEm}em`);
  if (a.textDecorationLine) parts.push(`text-decoration-line:${a.textDecorationLine}`);
  return parts.join(';');
}
