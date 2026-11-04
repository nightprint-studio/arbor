/**
 * Converting a name between the spellings code uses: `camelCase`, `PascalCase`, `snake_case`,
 * `SCREAMING_SNAKE_CASE`, `kebab-case`, `lowercase`, `UPPERCASE` and `Title Case`.
 *
 * Pure text in, text out, so the editor command, the palette and the tests share one definition of
 * "a word". The boundaries are the ones a person reads and the spelling check already splits on:
 * a separator (`_`, `-`, a space, any punctuation), a lower→upper hump (`fooBar`), and the end of an
 * acronym (`HTTPServer` is `HTTP` + `Server`). Digits stay with the word they follow, so
 * `base64Encode` is `base64` + `Encode`, and `HTTP2Server` is `HTTP2` + `Server`.
 *
 * Unicode-aware: a letter is any `\p{L}`, so `città_vecchia` and `ÉcoleNormale` split where they should.
 */

export type CaseStyle =
  | 'camel'
  | 'pascal'
  | 'snake'
  | 'screaming'
  | 'kebab'
  | 'lower'
  | 'upper'
  | 'title';

/** Every style, in the order the palette and the menu list them. */
export const CASE_STYLES: readonly CaseStyle[] = ['camel', 'pascal', 'snake', 'screaming', 'kebab', 'lower', 'upper', 'title'];

/** The order the cycle command walks, for an identifier of more than one word. */
export const CASE_CYCLE: readonly CaseStyle[] = ['camel', 'pascal', 'snake', 'screaming', 'kebab'];

/** Every style with the label the palette and the menu show. */
export const CASE_LABELS: Record<CaseStyle, string> = {
  camel: 'camelCase',
  pascal: 'PascalCase',
  snake: 'snake_case',
  screaming: 'SCREAMING_SNAKE_CASE',
  kebab: 'kebab-case',
  lower: 'lowercase',
  upper: 'UPPERCASE',
  title: 'Title Case',
};

const WORD = /\p{Lu}+(?!\p{Ll})\p{N}*|\p{Lu}?\p{Ll}+\p{N}*|\p{N}+|\p{L}/gu;

/** `_` and `$` on either end of an identifier belong to its spelling, not to its words (`_fooBar`, `foo_`). */
const EDGES = /^([_$]*)([\s\S]*?)([_$]*)$/u;

/** The words of `text`, in order, with their own capitalisation. */
export function splitWords(text: string): string[] {
  const words: string[] = [];
  for (const chunk of text.split(/[^\p{L}\p{N}]+/u)) {
    if (chunk) words.push(...(chunk.match(WORD) ?? [chunk]));
  }
  return words;
}

const capitalise = (w: string) => (w ? w[0].toUpperCase() + w.slice(1).toLowerCase() : w);

/** `text` written in `style`. A name already in that style comes back unchanged. */
export function convertCase(text: string, style: CaseStyle): string {
  if (style === 'lower') return text.toLowerCase();
  if (style === 'upper') return text.toUpperCase();
  const [, head, core, tail] = EDGES.exec(text) ?? ['', '', text, ''];
  const words = splitWords(core);
  if (words.length === 0) return text;
  switch (style) {
    case 'camel':
      return head + words.map((w, i) => (i === 0 ? w.toLowerCase() : capitalise(w))).join('') + tail;
    case 'pascal':
      return head + words.map(capitalise).join('') + tail;
    case 'snake':
      return head + words.map((w) => w.toLowerCase()).join('_') + tail;
    case 'screaming':
      return head + words.map((w) => w.toUpperCase()).join('_') + tail;
    case 'kebab':
      return head + words.map((w) => w.toLowerCase()).join('-') + tail;
    case 'title':
      return words.map(capitalise).join(' ');
  }
}

/** Which of the identifier styles `text` is written in, or `null` when it is none of them (spaces, mixed). */
export function detectCase(text: string): CaseStyle | null {
  const core = text.replace(/^[_$]+|[_$]+$/gu, '');
  if (!core || /\s/u.test(core)) return null;
  const hasLower = /\p{Ll}/u.test(core);
  const hasUpper = /\p{Lu}/u.test(core);
  if (core.includes('-')) return hasUpper ? null : 'kebab';
  if (core.includes('_')) {
    if (hasUpper && hasLower) return null;
    return hasUpper ? 'screaming' : 'snake';
  }
  if (hasUpper && !hasLower) return 'screaming';
  if (!hasUpper) return 'camel';
  return /^\p{Lu}/u.test(core) ? 'pascal' : 'camel';
}

/**
 * The style the cycle command moves `text` to.
 *
 * An identifier of several words walks {@link CASE_CYCLE}. A single word has no hump to tell the
 * styles apart, so it walks what changes: `foo` → `Foo` → `FOO` → `foo`.
 */
export function nextCase(text: string): CaseStyle {
  const words = splitWords(text);
  const current = detectCase(text);
  if (words.length <= 1) {
    if (text === text.toUpperCase() && text !== text.toLowerCase()) return 'lower';
    if (/^\p{Lu}/u.test(text.replace(/^[_$]+/u, ''))) return 'upper';
    return 'pascal';
  }
  if (current === null) return 'camel';
  return CASE_CYCLE[(CASE_CYCLE.indexOf(current) + 1) % CASE_CYCLE.length];
}

/** `text` moved one step round the cycle. */
export function cycleCase(text: string): string {
  return convertCase(text, nextCase(text));
}
