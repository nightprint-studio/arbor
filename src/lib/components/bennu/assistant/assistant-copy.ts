/**
 * What "Copy" on a message puts on the clipboard: its readable blocks as Markdown, in order. Receipts of tools,
 * notices and cards that only make sense live (an access request, a confirmation) are left out; what the
 * assistant *said* — text, code, diagrams, and the questions it asked with their options and answers — goes in.
 */
import type { MessageBlock, QuestionSet } from './assistant-types';

/** A set of questions as Markdown: each numbered with its header, the options under it, and the answer once given. */
export function questionsAsText(set: QuestionSet): string {
  const many = set.questions.length > 1;
  return set.questions
    .map((q, i) => {
      const lines = [`${many ? `${i + 1}. ` : ''}**${q.header}** — ${q.text}`];
      const indent = many ? '   ' : '';
      for (const o of q.options) lines.push(`${indent}- ${o.label}${o.description ? ` — ${o.description}` : ''}`);
      const answer = set.answers?.[q.id];
      if (answer?.length) lines.push(`${indent}Answer: ${answer.join(', ')}`);
      return lines.join('\n');
    })
    .join('\n\n');
}

/** The text of one block for the clipboard, or `null` when the block has none worth copying. */
function blockText(b: MessageBlock): string | null {
  switch (b.type) {
    case 'text': return b.text;
    case 'code': return b.code;
    case 'diagram': return '```mermaid\n' + b.diagram.source + '\n```';
    case 'questions': return questionsAsText(b.set);
    default: return null;
  }
}

/** The whole message as the clipboard receives it. */
export function messageAsText(blocks: MessageBlock[]): string {
  return blocks.map(blockText).filter((t): t is string => !!t && t.trim().length > 0).join('\n\n');
}
