/**
 * Character-set predicates for the matcher: the predefined classes (`\d`, `\w`, `\s`, …), the named
 * properties (`\p{Alpha}`, `\p{L}`, `\p{IsGreek}`, …) and case folding.
 *
 * Properties outside the POSIX/Java names are answered by the host's own Unicode tables through a
 * `\p{…}` RegExp, so they are exactly as current as the JS engine's data — which is the data a Java
 * runtime of the same Unicode version carries.
 */

import type { ClassKind, FlagSet } from './types';

type Pred = (cp: number) => boolean;

const between = (lo: number, hi: number): Pred => (cp) => cp >= lo && cp <= hi;
const any = (...ps: Pred[]): Pred => (cp) => ps.some((p) => p(cp));

const upper = between(65, 90);
const lower = between(97, 122);
const digit = between(48, 57);
const alpha = any(upper, lower);
const alnum = any(alpha, digit);
const punct: Pred = (cp) =>
  (cp >= 33 && cp <= 47) || (cp >= 58 && cp <= 64) || (cp >= 91 && cp <= 96) || (cp >= 123 && cp <= 126);
const graph = any(alnum, punct);
const space: Pred = (cp) => cp === 32 || (cp >= 9 && cp <= 13);

/** `\p{Alpha}` and its POSIX siblings — ASCII only, as in Java. */
const POSIX: Record<string, Pred> = {
  Lower: lower,
  Upper: upper,
  ASCII: between(0, 127),
  Alpha: alpha,
  Digit: digit,
  Alnum: alnum,
  Punct: punct,
  Graph: graph,
  Print: any(graph, (cp) => cp === 32),
  Blank: (cp) => cp === 32 || cp === 9,
  Cntrl: (cp) => cp < 32 || cp === 127,
  XDigit: any(digit, between(65, 70), between(97, 102)),
  Space: space,
};

/** Java's `\p{javaXxx}` names, mapped onto the Unicode property that backs each. */
const JAVA_PROPS: Record<string, string> = {
  javaLowerCase: 'Lowercase',
  javaUpperCase: 'Uppercase',
  javaWhitespace: 'White_Space',
  javaLetter: 'L',
  javaDigit: 'Nd',
  javaLetterOrDigit: 'L',
  javaAlphabetic: 'Alphabetic',
  javaIdeographic: 'Ideographic',
  javaTitleCase: 'Lt',
  javaMirrored: 'Bidi_Mirrored',
  javaISOControl: 'Cc',
  javaSpaceChar: 'Z',
  javaDefined: 'Assigned',
};

function unicodeTester(expr: string): Pred | null {
  try {
    const re = new RegExp(`^${expr}$`, 'u');
    return (cp) => re.test(String.fromCodePoint(cp));
  } catch {
    return null;
  }
}

const propCache = new Map<string, Pred | null>();

/** The predicate behind `\p{name}`, or `null` when the name is not a property. */
export function propertyTester(name: string): Pred | null {
  const cached = propCache.get(name);
  if (cached !== undefined) return cached;
  let pred: Pred | null = POSIX[name] ?? null;
  if (!pred && name === 'javaLetterOrDigit') {
    const l = unicodeTester('\\p{L}');
    const d = unicodeTester('\\p{Nd}');
    pred = l && d ? any(l, d) : null;
  }
  if (!pred) {
    const mapped = JAVA_PROPS[name];
    pred = unicodeTester(`\\p{${mapped ?? name}}`);
    // `IsGreek` / `script=Greek` / a bare script name: each is a spelling of a property.
    if (!pred && name.startsWith('Is')) {
      const rest = name.slice(2);
      pred = unicodeTester(`\\p{${rest}}`) ?? unicodeTester(`\\p{Script=${rest}}`);
    }
    if (!pred) pred = unicodeTester(`\\p{Script=${name}}`);
  }
  propCache.set(name, pred);
  return pred;
}

const UNICODE_WORD = unicodeTester('[\\p{L}\\p{M}\\p{Nd}\\p{Pc}]');
const UNICODE_DIGIT = unicodeTester('\\p{Nd}');
const UNICODE_SPACE = unicodeTester('\\p{White_Space}');

/** Whether a code point is a word character under the flags — the question `\w` and `\b` both ask. */
export function isWordChar(cp: number, fl: FlagSet): boolean {
  if (fl.U && UNICODE_WORD) return UNICODE_WORD(cp);
  return alnum(cp) || cp === 95;
}

const HSPACE: Pred = (cp) =>
  cp === 9 || cp === 32 || cp === 0xa0 || cp === 0x1680 || cp === 0x180e ||
  (cp >= 0x2000 && cp <= 0x200a) || cp === 0x202f || cp === 0x205f || cp === 0x3000;
const VSPACE: Pred = (cp) => (cp >= 10 && cp <= 13) || cp === 0x85 || cp === 0x2028 || cp === 0x2029;

/** The predicate for a predefined class under the flags in force. */
export function shortTester(kind: ClassKind, fl: FlagSet): Pred {
  const lowerKind = kind.toLowerCase() as 'd' | 'w' | 's' | 'h' | 'v';
  let base: Pred;
  switch (lowerKind) {
    case 'd': base = fl.U && UNICODE_DIGIT ? UNICODE_DIGIT : digit; break;
    case 'w': base = (cp) => isWordChar(cp, fl); break;
    case 's': base = fl.U && UNICODE_SPACE ? UNICODE_SPACE : space; break;
    case 'h': base = HSPACE; break;
    default: base = VSPACE;
  }
  return kind === lowerKind ? base : (cp) => !base(cp);
}

/** A line terminator, for `.`, `^` and `$`. With the unix-lines flag only `\n` is one. */
export function isLineTerminator(cp: number, fl: FlagSet): boolean {
  if (fl.d) return cp === 10;
  return cp === 10 || cp === 13 || cp === 0x85 || cp === 0x2028 || cp === 0x2029;
}

/** The canonical case of a code point for case-insensitive comparison. */
export function fold(cp: number, fl: FlagSet): number {
  if (cp < 128) return cp >= 65 && cp <= 90 ? cp + 32 : cp;
  if (!fl.u) return cp;
  const lowered = String.fromCodePoint(cp).toLowerCase();
  const first = lowered.codePointAt(0)!;
  return lowered.length === String.fromCodePoint(first).length ? first : cp;
}
