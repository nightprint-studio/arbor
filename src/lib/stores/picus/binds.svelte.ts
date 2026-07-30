/**
 * The values a parameterised statement is waiting for.
 *
 * Two things live here, and they are different in kind:
 *
 *  • **the prompt** — the one statement currently asking. A run that finds
 *    placeholders stops before sending anything and files a prompt; the modal that
 *    renders it is what starts the run again;
 *  • **what each tab last supplied**, keyed by placeholder. Re-running the same
 *    query is the normal case — you change one value and press Run — so the boxes
 *    come back filled in. In memory only: a value typed into a `WHERE` is
 *    frequently somebody's customer number, and there is no version of writing that
 *    to disk that is worth the convenience.
 *
 * ## NULL is a value, not an empty box
 *
 * An entry carries its text *and* whether it is NULL, because on a text column
 * `''` and `NULL` select different rows — and in a maintenance tool that difference
 * is how a wrong `UPDATE` gets written. The two are never inferred from each other:
 * clearing the box gives the empty string, and NULL is a switch.
 */

import type { BindSlot } from '$lib/components/picus/sql-intel/binds';
import type { RunScope } from './query.svelte';

/** One placeholder's value, as the user set it. */
export interface BindEntry {
  text: string;
  /** Send a real SQL NULL, whatever `text` holds. */
  isNull: boolean;
}

/** A run that stopped to ask. */
export interface BindPrompt {
  tabId: string;
  connectionId: string;
  /** The scope the interrupted run was started with — restarted unchanged. */
  scope: RunScope;
  /** What the statement wants, in the order it reads. */
  slots: BindSlot[];
}

/** The starting point for a placeholder nobody has filled in yet. */
export function emptyEntry(): BindEntry {
  return { text: '', isNull: false };
}

/**
 * Placeholders fold case, so `:codice` and `:CODICE` are one value. Asking twice
 * for the same thing under two spellings is how a second box ends up empty.
 */
function key(label: string): string {
  return label.toUpperCase();
}

function createBindsStore() {
  let prompt = $state<BindPrompt | null>(null);
  let byTab = $state<Record<string, Record<string, BindEntry>>>({});

  return {
    get prompt() { return prompt; },

    /** Stop a run and ask. */
    ask(next: BindPrompt) { prompt = next; },

    /** The user cancelled, or the run has been restarted. */
    close() { prompt = null; },

    /** What this tab last supplied for one placeholder, ready to edit. */
    entry(tabId: string, label: string): BindEntry {
      return byTab[tabId]?.[key(label)] ?? emptyEntry();
    },

    /**
     * Keep what the user just typed. Merged rather than replaced: a run over a
     * selection asks about its own placeholders, and forgetting the rest would
     * empty boxes the user filled in a moment earlier for the same tab.
     */
    remember(tabId: string, entries: Record<string, BindEntry>) {
      const merged = { ...(byTab[tabId] ?? {}) };
      for (const [label, entry] of Object.entries(entries)) merged[key(label)] = entry;
      byTab = { ...byTab, [tabId]: merged };
    },

    /**
     * The value to send for one placeholder: its text, or a real NULL.
     *
     * A placeholder nobody filled in is NULL rather than the empty string — the
     * modal always asks for every one of them, so this only answers for the gaps
     * PostgreSQL's numbering leaves (`$1` and `$3` with no `$2`), and a NULL is the
     * honest thing to put where the user wrote nothing at all.
     */
    valueOf(tabId: string, label: string): string | null {
      const entry = byTab[tabId]?.[key(label)];
      if (!entry || entry.isNull) return null;
      return entry.text;
    },

    /** The tab is gone. */
    forget(tabId: string) {
      if (prompt?.tabId === tabId) prompt = null;
      if (!byTab[tabId]) return;
      const { [tabId]: _gone, ...rest } = byTab;
      byTab = rest;
    },
  };
}

export const picusBindsStore = createBindsStore();
