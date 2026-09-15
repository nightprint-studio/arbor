/**
 * Where a new line of Java code starts — the rule behind Enter, the electric `}` and `case`, and any
 * other indentation CodeMirror asks the language for.
 *
 * A pure function of the text before the line, for the reason `java-lexical` is one: it answers with
 * no parser loaded and cannot lag a keystroke behind the buffer — and it can be tested without an
 * editor. The text is read for structure only (brackets, `;`, `,`, label colons) after comments and
 * literals are blanked out, so a brace in a string or a `;` in a comment never counts.
 *
 * The rules are IntelliJ's defaults, expressed in the editor's indent unit:
 *
 * - inside `{ … }` a statement sits one level in from the line its construct starts on;
 * - a statement still being written — no `;` yet — continues two levels in from its start;
 * - inside `( … )` / `[ … ]` a line continues two levels in from the line of the opener;
 * - a braceless `if (…)` / `for (…)` / `while (…)` / `else` / `do` indents its body one level, and
 *   once that body's `;` is typed the next line is back at the outer level;
 * - after `case X:` statements sit one level in from the label (Settings → Java → Code Style decides
 *   whether a label itself is outdented, the same way Reformat does); after `case X ->` one level;
 * - a line starting with `}` / `)` / `]` sits at the level of the line that opened it.
 */

import { javaContextAt, maskJava } from './java-lexical';

/** The indentation preferences a rule is computed with — all columns. */
export interface JavaIndentStyle {
  /** One indent level. */
  unit: number;
  /** Where a tab advances to. */
  tabSize: number;
  /** Outdent `case` labels one level from the statements under them. */
  indentCaseBody: boolean;
  /**
   * The indentation of the line starting at `lineFrom`. Defaults to measuring the text; the editor
   * substitutes indentation it has decided but not yet written (re-indenting several lines at once).
   */
  lineIndent?: (lineFrom: number) => number;
}

/** Levels a continuation line is indented by — IntelliJ's 8 columns at a 4-column unit. */
export const CONTINUATION_LEVELS = 2;

/** An open bracket, or the file itself. */
interface Frame {
  open: '{' | '(' | '[' | 'root';
  /** Indentation of the line that owns the bracket: where its construct starts for `{`, the
   *  opener's own line for `(` / `[`. */
  owner: number;
  /** Offset where the statement / item being written inside this frame starts, or -1 if none. */
  segStart: number;
  /** Indentation of the `case …:` label the statements here follow, or `null`. */
  label: number | null;
  /** Commas separate items here (enum constants, array and annotation initialisers). */
  list: boolean;
}

const CLOSER_OF: Record<string, '{' | '(' | '['> = { '}': '{', ')': '(', ']': '[' };

/**
 * The column the line after `before` should start at, when that line will begin with `after`.
 *
 * `null` when the break falls inside a block comment or a literal: that is not code, and the caller
 * keeps whatever indentation the line already has.
 */
export function javaIndentFor(before: string, after: string, style: JavaIndentStyle): number | null {
  const ctx = javaContextAt(`${before}\n`, before.length);
  if (ctx.kind !== 'code' && !(ctx.kind === 'comment' && !ctx.block)) return null;

  const measure = style.lineIndent ?? ((from: number) => leadingColumns(before, from, style.tabSize));
  const indentAt = (offset: number) => measure(before.lastIndexOf('\n', offset - 1) + 1);
  const code = maskJava(before);
  const stack = scanFrames(code, indentAt);
  const top = stack[stack.length - 1];
  const head = after.trimStart();
  const { unit } = style;

  if (top.open === '(' || top.open === '[') {
    if (CLOSER_OF[head[0]] === top.open) return top.owner;
    return top.owner + CONTINUATION_LEVELS * unit;
  }

  const blockBase = top.open === 'root' ? 0 : top.owner + unit;
  if (head.startsWith('}')) return top.open === 'root' ? 0 : top.owner;
  if (isCaseLabelLine(head)) return Math.max(0, blockBase - (style.indentCaseBody ? unit : 0));

  const statementBase = top.label !== null
    ? top.label + (style.indentCaseBody ? unit : 0)
    : blockBase;
  if (top.segStart < 0) return statementBase;

  // A statement already under way is measured from the line it starts on — the level it was written
  // at — rather than from where the block says it should be.
  const segment = code.slice(top.segStart).trimEnd();
  const start = indentAt(top.segStart);
  if (isArrowLabel(segment)) return start + unit;
  if (isOnlyAnnotations(segment)) return start;

  const { headers, rest } = splitControlHeaders(segment);
  // A brace on a line of its own belongs to the header above it, not to that header's body.
  if (head.startsWith('{')) return start + Math.max(0, headers - (rest ? 0 : 1)) * unit;
  return start + headers * unit + (rest ? CONTINUATION_LEVELS * unit : 0);
}

