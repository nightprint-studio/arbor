/**
 * The **Java generators** offered at the caret — the editor-local half of Alt+Enter.
 *
 * Everything that needs to look at the code is answered elsewhere and by something that can:
 * `bennu_intentions` for Java (the real quick-fix catalogue, resolved against the buffer at the
 * caret) and a language server's code actions for every other language. What is left here is the two
 * flows that are not fixes at all — they open the Generate modal — and they are here because they are
 * *editor gestures* rather than an engine's answer.
 *
 * ## Why this file is now two items long
 *
 * It used to return five, three of which were stubs that toasted "not implemented", offered on every
 * file regardless of the caret. That is worse than offering nothing: a list where most rows do
 * nothing teaches that Alt+Enter does nothing, and it buried the rows that do work. Add-import,
 * surround-with and introduce-variable are all things a real engine offers — Java's from the backend,
 * Rust's from rust-analyzer — so the honest move was to delete the placeholders rather than keep
 * promising them.
 */

import { Hammer, ArrowLeftRight } from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';
import type { JavaSymbol } from './java-outline';

/** Which Generate flow an intention opens (routed to the Generate modal by the
 *  Wire phase via the overlay's `onGenerate` callback). Kept as a small named
 *  union so the modal and the intentions agree on one vocabulary. */
export type GenerateMode = 'constructor' | 'getters-setters';

/** One context action offered in the Alt+Enter popup. `run()` performs it (the
 *  overlay calls it on Enter/click, then closes). */
export interface IntentionItem {
  id: string;
  label: string;
  /** Lucide icon component (rendered at 14px in the list). */
  icon: IconComponent;
  /** Perform the action. Called once when the user picks the item. */
  run: () => void;
}

/** The caret context the editor hands the collector. */
export interface IntentionContext {
  /** Full source of the active buffer. */
  src?: string;
  /** Caret offset (UTF-16) into `src`. */
  offset?: number;
  /** The identifier under the caret, if any. */
  wordUnderCaret?: string | null;
  /** The active file outline (regex-based) — what decides whether there is a type to generate
   *  into. */
  outline?: JavaSymbol[];
}

/** Callbacks the collector needs to build the actionable items — supplied by the
 *  editor host (the overlay forwards them down). Keeps the mock set free of any
 *  direct store/modal coupling so the contract is explicit. */
export interface IntentionCallbacks {
  /** Open the Generate modal in the given mode (Wire phase points this at it). */
  onGenerate: (mode: GenerateMode) => void;
}

/**
 * The generators available at the caret.
 *
 * Both write **members into a type**, so both are offered only when the file declares one — which is
 * what the outline is for and what this ignored while it was a fixed list. In a file that declares no
 * type (a `package-info.java`, a buffer being started) they are absent rather than opening a dialog
 * with nothing to generate into.
 *
 * The caller decides whether the *language* is right: these are Java flows, and the editor does not
 * offer them on a file Java has nothing to say about.
 */
export function collectIntentions(
  ctx: IntentionContext,
  cb: IntentionCallbacks,
): IntentionItem[] {
  if (!(ctx.outline?.length)) return [];
  return [
    {
      id: 'gen-constructor',
      label: 'Generate constructor…',
      icon: Hammer,
      run: () => cb.onGenerate('constructor'),
    },
    {
      id: 'gen-getters-setters',
      label: 'Generate getters and setters…',
      icon: ArrowLeftRight,
      run: () => cb.onGenerate('getters-setters'),
    },
  ];
}
