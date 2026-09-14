/**
 * The **Alt+Enter list's model** — what a row is, which section it sits in, and the order sections
 * are read in.
 *
 * Every row comes from an engine that looked at the code: `bennu_intentions` for Java (fixes,
 * intentions and — where a member is being written — the generators), a language server's code
 * actions, the refactorings. This file only says how they are presented, so the popup and every
 * source agree on one vocabulary.
 *
 * ## Why sections
 *
 * The rows answer different questions, and a flat list made them compete: on an unimported `List`,
 * "Generate constructor…" and "Move member to…" sat in the same column as the six imports, with
 * nothing to say the imports were the reason the key was pressed. Sections put the **fix** first,
 * then what changes working code, then the refactorings, then the generators — the IntelliJ order,
 * each with its own colour, so the eye finds the red bulb without reading.
 */

import { Hammer, Lightbulb, Wand2 } from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';

/** Which Generate flow an intention opens (routed to the Generate modal by the
 *  Wire phase via the overlay's `onGenerate` callback). Kept as a small named
 *  union so the modal and the intentions agree on one vocabulary. */
export type GenerateMode = 'constructor' | 'getters-setters';

/** The section a row belongs to — see the module docs for the order. */
export type IntentionCategory = 'fix' | 'intention' | 'refactor' | 'generate';

/** One context action offered in the Alt+Enter popup. `run()` performs it (the
 *  overlay calls it on Enter/click, then closes). */
export interface IntentionItem {
  id: string;
  label: string;
  /** Lucide icon component (rendered at 14px in the list). */
  icon: IconComponent;
  category: IntentionCategory;
  /** The one to take among several alike — marked in the list, and first in its section. */
  preferred?: boolean;
  /** Shown to say why it cannot be done here (the reason is in the label); picking it does nothing. */
  disabled?: boolean;
  /** Perform the action. Called once when the user picks the item. */
  run: () => void;
}

/** How each section is titled and marked, in reading order. */
export const INTENTION_SECTIONS: readonly { category: IntentionCategory; title: string; icon: IconComponent }[] = [
  { category: 'fix', title: 'Quick fixes', icon: Lightbulb },
  { category: 'intention', title: 'Intentions', icon: Lightbulb },
  { category: 'refactor', title: 'Refactor', icon: Wand2 },
  { category: 'generate', title: 'Generate', icon: Hammer },
];

/**
 * The rows in the order the popup draws them: by section, then the preferred row first and the
 * disabled ones last within it. Stable otherwise — every source already orders its own rows by what
 * it knows (the backend ranks imports, a server ranks its actions), and a sort here knows less.
 */
export function orderIntentions(items: readonly IntentionItem[]): IntentionItem[] {
  const section = (c: IntentionCategory) => INTENTION_SECTIONS.findIndex((s) => s.category === c);
  const weight = (i: IntentionItem) => section(i.category) * 3 + (i.preferred ? 0 : i.disabled ? 2 : 1);
  return [...items].sort((a, b) => weight(a) - weight(b));
}

