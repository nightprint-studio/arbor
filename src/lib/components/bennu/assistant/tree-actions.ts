/**
 * What Imber offers on a file or a folder of the project tree, and the request each choice turns into.
 *
 * The tree knows the path and whether it is a folder; that is all a request needs ("explain this file"), so the
 * user never types a name. The actions are grouped for the menu (see `menu-groups.ts`), and `ask` opens the
 * composer with the path already written so the question can be finished by hand.
 */

import type { Groupable } from './menu-groups';

export type TreeActionId = 'explain' | 'bugs' | 'improve' | 'tests' | 'problems' | 'ask';

export interface TreeAction extends Groupable { id: TreeActionId }

export function treeActionsFor(isDir: boolean): TreeAction[] {
  if (isDir) {
    return [
      { id: 'explain', label: 'Explain this folder' },
      { id: 'problems', label: 'Look for problems in it' },
      { id: 'ask', label: 'Ask about this…' },
    ];
  }
  return [
    { id: 'explain', label: 'Explain this file' },
    { id: 'bugs', label: 'Look for bugs', group: 'Review', child: 'Look for bugs' },
    { id: 'improve', label: 'Suggest improvements', group: 'Review', child: 'Suggest improvements' },
    { id: 'tests', label: 'Write tests for it' },
    { id: 'ask', label: 'Ask about this…' },
  ];
}

/** The message sent to Imber for `action` on `path` (relative to the project). */
export function treePrompt(action: Exclude<TreeActionId, 'ask'>, path: string, isDir: boolean): string {
  const what = `\`${path}\``;
  switch (action) {
    case 'explain':
      return isDir
        ? `Give me an overview of the folder ${what}: what lives there, how the parts relate, and where to start reading.`
        : `Explain what ${what} does and how it fits into the project.`;
    case 'bugs':
      return `Read ${what} and look for bugs: wrong conditions, null handling, resource leaks, concurrency problems. Report what you find; change nothing yet.`;
    case 'improve':
      return `Read ${what} and suggest improvements to its structure, naming and clarity. Do not change anything yet.`;
    case 'tests':
      return `Write tests for ${what}, in the project's existing test style.`;
    case 'problems':
      return `Look at the problems the project reports in ${what} and tell me what is wrong and how to fix it.`;
  }
}

/** The start of a question about `path`, for the composer. */
export function treeDraft(path: string): string {
  return `About \`${path}\`: `;
}