/** What Enter does at the end of `before`, when `lineAfter` is the rest of the caret's line. */
export interface JavaLineBreak {
  /** Trailing whitespace to remove before the caret. */
  trimBefore: number;
  /** Leading whitespace to remove from the text moved to the new line. */
  trimAfter: number;
  /** The new caret line's indentation, in columns. */
  caretIndent: number;
  /** When the caret was between `{` and `}`: the indentation of the line the `}` moves to. */
  closeIndent: number | null;
}

/** The line break Enter makes in code, or `null` inside a block comment or a literal. */
export function javaLineBreak(before: string, lineAfter: string, style: JavaIndentStyle): JavaLineBreak | null {
  const ctx = javaContextAt(`${before}\n`, before.length);
  if (ctx.kind !== 'code' && !(ctx.kind === 'comment' && !ctx.block)) return null;

  const trimmed = before.replace(/[ \t]+$/, '');
  const head = lineAfter.replace(/^[ \t]+/, '');
  const explode = ctx.kind === 'code' && trimmed.endsWith('{') && head.startsWith('}');
  const caretIndent = javaIndentFor(trimmed, explode ? '' : head, style);
  if (caretIndent === null) return null;
  return {
    trimBefore: before.length - trimmed.length,
    trimAfter: lineAfter.length - head.length,
    caretIndent,
    // The caret line in between is blank, so it changes nothing about the brackets open above.
    closeIndent: explode ? javaIndentFor(`${trimmed}\n`, head, style) : null,
  };
}

// ── Structure scan ──────────────────────────────────────────────────────────────

/** Walk the masked text, keeping the stack of brackets still open at its end. */
function scanFrames(code: string, indentAt: (offset: number) => number): Frame[] {
  const stack: Frame[] = [{ open: 'root', owner: 0, segStart: -1, label: null, list: false }];
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') continue;
    const top = stack[stack.length - 1];

    if (c === '{') {
      const segText = top.segStart >= 0 ? code.slice(top.segStart, i) : '';
      endArrowLabel(top, segText);
      stack.push({
        open: '{',
        owner: indentAt(top.segStart >= 0 ? top.segStart : i),
        segStart: -1,
        label: null,
        list: opensList(top, segText),
      });
    } else if (c === '(' || c === '[') {
      if (top.segStart < 0) top.segStart = i;
      stack.push({ open: c, owner: indentAt(i), segStart: -1, label: null, list: false });
    } else if (c in CLOSER_OF) {
      closeFrame(stack, CLOSER_OF[c]);
      const parent = stack[stack.length - 1];
      // A closed block ends the statement it was part of; a closed paren is still inside one.
      if (c === '}' && isBraced(parent)) parent.segStart = -1;
    } else if (c === ';') {
      if (isBraced(top)) {
        endArrowLabel(top, top.segStart >= 0 ? code.slice(top.segStart, i) : '');
        top.list = false;
      }
      top.segStart = -1;
    } else if (c === ',' && (!isBraced(top) || top.list)) {
      top.segStart = -1;
    } else if (c === ':' && isBraced(top) && code[i + 1] !== ':' && code[i - 1] !== ':'
        && top.segStart >= 0 && endsLabel(top, code.slice(top.segStart, i), indentAt)) {
      top.segStart = -1;
    } else if (top.segStart < 0) {
      top.segStart = i;
    }
  }
  return stack;
}

function isBraced(frame: Frame): boolean {
  return frame.open === '{' || frame.open === 'root';
}

