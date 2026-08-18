/**
 * Bennu i18n IPC — the fulcrum bundle under the caret.
 *
 * One verb. Everything else about i18n travels the generic framework seam (`ext.ts`): the Labels
 * catalog, the unknown-label warnings, hover and go-to on a label in a `.ron`. This is the
 * exception, and the reason is the shape of the answer rather than the subject: a parsed markup
 * tree, a stylesheet and the same label in four other languages have no representation in the
 * seam's shared vocabulary, and inventing one that only fulcrum could fill would be the wrong
 * generalisation of exactly one case.
 *
 * Offsets are **UTF-8 byte offsets**, like everything else in the bennu contract. Two frames are in
 * play and mixing them is the bug to watch for:
 *
 * - `value_start` / `value_end` / `content_start` are offsets into the **file**;
 * - every `start` / `end` inside {@link Segment}, {@link Name} and {@link MarkupProblem} is an
 *   offset into `raw` — the value's own text.
 *
 * `content_start` is what converts between them, and it is `null` exactly when that conversion is
 * not sound (see below).
 */

import { bennu } from '../rpc';

/** A name written inside a construct, with its own span — what makes "no such style" point at the
 *  one wrong word instead of the whole span. Offsets are into `raw`. */
export interface Name {
  text: string;
  start: number;
  end: number;
}

/** What a piece of a translation *is*. The discriminant matches the Rust enum's `snake_case`. */
export type SegmentKind =
  /** Literal text, escapes already resolved. */
  | { kind: 'text'; text: string }
  /** `{amount}` — interpolated by the caller at render time. */
  | { kind: 'placeholder'; name: Name }
  /** `$[ns:]a.b{…}` — styles applied left to right, each overriding only the fields it sets. */
  | { kind: 'style'; namespace: Name | null; styles: Name[]; content: Segment[] }
  /** `@[ns:]key{…}` — a glossary entry; the content is what is shown. */
  | { kind: 'glossary'; namespace: Name | null; key: Name; content: Segment[] }
  /** `~name(args){…}` — pacing or effect. What a control *means* is the engine's, not i18n's. */
  | { kind: 'control'; name: Name; args: string[]; content: Segment[] };

/** One piece of a parsed translation, and where it was written within `raw`. */
export interface Segment {
  kind: SegmentKind;
  start: number;
  end: number;
}

/** Something wrong with the markup, on the span that caused it. Offsets are into `raw`. */
export interface MarkupProblem {
  message: string;
  start: number;
  end: number;
}

/** A style `styles.toml` declares, and the four fields the preview renders it with. */
export interface StyleDecl {
  name: string;
  /** `light` · `normal` · `medium` · `bold` · `black`. */
  weight: string | null;
  /** Point size, as written. */
  size: string | null;
  /** `none` · `underline` · `line_through`. */
  decoration: string | null;
  /** Whatever the colour was written as — a hex string, a named colour, a table. */
  color: string | null;
  file: string;
  start: number;
  line: number;
}

/** A glossary entry `glossary.toml` declares. */
export interface GlossaryDecl {
  key: string;
  name: string;
  description: string;
  /** The style it renders with; the engine defaults it to `glossary-item`. */
  style: string;
  file: string;
  start: number;
  line: number;
}

/**
 * Another language's side of the same label — **whether or not it has one**.
 *
 * The rows with `declares: false` are the ones the language picker most needs: "now do the German"
 * is the reason you open it, and a list of only the languages already translated cannot express
 * that. Such a row carries the file the translation *would* go in, derived from the tree's layout,
 * so the picker has somewhere to send you even when the file does not exist yet.
 */
export interface Sibling {
  lang: string;
  /** The language's own name from `languages.toml`. Empty when it declared none. */
  name: string;
  /** Whether this language declares the label. When `false`, everything but `file` is empty. */
  declares: boolean;
  /** Whether the language is switched on. A disabled one is a place a translation may go, but
   *  nothing is owed to it. */
  enabled: boolean;
  /** The markup as written. */
  value: string;
  /** What the sentence says, constructs flattened away. */
  text: string;
  /** The placeholders it uses — what makes "this language forgot `{amount}`" visible. */
  params: string[];
  /** Where it is declared — or, when it is not, where it would be. */
  file: string;
  offset: number;
  line: number;
}

/** Everything the i18n panel draws, for one caret. */
export interface StudioView {
  /** `menu:items.new_game`. */
  label: string;
  lang: string;
  category: string;
  /** The `i18n/` directory this file belongs to. */
  root: string;
  /** The markup as the buffer has it. */
  raw: string;
  /**
   * Byte offset of `raw[0]` in the file, or `null`.
   *
   * `null` for a **basic** string carrying a backslash escape: its content is shorter than its
   * source, so an offset into it drifts past the escape. The toolbar goes read-only on one rather
   * than writing to the wrong byte — and the panel says so, because "the buttons are greyed out"
   * with no reason given is indistinguishable from broken.
   */
  content_start: number | null;
  /** The value including its quotes, in file offsets. */
  value_start: number;
  value_end: number;
  line: number;
  segments: Segment[];
  problems: MarkupProblem[];
  /** Placeholder names, in order, deduplicated. */
  params: string[];
  /** Whether the catalogue has this label at all. `false` on a key just typed, which is normal. */
  known: boolean;
  siblings: Sibling[];
  /** Enabled languages that do not declare it. */
  missing: string[];
  /** The whole stylesheet: the picker's list, and the preview's rendering. */
  styles: StyleDecl[];
  glossary: GlossaryDecl[];
  /**
   * The project's own control vocabulary, most-used first.
   *
   * There is no catalogue of controls to offer — what `~slow` does is the engine's business — so
   * this is harvested from the translations that exist. Empty on a project that uses none, and the
   * picker then says so instead of listing invented names.
   */
  controls: string[];
  has_stylesheet: boolean;
  has_glossary: boolean;
}

/**
 * What the panel draws — and, when there is nothing to draw, which link was missing.
 *
 * An empty panel used to be one bit of information, which made four different situations look
 * identical: the file is not a bundle, the project has no i18n model, the text never arrived, and the
 * caret is genuinely not on a value. Only the last is normal, and reporting the other three as the
 * last means telling somebody to put the caret on a translation while their caret is on one. Three of
 * the four cost nothing to answer, so they are answered.
 */
export interface StudioAnswer {
  /** The translation under the caret. `null` is ordinary — most lines are a header or a blank. */
  view: StudioView | null;
  /** Whether the path is `i18n/<lang>/<category>.toml`. From the path alone, so always answered. */
  bundle: boolean;
  /** Whether any open project owns this file. */
  project: boolean;
  /** The owning project's root, or `''` — what a rescan is asked for. */
  root: string;
  /**
   * Whether that project carries the fulcrum i18n model.
   *
   * `false` means no `i18n/languages.toml` was found **when the project was scanned**, and the
   * emphasis matters: capabilities are detected once and cached for the life of the project's slot,
   * so a bundle tree created after the project was opened stays invisible until something rebuilds
   * it. Which is why `root` is here — the panel offers the rescan rather than being a dead end.
   */
  model: boolean;
  /** How many translations the buffer parsed into. `0` on a file full of them is the interesting
   *  case: the text never arrived, and moving the caret will not help. */
  translations: number;
}

/**
 * The translation at `offset`, parsed, with the project around it.
 *
 * Wire: `bennu_i18n_studio`.
 */
export function i18nStudio(
  file: string,
  source: string,
  offset: number,
): Promise<StudioAnswer> {
  return bennu('bennu_i18n_studio', { args: { file, source, offset } });
}
