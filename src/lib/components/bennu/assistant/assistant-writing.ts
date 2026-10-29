/**
 * What the assistant offers on a passage of prose — a README, a design note — and what each choice asks.
 *
 * Code has a margin mark and declarations to hang things on; a Markdown document has a selection. Select a
 * passage and a small button appears at its end; it offers the things people do to a paragraph: say it
 * better, shorter, fuller, without the typos, in another language. The assistant rewrites the passage in the
 * file with an exact edit, so the change arrives as a proposal to review like any other.
 *
 * Pure — the context goes in, items and prompts come out — so it is tested.
 */

export type WritingActionId = 'improve' | 'shorten' | 'expand' | 'fix' | 'translate-en' | 'translate-it' | 'ask';

export interface WritingAction {
  id: WritingActionId;
  label: string;
}

export const WRITING_ACTIONS: WritingAction[] = [
  { id: 'improve', label: 'Improve the writing' },
  { id: 'shorten', label: 'Make it shorter' },
  { id: 'expand', label: 'Say more' },
  { id: 'fix', label: 'Fix spelling and grammar' },
  { id: 'translate-en', label: 'Translate to English' },
  { id: 'translate-it', label: 'Translate to Italian' },
  { id: 'ask', label: 'Ask about this…' },
];

export interface WritingContext {
  /** The file's path as the assistant should see it. */
  file: string;
  /** The selected passage, exactly as it is in the file. */
  text: string;
}

const MAX_PASSAGE_CHARS = 6000;

const TASK: Record<Exclude<WritingActionId, 'ask'>, string> = {
  improve: 'Improve the writing of this passage: clearer, more direct, same meaning and same facts.',
  shorten: 'Make this passage shorter, keeping what a reader needs and nothing they do not.',
  expand: 'Expand this passage: say more where it is thin, with concrete detail, without padding or inventing facts the document does not support.',
  fix: 'Fix the spelling, grammar and punctuation of this passage. Change nothing else.',
  'translate-en': 'Translate this passage to English. Keep the Markdown structure and any code or names as they are.',
  'translate-it': 'Translate this passage to Italian. Keep the Markdown structure and any code or names as they are.',
};

/** The message sent for `action`. `ask` is not here: it opens the composer instead of sending. */
export function writingPrompt(action: Exclude<WritingActionId, 'ask'>, c: WritingContext): string {
  const passage = c.text.length > MAX_PASSAGE_CHARS ? `${c.text.slice(0, MAX_PASSAGE_CHARS)}\n… (cut)` : c.text;
  return [
    TASK[action],
    '',
    `File: ${c.file}`,
    '',
    '```markdown',
    passage,
    '```',
    '',
    'Make the change in the file with edit_file: `old_text` must be the passage exactly as it appears above (read the file first if you are not sure), and keep its Markdown structure — headings, lists, links, code blocks. Do not touch anything outside the passage.',
  ].join('\n');
}

export function writingDraft(c: WritingContext): string {
  const first = c.text.trim().split(/\r?\n/)[0].slice(0, 60);
  return `About "${first}${c.text.trim().length > first.length ? '…' : ''}" in ${c.file}: `;
}
