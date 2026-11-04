/**
 * The Regex Tester's quick reference — one table, read by the modal and by nothing else, so the
 * docs page and the tester cannot describe different syntax.
 */

export interface ReferenceRow { token: string; text: string }
export interface ReferenceGroup { title: string; rows: ReferenceRow[] }

export const REGEX_REFERENCE: ReferenceGroup[] = [
  {
    title: 'Characters',
    rows: [
      { token: '.', text: 'Any character except a line break (with the s flag, any at all)' },
      { token: '\\d  \\D', text: 'A digit / anything but a digit' },
      { token: '\\w  \\W', text: 'A word character [A-Za-z0-9_] / anything else' },
      { token: '\\s  \\S', text: 'Whitespace / anything but whitespace' },
      { token: '\\t \\n \\r \\f', text: 'Tab, line feed, carriage return, form feed' },
      { token: '\\xhh  \\uhhhh', text: 'A character by its hex code' },
      { token: '\\p{L}  \\P{L}', text: 'A Unicode property / its complement (\\p{Alpha}, \\p{Lu}, \\p{IsGreek} …)' },
      { token: '\\Q…\\E', text: 'Literal text, no metacharacters (Java)' },
    ],
  },
  {
    title: 'Classes',
    rows: [
      { token: '[abc]', text: 'One of a, b or c' },
      { token: '[^abc]', text: 'Anything but a, b or c' },
      { token: '[a-z0-9]', text: 'A range' },
      { token: '[a-z&&[^aeiou]]', text: 'Intersection: a consonant (Java, Rust)' },
      { token: '[\\d\\s]', text: 'Shortcuts work inside a class' },
    ],
  },
  {
    title: 'Anchors',
    rows: [
      { token: '^  $', text: 'Start / end of input (of a line with the m flag)' },
      { token: '\\b  \\B', text: 'A word boundary / not a word boundary' },
      { token: '\\A  \\z  \\Z', text: 'Start of input / very end / end before a final line break' },
      { token: '\\G', text: 'The end of the previous match (Java)' },
    ],
  },
  {
    title: 'Quantifiers',
    rows: [
      { token: '*  +  ?', text: 'Zero or more / one or more / zero or one' },
      { token: '{n}  {n,}  {n,m}', text: 'Exactly n / at least n / between n and m' },
      { token: 'X*?  X+?  X??', text: 'Lazy: as few as possible' },
      { token: 'X*+  X++  X?+', text: 'Possessive: as many as possible, never given back (Java)' },
    ],
  },
  {
    title: 'Groups',
    rows: [
      { token: '(…)', text: 'Capturing group, numbered from 1 by its opening parenthesis' },
      { token: '(?<name>…)', text: 'Named capturing group (Rust/PCRE also accept (?P<name>…))' },
      { token: '(?:…)', text: 'Non-capturing group' },
      { token: '(?>…)', text: 'Atomic group: no backtracking into it (Java)' },
      { token: '\\1  \\k<name>', text: 'Back-reference to a group' },
      { token: 'a|b', text: 'Alternation: a or b' },
    ],
  },
  {
    title: 'Look-around',
    rows: [
      { token: '(?=…)  (?!…)', text: 'Followed by / not followed by' },
      { token: '(?<=…)  (?<!…)', text: 'Preceded by / not preceded by (bounded length in Java)' },
    ],
  },
  {
    title: 'Flags',
    rows: [
      { token: 'i', text: 'Case-insensitive (ASCII; with u, Unicode too)' },
      { token: 'm', text: 'Multi-line: ^ and $ match at every line' },
      { token: 's', text: 'Dot-all: . matches line breaks' },
      { token: 'x', text: 'Free-spacing: whitespace and # comments ignored (Java, Rust)' },
      { token: 'u  U', text: 'Unicode case folding / Unicode character classes' },
      { token: '(?i)  (?i:…)  (?-i)', text: 'Inline flags, from here on / for a group / switched off (Java, Rust)' },
    ],
  },
  {
    title: 'Replacement',
    rows: [
      { token: '$1  ${name}', text: 'A group (Java, Rust). JavaScript uses $1 and $<name>' },
      { token: '\\$', text: 'A literal dollar sign (Java); $$ in Rust and JavaScript' },
      { token: '$&', text: 'The whole match (JavaScript)' },
    ],
  },
];

/** What a quick-insert of a reference token writes: its first spelling, with `…` as the gap the
 *  caret is left in (`(?:…)` → `(?:)` with the caret before the `)`). */
export function insertion(token: string): { text: string; back: number } {
  const first = token.split(/\s{2,}/)[0];
  const gap = first.indexOf('…');
  if (gap < 0) return { text: first, back: 0 };
  const text = first.replace(/…/g, '');
  return { text, back: text.length - gap };
}
