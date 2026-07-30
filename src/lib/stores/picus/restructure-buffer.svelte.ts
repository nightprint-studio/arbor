/**
 * Structural replace inside the document in front of you.
 *
 * The same pattern language as the repository-wide migration
 * ([`restructureStore`](./restructure.svelte)), pointed at one buffer — the query
 * tab or the script you have open — and that difference is worth stating, because
 * it is why this is a separate store rather than a scope of the other one:
 *
 * | | repository | this tab |
 * |---|---|---|
 * | reads | scripts on disk | the editor's live buffer |
 * | writes | files, after a preview and a digest check | nothing — the *editor* applies the edit |
 * | undo | re-run the inverse migration | Ctrl+Z |
 *
 * That last row is the whole design. Handing the ranges to the editor rather than
 * rewriting the text ourselves is what keeps a forty-match replace inside the
 * buffer's own history, in one step, exactly like every other edit in it. A store
 * that produced a new string and pushed it in would be the one edit in Picus you
 * cannot take back.
 *
 * ## It follows the buffer
 *
 * Matches are re-found as you type — in the pattern *or* in the document, debounced
 * — for the same reason the syntax-tree panel re-parses: the moment you want to see
 * what a pattern catches is the moment you are still writing it. The count in the
 * header is therefore always about the text on screen.
 *
 * ## Nothing is applied against a buffer that moved
 *
 * The ranges are UTF-8 offsets into one exact version of the text. If the document
 * changed since they were found — a keystroke inside the debounce window, an edit
 * from somewhere else — applying them would splice at the wrong places. So every
 * replace re-checks the text it was measured against and, if it moved, refreshes
 * the list and replaces nothing. The repository half refuses a stale write by
 * comparing digests; this is the same promise, one buffer down.
 */

import { structuralScan, type Hit } from '$lib/ipc/picus/restructure';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

import { picusEditorStore } from './editor.svelte';

/** How long the buffer or the pattern must sit still before it is re-scanned. */
const DEBOUNCE_MS = 200;

function createBufferRestructureStore() {
  let pattern = $state('');
  let replacement = $state('');
  /** The editor's own text, as the bridge last reported it. */
  let source = $state('');
  /** The text {@link matches} describe — what their offsets index into. */
  let scanned = $state('');
  let matches = $state<Hit[]>([]);
  let placeholders = $state<string[]>([]);
  let scanning = $state(false);
  let error = $state<string | null>(null);
  /** Start offset of the highlighted match, or `null`. Not an index: the list is
   *  rebuilt on every scan, and an index would point at a different row after one. */
  let selectedAt = $state<number | null>(null);

  /** Guards against an older scan landing after a newer one. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  /** Matches that carry a replacement that could actually be rendered here. */
  const ready = $derived(
    matches.filter((m) => typeof m.replacement === 'string' && !m.problem),
  );
  /** Matches the template could not be applied to — the rows to fix first. */
  const failing = $derived(matches.filter((m) => !!m.problem));

  async function scan(mine: number) {
    const text = source;
    if (!pattern.trim()) {
      matches = [];
      placeholders = [];
      error = null;
      scanned = text;
      scanning = false;
      return;
    }
    scanning = true;
    try {
      const found = await structuralScan(text, pattern, replacement.trim() || undefined);
      if (mine !== seq) return;
      matches = found.matches;
      placeholders = found.placeholders;
      scanned = text;
      error = null;
    } catch (e) {
      if (mine !== seq) return;
      matches = [];
      placeholders = [];
      scanned = text;
      error = String(e);
    } finally {
      if (mine === seq) scanning = false;
    }
  }

  function schedule() {
    if (timer) clearTimeout(timer);
    const mine = ++seq;
    timer = setTimeout(() => void scan(mine), DEBOUNCE_MS);
  }

  /**
   * The editor to act on, and the text it currently holds — or `null` when the
   * buffer moved since the matches were found, in which case the list is refreshed
   * and the caller does nothing.
   */
  function editorOnCurrentText() {
    const handle = picusEditorStore.active;
    if (!handle) return null;
    const live = handle.getValue();
    if (live === scanned) return handle;
    source = live;
    schedule();
    toastStore.show(
      'The document changed since these matches were found, so nothing was replaced. They have been looked for again.',
      'warning',
    );
    return null;
  }

  return {
    get pattern() { return pattern; },
    get replacement() { return replacement; },
    get matches() { return matches; },
    get placeholders() { return placeholders; },
    get scanning() { return scanning; },
    get error() { return error; },
    get ready() { return ready; },
    get failing() { return failing; },
    get selectedAt() { return selectedAt; },

    /** True once there is a pattern to answer for — what tells "no matches" apart
     *  from "nothing asked yet", which are different things to put on screen. */
    get asked() { return !!pattern.trim(); },
    /** A replacement was written, so the panel is a rewrite rather than a search. */
    get rewriting() { return !!replacement.trim(); },

    setPattern(text: string) {
      pattern = text;
      schedule();
    },
    setReplacement(text: string) {
      replacement = text;
      schedule();
    },

    /** The buffer changed. Called by the document bridge on every keystroke. */
    follow(text: string) {
      if (text === source) return;
      source = text;
      schedule();
    },

    /** The document went away — the panel must stop describing one that is gone. */
    clear() {
      if (timer) clearTimeout(timer);
      seq++;
      source = '';
      scanned = '';
      matches = [];
      placeholders = [];
      error = null;
      scanning = false;
      selectedAt = null;
    },

    /** Show a match in the editor, and mark it here. */
    reveal(hit: Hit) {
      selectedAt = hit.range.start;
      picusEditorStore.active?.selectByteRange(hit.range.start, hit.range.end);
    },

    /**
     * Replace every match that has a rendered replacement — as **one** edit, and
     * therefore one press of Ctrl+Z.
     *
     * The matches that failed to render are left exactly where they are. A rewrite
     * that quietly skipped some of what it matched would be worse than one that
     * refuses, but this is a buffer the user is looking at: the failing rows are on
     * screen next to the button, so doing the ones that work and saying how many is
     * the more useful half of that trade.
     */
    replaceAll() {
      const handle = editorOnCurrentText();
      if (!handle || !ready.length) return;
      const done = handle.replaceByteRanges(
        ready.map((m) => ({
          startByte: m.range.start,
          endByte: m.range.end,
          text: m.replacement as string,
        })),
      );
      toastStore.show(
        `${done} statement${done === 1 ? '' : 's'} rewritten. Ctrl+Z takes it back.`,
        'success',
      );
    },

    /** Replace one match — the row's own button. */
    replaceOne(hit: Hit) {
      const handle = editorOnCurrentText();
      if (!handle || typeof hit.replacement !== 'string' || hit.problem) return;
      handle.replaceByteRange(hit.range.start, hit.range.end, hit.replacement);
    },

    reset() {
      pattern = '';
      replacement = '';
      matches = [];
      placeholders = [];
      error = null;
      selectedAt = null;
    },
  };
}

export const bufferRestructureStore = createBufferRestructureStore();
