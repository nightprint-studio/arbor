/**
 * What the comment button offers, and the request each choice turns into.
 *
 * The button never asks the user to name a class and a method: the editor knows where they are, so the
 * request carries the file, the line and the declaration, and says what is wanted. The assistant does the
 * rest — reads the code, writes or fixes the comment, and proposes the edit like any other, so it is
 * reviewed in the same card.
 */

import type { DocLanguage, DocTarget } from './doc-comments';

export type DocActionId = 'write' | 'improve' | 'review' | 'shorten' | 'translate-en' | 'translate-it';

export interface DocAction {
  id: DocActionId;
  label: string;
  hint: string;
  /** The submenu it is shown under, and its label there (see `menu-groups.ts`). */
  group?: string;
  child?: string;
}

const COMMENT_ACTIONS: DocAction[] = [
  { id: 'improve', label: 'Improve this comment', hint: 'Clearer wording, same facts', group: 'Improve', child: 'Clearer wording' },
  { id: 'review', label: 'Check against the code', hint: 'Parameters, return value, exceptions, stale statements' },
  { id: 'shorten', label: 'Make it shorter', hint: 'Keep what a reader needs', group: 'Improve', child: 'Shorter' },
  { id: 'translate-en', label: 'Translate to English', hint: '', group: 'Translate', child: 'English' },
  { id: 'translate-it', label: 'Translate to Italian', hint: '', group: 'Translate', child: 'Italian' },
];

const WRITE_ACTIONS: DocAction[] = [
  { id: 'write', label: 'Write documentation', hint: 'From the code and how it is used' },
];

export function actionsFor(target: DocTarget): DocAction[] {
  return target.kind === 'comment' ? COMMENT_ACTIONS : WRITE_ACTIONS;
}

const CONVENTIONS: Record<DocLanguage, string> = {
  java:
    'Javadoc: a first sentence that stands alone as a summary, then @param, @return and @throws only where they add something the name does not say. No @author or dates. Keep the file\'s existing language and tone.',
  rust:
    'rustdoc: a one-line summary, then detail if needed; use `# Errors`, `# Panics` and `# Safety` sections only when they apply, and a runnable example only when it helps. Keep the file\'s existing language and tone.',
};

const TASK: Record<DocActionId, string> = {
  write: 'Write the documentation comment this declaration is missing.',
  improve: 'Improve the existing documentation comment: clearer wording, same facts. Do not add claims the code does not support.',
  review:
    'Check the documentation comment against the code. Report what is wrong or stale (parameters that do not exist or are missing, a wrong return description, exceptions not thrown or not mentioned, statements the code contradicts), then fix the comment.',
  shorten: 'Make the documentation comment shorter, keeping everything a reader needs and nothing they do not.',
  'translate-en': 'Translate the documentation comment to English. Keep the structure and tags.',
  'translate-it': 'Translate the documentation comment to Italian. Keep the structure and tags.',
};

/** The message sent to the assistant for `action` on `target` in `file`. */
export function docPrompt(action: DocActionId, target: DocTarget, file: string, lang: DocLanguage): string {
  const where =
    target.kind === 'comment'
      ? `lines ${target.line}–${target.endLine}, documenting \`${target.declaration}\` (line ${target.declLine})`
      : `line ${target.declLine}: \`${target.declaration}\``;
  return [
    `${TASK[action]}`,
    '',
    `File: ${file}`,
    `Where: ${where}`,
    '',
    `Convention — ${CONVENTIONS[lang]}`,
    '',
    'Read the declaration and what it calls before you write. Change only the comment, with edit_file; do not touch the code.',
  ].join('\n');
}
