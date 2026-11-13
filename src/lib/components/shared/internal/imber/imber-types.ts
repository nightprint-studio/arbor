/**
 * The shapes the shared Imber chat widgets take. They are structural: each product's own message types
 * (Bennu's `ToolCall`, Corvus's `ImberToolCall`) fit them as they are, so neither product converts anything.
 */

/** What a tool call did, which is what picks its icon. */
export type ToolKind = 'read' | 'search' | 'edit' | 'validate' | 'refactor';

export interface ToolCall {
  id: string;
  kind: ToolKind;
  /** `Read BandoService.java`. */
  label: string;
  /** What came back, in a few words: `412 lines`, `3 matches`. */
  detail: string;
  /** False when it was refused or failed; absent in conversations saved before this was recorded. */
  ok?: boolean;
  /** How long it ran, in milliseconds. Absent in conversations saved before this was recorded. */
  ms?: number;
  /** How long it waited for the user's answer on a card first — present only when it did. */
  waitMs?: number;
}

/** One answer of the model in a run: how long the request took, until the first token, and what it wrote. */
export interface ModelStep {
  ms: number;
  firstMs?: number;
  outputTokens: number;
}

/** A row inside the tools fold: a tool call, or the model answer that asked for the calls after it. */
export type WorkItem = { kind: 'call'; call: ToolCall } | { kind: 'step'; step: ModelStep };

/** One conversation in the switcher. `meta` is the quiet figure at the row's edge (its cost); `subtitle` the line under the title. */
export interface ConversationEntry {
  id: string;
  title: string;
  updatedAt: number;
  subtitle?: string;
  meta?: string;
}

/** A card on the home screen: a whole request, or the start of a sentence to finish. */
export interface HomeStarter {
  icon: typeof import('lucide-svelte').Sparkles;
  title: string;
  hint: string;
  text: string;
}

/** A recent conversation on the home screen. */
export interface HomeRecent { id: string; title: string; meta: string }
