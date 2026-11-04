/**
 * The regex model shared by everything that reads a pattern: the parser's AST, the diagnostics, the
 * highlighter, the explainer and the matcher. One tree, so a pattern is parsed once per consumer and
 * every consumer agrees on where each piece starts and ends.
 *
 * Offsets (`from`/`to`) are UTF-16 offsets into the pattern text — the same frame CodeMirror and a
 * JS string use, so they map straight onto the editor once the string literal's escapes are undone.
 */

/** The dialect a pattern is read in. `rust` also stands for PCRE-ish engines: it is the strictest of
 *  the three about what it accepts (no look-around, no back-references), which is what a portable
 *  pattern has to live within. */
export type Flavour = 'java' | 'js' | 'rust';

export const FLAVOUR_LABELS: Record<Flavour, string> = {
  java: 'Java',
  js: 'JavaScript',
  rust: 'Rust / PCRE',
};

/** The flags in force at a point of the pattern. Letters follow Java's inline-flag alphabet; the
 *  JS/Rust equivalents are mapped onto it when a flag string is read. */
export interface FlagSet {
  /** Case-insensitive. */
  i: boolean;
  /** Multi-line `^` / `$`. */
  m: boolean;
  /** Dot matches line terminators. */
  s: boolean;
  /** Free-spacing mode: whitespace and `#` comments in the pattern are ignored. */
  x: boolean;
  /** Unicode-aware case folding. */
  u: boolean;
  /** Unicode character classes (`\w`, `\d`, `\b` become Unicode-aware). */
  U: boolean;
  /** Unix lines: only `\n` terminates a line. */
  d: boolean;
}

export const NO_FLAGS: FlagSet = { i: false, m: false, s: false, x: false, u: false, U: false, d: false };

/** A span of the pattern. */
export interface Span { from: number; to: number }

export type ClassKind = 'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'h' | 'H' | 'v' | 'V';

/** One member of a character class. */
export type ClassItem =
  | (Span & { k: 'char'; cp: number })
  | (Span & { k: 'range'; lo: number; hi: number })
  | (Span & { k: 'short'; kind: ClassKind })
  | (Span & { k: 'prop'; name: string; negate: boolean })
  | (Span & { k: 'nested'; cls: ClassNode });

export interface ClassNode extends Span {
  t: 'class';
  negate: boolean;
  /** `[a-z&&[^aeiou]]` is two operands intersected; every plain class has one. */
  operands: ClassItem[][];
  fl: FlagSet;
}

export type GroupKind =
  | 'capture' | 'named' | 'noncapture' | 'atomic'
  | 'lookahead' | 'neglookahead' | 'lookbehind' | 'neglookbehind'
  /** `(?i:…)` — flags applied to a body. `(?i)` alone is a {@link FlagsNode}. */
  | 'flagged';

export type AnchorKind = '^' | '$' | 'A' | 'Z' | 'z' | 'b' | 'B' | 'G';

export type RegexNode =
  | (Span & { t: 'lit'; cp: number; raw: string; fl: FlagSet })
  | (Span & { t: 'quote'; text: string; fl: FlagSet })
  | (Span & { t: 'any'; fl: FlagSet })
  | ClassNode
  | (Span & { t: 'short'; kind: ClassKind; fl: FlagSet })
  | (Span & { t: 'prop'; name: string; negate: boolean; fl: FlagSet })
  | (Span & { t: 'anchor'; kind: AnchorKind; fl: FlagSet })
  | (Span & { t: 'linebreak' })
  | (Span & { t: 'backref'; index: number | null; name: string | null; fl: FlagSet })
  | (Span & { t: 'flags'; on: string; off: string })
  | (Span & { t: 'comment' })
  | GroupNode
  | QuantNode
  | AltNode
  | SeqNode;

export interface GroupNode extends Span {
  t: 'group';
  kind: GroupKind;
  /** Capture-group number, for `capture` and `named`. */
  index: number | null;
  name: string | null;
  /** The opener's span — `(`, `(?:`, `(?<name>` — so the highlighter can colour it apart from the body. */
  open: Span;
  /** The closer's span, or null when the group is unclosed. */
  close: Span | null;
  body: RegexNode;
  /** For `flagged`: the flags switched on and off. */
  on?: string;
  off?: string;
}

export interface QuantNode extends Span {
  t: 'quant';
  body: RegexNode;
  min: number;
  /** `Infinity` for an open upper bound. */
  max: number;
  mode: 'greedy' | 'lazy' | 'possessive';
  /** The quantifier token itself (`*`, `{2,5}?`), apart from the body it applies to. */
  token: Span;
}

export interface AltNode extends Span {
  t: 'alt';
  branches: RegexNode[];
  /** Offsets of the `|` separators. */
  pipes: number[];
}

export interface SeqNode extends Span {
  t: 'seq';
  items: RegexNode[];
}

export type FlagsNode = Extract<RegexNode, { t: 'flags' }>;

export interface RegexDiagnostic extends Span {
  message: string;
}

export interface ParsedRegex {
  root: RegexNode;
  /** The number of capturing groups. */
  groupCount: number;
  /** Name per group index (`null` for an unnamed one). Index 0 is the whole match. */
  groupNames: (string | null)[];
  errors: RegexDiagnostic[];
  flavour: Flavour;
}

/** One group of one match. */
export interface GroupMatch { start: number; end: number; text: string }

export interface RegexMatch {
  start: number;
  end: number;
  /** Index 0 is the whole match; a group that did not take part is `null`. */
  groups: (GroupMatch | null)[];
}

export type MatchMode = 'find' | 'matches' | 'lookingAt';

export interface RunRequest {
  flavour: Flavour;
  pattern: string;
  /** Flag letters of the flavour: Java `imsxuUd`, JS `gimsuyd`, Rust `imsxuU`. */
  flags: string;
  text: string;
  mode: MatchMode;
  /** Replacement template, or `null` for no substitution preview. */
  replacement: string | null;
  /** Cap on matches returned, so a pattern that matches everywhere cannot flood the UI. */
  maxMatches: number;
}

export interface RunResult {
  ok: boolean;
  /** A pattern the engine rejected — the message is the engine's own. */
  error: string | null;
  matches: RegexMatch[];
  groupNames: (string | null)[];
  replaced: string | null;
  replaceError: string | null;
  split: string[] | null;
  /** The engine gave up (step budget) — results so far are partial. */
  aborted: boolean;
  truncated: boolean;
  ms: number;
}
