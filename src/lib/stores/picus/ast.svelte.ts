/**
 * The syntax tree of whatever document is in front of the user.
 *
 * ## It follows the buffer, not the file
 *
 * A panel that showed the tree of what is *on disk* while the user edits would be
 * wrong from the first keystroke, and wrong in the way that matters most — the
 * moment you want the tree is the moment you have typed something the parser reads
 * differently than you expected. So the text comes from the editor, debounced.
 *
 * ## Selection travels both ways
 *
 * Clicking a node selects its bytes in the editor; moving the caret reveals the
 * node holding it. The second half is what makes the panel a reading tool rather
 * than a toy: you point at the construct you do not understand and the panel says
 * what the grammar called it.
 */

import { untrack } from 'svelte';

import { syntaxPathAt, syntaxTreeOf, type SyntaxNode, type SyntaxTree } from '$lib/ipc/picus/ast';

/** How long a buffer must sit still before it is re-parsed. */
const DEBOUNCE_MS = 220;

/** A node's identity in the panel: its range, which is unique within one tree. */
export function nodeKey(node: SyntaxNode): string {
  return `${node.range.start}:${node.range.end}:${node.kind}`;
}

function createAstStore() {
  let tree = $state<SyntaxTree | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** The text the current tree describes — what a click's ranges index into. */
  let text = $state('');
  let namedOnly = $state(false);
  /** Ranges of the nodes to draw open, as `start:end` — the path to the revealed one. */
  let revealed = $state<string[]>([]);
  /** The node the panel highlights, if any. */
  let selectedKey = $state<string | null>(null);
  /** Bumped when a node is clicked; the view watches it and selects in the editor. */
  let selectRequest = $state<{ start: number; end: number; at: number } | null>(null);

  /** Guards against an older parse landing after a newer one. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function reparse(source: string, mine: number) {
    loading = true;
    try {
      const found = await syntaxTreeOf(source, { namedOnly });
      if (mine !== seq) return;
      tree = found;
      text = source;
      error = null;
    } catch (e) {
      if (mine !== seq) return;
      tree = null;
      error = String(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  return {
    get tree() { return tree; },
    get loading() { return loading; },
    get error() { return error; },
    get text() { return text; },
    get namedOnly() { return namedOnly; },
    get revealed() { return revealed; },
    get selectedKey() { return selectedKey; },
    get selectRequest() { return selectRequest; },

    /** True when the panel has nothing to say about the document in front. */
    get empty() { return !tree && !loading && !error; },

    /**
     * The buffer changed. Debounced, and a no-op when the text is what the current
     * tree already describes — retyping a character and deleting it must not cost
     * a round trip.
     */
    follow(source: string) {
      if (source === text && tree) return;
      if (timer) clearTimeout(timer);
      const mine = ++seq;
      timer = setTimeout(() => void reparse(source, mine), DEBOUNCE_MS);
    },

    /** The document went away. */
    clear() {
      if (timer) clearTimeout(timer);
      seq++;
      tree = null;
      text = '';
      error = null;
      revealed = [];
      selectedKey = null;
      loading = false;
    },

    setNamedOnly(yes: boolean) {
      if (yes === namedOnly) return;
      namedOnly = yes;
      // The shape of the tree changed, so what is held is the wrong tree — not a
      // stale one, a different one.
      const source = text;
      const mine = ++seq;
      void untrack(() => reparse(source, mine));
    },

    /** A node was clicked: highlight it here and select its bytes over there. */
    select(node: SyntaxNode) {
      selectedKey = nodeKey(node);
      selectRequest = { ...node.range, at: Date.now() };
    },

    /**
     * The caret moved: open the tree down to the node holding it.
     *
     * Asks the backend rather than searching the outline, because the outline may
     * have been truncated and "reveal what I am in" must not depend on how far the
     * panel happened to walk.
     */
    async revealAt(offset: number) {
      if (!text) return;
      const mine = seq;
      try {
        const path = await syntaxPathAt(text, offset);
        if (mine !== seq) return;
        revealed = path.map((r) => `${r.start}:${r.end}`);
        const last = path[path.length - 1];
        // Highlighted without a select request: the caret is already there, and
        // moving the selection under the user's own caret would fight them.
        if (last) selectedKey = null;
      } catch {
        // A path that cannot be computed is not worth a message: the tree itself
        // is on screen and still readable.
      }
    },

    /** Should this node be drawn open? */
    isRevealed(node: SyntaxNode): boolean {
      return revealed.includes(`${node.range.start}:${node.range.end}`);
    },
  };
}

export const astStore = createAstStore();
