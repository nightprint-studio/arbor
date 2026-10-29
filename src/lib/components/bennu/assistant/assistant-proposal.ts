/** What a proposal's card becomes when it is decided or made again — pure, so the rules can be tested. */

import type { DecideResult, RemakeResult, UndoResult } from '$lib/ipc/bennu/assistant';
import type { EditProposal } from './assistant-types';

/** The proposal after a decision came back. An `outdated` one keeps its files and marks the stale ones. */
export function withDecision(p: EditProposal, r: DecideResult): EditProposal {
  if (r.status !== 'outdated') return { ...p, status: r.status };
  return { ...p, status: 'outdated', files: p.files.map((f) => ({ ...f, stale: r.stale.includes(f.path) })) };
}

/** The proposal after being made again on the current files; `rejected` when nothing was left to apply. */
export function withRemade(p: EditProposal, r: RemakeResult): EditProposal {
  return { ...p, status: r.status, files: r.status === 'pending' ? r.files : p.files.map((f) => ({ ...f, stale: false })) };
}

/** The message that asks the assistant to make an outdated proposal again. */
export function redoRequest(p: EditProposal): string {
  const stale = p.files.filter((f) => f.stale);
  const named = (stale.length ? stale : p.files).map((f) => `\`${f.path}\``).join(', ');
  return `The changes you proposed to ${named} are outdated: the file changed after you proposed them. Read the current content and propose them again.`;
}

/** Files of an outdated proposal that did change, as a short phrase for a note. */
export function staleSummary(p: EditProposal): string {
  const n = p.files.filter((f) => f.stale).length;
  return n === 0 ? 'The files changed since this was proposed' : n === 1 ? 'The file changed since this was proposed' : `${n} files changed since this was proposed`;
}

/** The proposal after an undo came back: `undone` once every file is back, otherwise it stays as it was. */
export function withUndo(p: EditProposal, r: UndoResult): EditProposal {
  return r.status === 'undone' ? { ...p, status: 'undone' } : p;
}
