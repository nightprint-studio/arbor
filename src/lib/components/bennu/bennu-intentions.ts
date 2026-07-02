/**
 * Bennu "intentions" (Alt+Enter quick-fixes / context actions) — pure, CM-free.
 *
 * Mirrors merula's `merula-intentions.ts` in shape: given the caret context the
 * editor will eventually pass (source, offset, the identifier under the caret,
 * the file outline), this returns the actions applicable right there. Each item
 * carries a `run()` the overlay invokes on Enter/click.
 *
 * SCAFFOLD — the collector below returns a fixed MOCK set of Java intentions.
 * The two "Generate…" items delegate to the `onGenerate(mode)` callback the Wire
 * phase will point at the Generate modal; every other item is a stub that toasts
 * "not implemented". When the Java language service (`bennu_completion` / a real
 * quick-fix index) lands, replace `collectIntentions` with a context-aware build
 * (the `IntentionItem` shape + the overlay can stay unchanged) — this is the seam.
 */

import {
  Hammer, ArrowLeftRight, PackagePlus, ShieldAlert, Variable,
} from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';
import type { JavaSymbol } from './java-outline';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

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

/** The caret context the editor hands the collector. All fields are optional so
 *  the scaffold can be exercised before the editor exposes real caret data —
 *  the Wire phase fills them in from CodeMirror. */
export interface IntentionContext {
  /** Full source of the active buffer. */
  src?: string;
  /** Caret offset (UTF-16) into `src`. */
  offset?: number;
  /** The identifier under the caret, if any (for "Add import for symbol"). */
  wordUnderCaret?: string | null;
  /** The active file outline (regex-based) — lets a real collector scope actions
   *  to the enclosing class/method later. */
  outline?: JavaSymbol[];
}

/** Callbacks the collector needs to build the actionable items — supplied by the
 *  editor host (the overlay forwards them down). Keeps the mock set free of any
 *  direct store/modal coupling so the contract is explicit. */
export interface IntentionCallbacks {
  /** Open the Generate modal in the given mode (Wire phase points this at it). */
  onGenerate: (mode: GenerateMode) => void;
}

// MOCK — a stub action: toast that the flow isn't wired yet. Replaced per-item
// with real logic as the Java language service lands.
function notImplemented(label: string): void {
  // MOCK — no real quick-fix engine yet.
  toastStore.show(`${label} — not implemented yet`, 'info');
}

/**
 * The intentions available at the caret. SCAFFOLD: returns a fixed MOCK Java set
 * regardless of context (the shape is real; the gating on `ctx` is the follow-up).
 * The two "Generate…" items are wired to `cb.onGenerate`; the rest are stubs.
 */
export function collectIntentions(
  _ctx: IntentionContext,
  cb: IntentionCallbacks,
): IntentionItem[] {
  const word = _ctx.wordUnderCaret?.trim();
  return [
    // Real callback — routed to the Generate modal by the Wire phase.
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
    // MOCK — stubs until the Java language service lands.
    {
      id: 'add-import',
      label: word ? `Add import for "${word}"` : 'Add import for symbol',
      icon: PackagePlus,
      run: () => notImplemented('Add import'), // MOCK
    },
    {
      id: 'surround-try-catch',
      label: 'Surround with try/catch',
      icon: ShieldAlert,
      run: () => notImplemented('Surround with try/catch'), // MOCK
    },
    {
      id: 'introduce-variable',
      label: 'Introduce variable',
      icon: Variable,
      run: () => notImplemented('Introduce variable'), // MOCK
    },
  ];
}