/** Pop up to and including the innermost frame `open` opened. A closer nothing opened is ignored —
 *  half-written code is exactly what this runs on. */
function closeFrame(stack: Frame[], open: '{' | '(' | '['): void {
  for (let k = stack.length - 1; k > 0; k--) {
    if (stack[k].open === open) {
      stack.length = k;
      return;
    }
  }
}

/** Whether the `:` ending `segText` ends a label; a `case` label is remembered on `frame`. */
function endsLabel(frame: Frame, segText: string, indentAt: (offset: number) => number): boolean {
  if (/^(?:case\b|default\s*$)/.test(segText) && !segText.includes('->')) {
    frame.label = indentAt(frame.segStart);
    return true;
  }
  // `outer:` — a statement label. Ends the segment, owns nothing below it.
  return /^[A-Za-z_$][\w$]*\s*$/.test(segText);
}

/** A `case X -> …` arm that has ended: the statements after it no longer belong to a label. */
function endArrowLabel(frame: Frame, segText: string): void {
  if (/^(?:case|default)\b/.test(segText) && segText.includes('->')) frame.label = null;
}

/** Whether the `{` after `segText` opens a comma-separated list rather than statements. */
function opensList(parent: Frame, segText: string): boolean {
  const text = segText.trimEnd();
  if (/\benum\b/.test(text) || /[=\]]$/.test(text)) return true;
  return text === '' && !isBraced(parent);
}

// ── Segment shapes ─────────────────────────────────────────────────────────────

/** A line that is a `case` / `default` label — the same test Reformat uses. */
function isCaseLabelLine(head: string): boolean {
  return /^(?:case\b|default\s*(?::|->))/.test(head);
}

/** `case X ->` with its body still to come. */
function isArrowLabel(segment: string): boolean {
  return /^(?:case\b|default\b)[\s\S]*->$/.test(segment);
}

/** Nothing but annotations (`@Override`, `@Named("x")`): the declaration they annotate follows at
 *  the same level. */
function isOnlyAnnotations(segment: string): boolean {
  let i = 0;
  let seen = false;
  for (;;) {
    i = skipBlanks(segment, i);
    if (i >= segment.length) return seen;
    const name = /^@[\w$.]+/.exec(segment.slice(i));
    if (!name || name[0] === '@interface') return false;
    i = skipBlanks(segment, i + name[0].length);
    if (segment[i] === '(') {
      const close = matchingParen(segment, i);
      if (close < 0) return false;
      i = close + 1;
    }
    seen = true;
  }
}

/**
 * How many braceless control headers a segment opens with, and what follows them.
 *
 * `else if (…)` is one level, as it is drawn: the `if` continues the `else` rather than nesting in it.
 */
function splitControlHeaders(segment: string): { headers: number; rest: string } {
  let i = 0;
  let headers = 0;
  for (;;) {
    i = skipBlanks(segment, i);
    const word = /^(?:if|while|for|else|do)(?![\w$])/.exec(segment.slice(i))?.[0];
    if (!word) break;
    let next = i + word.length;
    if (word !== 'else' && word !== 'do') {
      next = skipBlanks(segment, next);
      if (segment[next] !== '(') break;
      const close = matchingParen(segment, next);
      if (close < 0) break;
      next = close + 1;
    }
    if (!(word === 'else' && /^\s*if(?![\w$])/.test(segment.slice(next)))) headers++;
    i = next;
  }
  return { headers, rest: segment.slice(i).trim() };
}

function skipBlanks(text: string, i: number): number {
  while (i < text.length && /\s/.test(text[i])) i++;
  return i;
}

/** The offset of the `)` closing the `(` at `open` (in masked text), or -1. */
function matchingParen(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === '(') depth++;
    else if (text[i] === ')' && --depth === 0) return i;
  }
  return -1;
}

/** The columns of whitespace starting the line at `from`. */
function leadingColumns(text: string, from: number, tabSize: number): number {
  let col = 0;
  for (let i = from; i < text.length; i++) {
    if (text[i] === ' ') col++;
    else if (text[i] === '\t') col += tabSize - (col % tabSize);
    else break;
  }
  return col;
}
