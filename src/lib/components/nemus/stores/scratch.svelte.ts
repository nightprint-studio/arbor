/**
 * nemus Scratch / expression-evaluator panel state — **multi-tab**.
 *
 * The Scratch panel lets you take arbitrary `.nemus` chunks — pasted, typed, or
 * loaded from the editor selection — and (a) **evaluate** them in isolation to
 * inspect the events they generate, and (b) **play** them one-shot to hear their
 * effect, all without touching the live arrangement. Each chunk lives in its own
 * tab so you can keep several experiments side by side.
 *
 * Window-local UI state, rune-store pattern. The eval call lives in the panel (it
 * owns the debounce + in-flight token); this store holds the tabs + their last
 * results so other entry points (right-click → "Send to Scratch") can seed a tab.
 */

import type { NemusSnippetEval } from '$lib/ipc/nemus';

export interface ScratchTab {
  id: string;
  name: string;
  source: string;
  result: NemusSnippetEval | null;
  evaluating: boolean;
}

function createScratchStore() {
  let seq = 0;
  const fresh = (source = ''): ScratchTab => {
    seq += 1;
    return { id: `scr-${seq}`, name: `Scratch ${seq}`, source, result: null, evaluating: false };
  };

  let tabs = $state<ScratchTab[]>([fresh()]);
  let activeId = $state<string>(tabs[0].id);

  const find = (id: string) => tabs.find((t) => t.id === id);

  return {
    get tabs() { return tabs; },
    get activeId() { return activeId; },
    /** The active tab (always present — the store keeps at least one). */
    get active() { return find(activeId) ?? tabs[0]; },

    setActive(id: string) { if (find(id)) activeId = id; },

    setSource(id: string, s: string) { const t = find(id); if (t) t.source = s; },
    /** Give a tab a custom title (empty/blank falls back to its current name). */
    renameTab(id: string, name: string) { const t = find(id); if (t && name.trim()) t.name = name.trim(); },
    setResult(id: string, r: NemusSnippetEval | null) { const t = find(id); if (t) t.result = r; },
    setEvaluating(id: string, v: boolean) { const t = find(id); if (t) t.evaluating = v; },

    /** Open a new tab (optionally seeded with text) and make it active. */
    addTab(source = ''): string {
      const t = fresh(source);
      tabs = [...tabs, t];
      activeId = t.id;
      return t.id;
    },

    /** Close a tab; keep at least one (a fresh empty tab replaces the last). */
    closeTab(id: string) {
      const idx = tabs.findIndex((t) => t.id === id);
      if (idx === -1) return;
      const next = tabs.filter((t) => t.id !== id);
      if (next.length === 0) {
        const t = fresh();
        tabs = [t];
        activeId = t.id;
        return;
      }
      tabs = next;
      if (activeId === id) {
        // Activate the neighbour (prefer the previous tab).
        activeId = next[Math.max(0, idx - 1)].id;
      }
    },

    /** Seed a chunk (right-click → "Send to Scratch", Load selection) into a new
     *  tab and activate it. */
    load(text: string) { this.addTab(text); },
  };
}

export const scratchStore = createScratchStore();
