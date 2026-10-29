/**
 * What the assistant mark in the editor's margin offers, and what each choice asks.
 *
 * The mark sits on the line the caret is on — or where a selection starts — and what it offers depends on
 * what is there: a selection, a problem the editor has already found, a declaration, a plain line. So the
 * menu is short and relevant instead of long and general, and nobody types a class and a method name into a
 * chat: the request carries the file, the lines and the code itself.
 *
 * Pure — the context goes in, items and prompts come out — so what is offered where is tested.
 */

import type { DocLanguage } from './doc-comments';

/** The 1-based line a UTF-16 offset falls on. */
export function lineOfOffset(text: string, offset: number): number {
  let line = 1;
  const end = Math.min(offset, text.length);
  for (let i = 0; i < end; i++) if (text.charCodeAt(i) === 10) line++;
  return line;
}

/** The 1-based line a UTF-8 byte offset falls on — the frame the editor's diagnostics are in. */
export function lineOfByte(text: string, byte: number): number {
  let line = 1;
  let seen = 0;
  for (let i = 0; i < text.length && seen < byte; i++) {
    const c = text.charCodeAt(i);
    if (c === 10) line++;
    // The bytes this UTF-16 unit takes in UTF-8; a surrogate pair is four bytes across its two units.
    seen += c < 0x80 ? 1 : c < 0x800 ? 2 : c >= 0xd800 && c <= 0xdfff ? 2 : 3;
  }
  return line;
}

export interface ProblemOnLine {
  message: string;
  severity: 'error' | 'warning' | 'info' | string;
}

/** Everything the menu is chosen from. */
export interface ActionContext {
  /** The file's path as the assistant should see it (relative to the project when it can be). */
  file: string;
  language: DocLanguage | null;
  /** 1-based first and last line of what is being asked about. */
  fromLine: number;
  toLine: number;
  /** The selected text, when there is a selection. */
  selection: string | null;
  /** The text of the line, when nothing is selected. */
  lineText: string;
  /** Problems the editor has found on these lines. */
  problems: ProblemOnLine[];
  /** The declaration on the line (a method, a class), when it is one. */
  declaration: string | null;
  /** The declaration has no doc comment. */
  undocumented: boolean;
}

export type ActionId =
  | 'fix' | 'explain-problem'
  | 'explain' | 'bugs' | 'improve' | 'tests'
  | 'usages' | 'document'
  | 'ask';

export interface AssistantAction {
  id: ActionId;
  label: string;
}

/** The longest piece of code put into a request. A selection past this is cut, with a note. */
const MAX_CODE_CHARS = 6000;

export function actionsFor(c: ActionContext): AssistantAction[] {
  const out: AssistantAction[] = [];
  const errors = c.problems.filter((p) => p.severity === 'error');
  if (c.problems.length) {
    out.push({ id: 'fix', label: errors.length ? 'Fix this error' : 'Fix this warning' });
    out.push({ id: 'explain-problem', label: 'Explain this problem' });
  }
  if (c.selection) {
    out.push({ id: 'explain', label: 'Explain the selection' });
    out.push({ id: 'bugs', label: 'Look for bugs' });
    out.push({ id: 'improve', label: 'Suggest improvements' });
    out.push({ id: 'tests', label: 'Write tests for it' });
  } else if (c.declaration) {
    out.push({ id: 'explain', label: 'Explain this' });
    out.push({ id: 'tests', label: 'Write tests for it' });
    out.push({ id: 'usages', label: 'Where is it used?' });
    if (c.undocumented) out.push({ id: 'document', label: 'Write documentation' });
  } else if (!c.problems.length && c.lineText.trim()) {
    out.push({ id: 'explain', label: 'Explain this line' });
  }
  out.push({ id: 'ask', label: 'Ask about this…' });
  return out;
}

function where(c: ActionContext): string {
  const lines = c.fromLine === c.toLine ? `line ${c.fromLine}` : `lines ${c.fromLine}–${c.toLine}`;
  return `File: ${c.file}\nWhere: ${lines}`;
}

/** The code the question is about, fenced, cut when it is long. */
function code(c: ActionContext): string {
  const text = c.selection ?? c.lineText;
  if (!text.trim()) return '';
  const lang = c.language ?? '';
  const body = text.length > MAX_CODE_CHARS ? `${text.slice(0, MAX_CODE_CHARS)}\n… (cut: ${text.length - MAX_CODE_CHARS} more characters)` : text;
  return `\n\n\`\`\`${lang}\n${body}\n\`\`\``;
}

function problemList(c: ActionContext): string {
  return c.problems.map((p) => `- ${p.severity}: ${p.message}`).join('\n');
}

/** The message sent for `action`. `ask` is not here: it opens the composer instead of sending. */
export function actionPrompt(action: Exclude<ActionId, 'ask'>, c: ActionContext): string {
  const head = where(c);
  switch (action) {
    case 'fix':
      return `Fix this problem the editor reports.\n\n${head}\n${problemList(c)}${code(c)}\n\nRead the code around it first, find the cause, and fix it with edit_file. Change only what the problem needs.`;
    case 'explain-problem':
      return `Explain this problem: what it means, what causes it here, and how it would be fixed. Do not change anything.\n\n${head}\n${problemList(c)}${code(c)}`;
    case 'explain':
      return `Explain what this does and why it is written this way. Read what it calls if that helps. Do not change anything.\n\n${head}${code(c)}`;
    case 'bugs':
      return `Look for bugs in this code: wrong conditions, null or empty cases, exceptions swallowed or leaked, resources not closed, concurrency problems. Say what you find and where; do not change anything.\n\n${head}${code(c)}`;
    case 'improve':
      return `Suggest improvements to this code — clarity, correctness, simplicity — and say why each is worth doing. Do not change anything yet.\n\n${head}${code(c)}`;
    case 'tests':
      return `Write tests for this. Look at the tests already in the project first and follow their conventions (framework, naming, where they live). Create or extend the test file, then check it compiles.\n\n${head}${code(c)}`;
    case 'usages':
      return `Where is this used? Use the project's index to find the callers, group them by what they do, and say which would be affected by changing its behaviour.\n\n${head}${code(c)}`;
    case 'document':
      return `Write the documentation comment this declaration is missing, following the file's language and tone. Change only the comment, with edit_file.\n\n${head}${code(c)}`;
  }
}

/** The start of a message for "Ask about this…": what it is about, ready for the user to finish. */
export function askDraft(c: ActionContext): string {
  const lines = c.fromLine === c.toLine ? `line ${c.fromLine}` : `lines ${c.fromLine}–${c.toLine}`;
  return `About ${c.file}, ${lines}: `;
}
