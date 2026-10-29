/**
 * The three ways the assistant can run, in one place: what each is called, what it means in a line, and
 * how a saved word is read. The composer menu, the settings page and the command palette all draw from this
 * list, so a mode cannot be described three different ways.
 */

import type { AssistantMode } from './assistant-types';

export interface ModeInfo {
  id: AssistantMode;
  label: string;
  /** One line: what it does and what it still asks. */
  description: string;
  /** `warning` marks the mode that widens its own reach — it is drawn so you do not pick it by accident. */
  tone: 'neutral' | 'warning';
}

export const MODES: readonly ModeInfo[] = [
  {
    id: 'manual',
    label: 'Manual',
    description: 'Every edit is a proposal you approve. Asks before every deletion, every build or test run, and for anything outside what you allowed.',
    tone: 'neutral',
  },
  {
    id: 'accept_edits',
    label: 'Accept edits',
    description: 'Applies edits and deletions at once inside what you allowed. Asks once per run before building or testing, and for anything outside what you allowed.',
    tone: 'neutral',
  },
  {
    id: 'autonomous',
    label: 'Autonomous',
    description: 'Like Accept edits, builds and runs the project’s own tests without asking, and opens the files its own build reports an error in. Everything else still asks.',
    tone: 'warning',
  },
];

export const DEFAULT_MODE: AssistantMode = 'manual';

/** A saved or received word as a mode. The old `auto` (the two-way switch's middle position) is Accept edits;
 *  anything unknown is the cautious one. */
export function parseMode(word: string | null | undefined): AssistantMode {
  switch (word?.trim().toLowerCase()) {
    case 'accept_edits':
    case 'accept-edits':
    case 'auto':
      return 'accept_edits';
    case 'autonomous':
      return 'autonomous';
    default:
      return DEFAULT_MODE;
  }
}

export function modeInfo(mode: AssistantMode): ModeInfo {
  return MODES.find((m) => m.id === mode) ?? MODES[0];
}

/** Edits are written as they are made, rather than held for review. */
export function modeAppliesEdits(mode: AssistantMode): boolean {
  return mode !== 'manual';
}

/** The next mode in the list, wrapping — for a keyboard shortcut that cycles. */
export function nextMode(mode: AssistantMode): AssistantMode {
  const at = MODES.findIndex((m) => m.id === mode);
  return MODES[(at + 1) % MODES.length].id;
}
