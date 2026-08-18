/**
 * The i18n panel's state — the translation under the caret, and what is being written into it.
 *
 * ## Why a store and not panel-local state
 *
 * Two things have to reach each other that cannot see each other. The **panel** knows which
 * construct you asked for; the **editor** owns the buffer, the selection and the undo history. A
 * panel in the bottom dock has no handle on the editor component, and giving it one would make every
 * dock panel a potential writer into the buffer.
 *
 * So this store is the seam, and it carries traffic in both directions:
 *
 * - the editor pushes `(path, source, caret)` in, on a debounce, and the store fetches;
 * - the panel pushes an **intent** in (`wrap this in $red.bold`), and the editor picks it up,
 *   resolves it against the live selection, and applies it.
 *
 * The intent is deliberately not a computed edit. Only the editor knows what is selected at the
 * moment the button is pressed, and an edit computed in the panel from a view that arrived 200ms ago
 * would be applied at offsets the buffer has since moved — silently, and into the middle of a word.
 *
 * ## Sample values are not settings
 *
 * The parameter table's sample values live here, in memory, keyed by label. They are scratch — what
 * you typed to see whether the sentence reads well with a long name in it — and they are gone on
 * restart on purpose. Persisting them would put a per-label cache of throwaway strings into the
 * project's config for no gain.
 */

import { SvelteMap } from 'svelte/reactivity';

import { i18nStudio, type StudioAnswer, type StudioView } from '$lib/ipc/bennu/i18n';
import type { Insert } from '$lib/components/bennu/i18n/markup-edit';

/** How long after the last keystroke the view is re-fetched. The editor's own framework queries use
 *  the same 220ms, and matching them means one burst of work per pause rather than two. */
const DEBOUNCE_MS = 220;

function createBennuI18nStore() {
  let view = $state<StudioView | null>(null);
  /** The whole answer, kept so the empty state can say which link was missing. */
  let answer = $state<StudioAnswer | null>(null);
  let loading = $state(false);
  /** What the panel says when the backend cannot answer at all, as opposed to answering "nothing". */
  let failed = $state(false);

  /** label → param name → sample value. */
  const samples = new SvelteMap<string, SvelteMap<string, string>>();
  /**
   * What a label with no samples reads as. Shared and never written to.
   *
   * The reason it exists: {@link samplesFor} is called from a `$derived`, and creating the label's
   * map on that read was a **write during derivation** — Svelte throws `state_unsafe_mutation` and
   * aborts the update, so the panel silently kept whatever was on screen before the value arrived,
   * which was its own "put the caret on a translation" empty state. Reads here are pure; the map is
   * created on the first write, in {@link setSample}, which is an event handler and may write.
   */
  const NO_SAMPLES: ReadonlyMap<string, string> = new Map();

  /**
   * The construct the panel asked for, waiting for the editor to write it.
   *
   * Carries a sequence number because the same request twice is a real gesture — pressing **bold**
   * on two different words in a row sends an identical `insert`, and without the counter the second
   * press would not change the state and the effect would not run.
   */
  let request = $state<{ insert: Insert; seq: number } | null>(null);
  let seq = 0;

  /**
   * Bumped to make the editor ask again with nothing having changed in the buffer.
   *
   * What a rescan needs: the project's capabilities were rebuilt, so the same file at the same caret
   * now has a different answer, and the effect that feeds this store depends on the buffer and the
   * caret — neither of which moved.
   */
  let retry = $state(0);

  let timer: ReturnType<typeof setTimeout> | null = null;
  /** Which fetch is current. A slower earlier answer must not overwrite a newer one. */
  let generation = 0;

  function clear() {
    if (timer) { clearTimeout(timer); timer = null; }
    generation += 1;
    view = null;
    answer = null;
    loading = false;
    failed = false;
  }

  return {
    get view() { return view; },
    /** The last answer, for the empty state — see {@link StudioAnswer}. `null` before the first. */
    get answer() { return answer; },
    get loading() { return loading; },
    get failed() { return failed; },

    /**
     * Re-read the translation at `caret`, on a debounce. Called by the editor on every caret move
     * and every keystroke while the panel is open.
     *
     * `loading` is set only when there is nothing on screen yet: a spinner replacing a preview that
     * is about to be replaced by an almost identical preview is a flicker on every keypress, and the
     * old sentence for 200ms reads better than a spinner for 200ms.
     */
    track(file: string, source: string, caret: number) {
      if (timer) clearTimeout(timer);
      if (!view) loading = true;
      const mine = ++generation;
      timer = setTimeout(() => {
        timer = null;
        void i18nStudio(file, source, caret)
          .then((next) => {
            if (mine !== generation) return;
            answer = next;
            view = next.view;
            loading = false;
            failed = false;
          })
          .catch(() => {
            if (mine !== generation) return;
            // A project with no fulcrum model answers, it does not error — so an error here is the
            // backend being unreachable, which is worth saying out loud rather than showing as an
            // empty panel.
            answer = null;
            view = null;
            loading = false;
            failed = true;
          });
      }, DEBOUNCE_MS);
    },

    /** Forget everything — the panel closed, or the file being edited is not a bundle. */
    reset() { clear(); },

    /** Read by the editor's feed effect, so bumping it re-asks. */
    get retry() { return retry; },
    /** Ask again without anything in the buffer having changed. */
    requestRetry() { retry += 1; },

    /**
     * The sample values for a label. **Read-only, and pure** — see {@link NO_SAMPLES}.
     *
     * Reading `samples.get(label)` registers a dependency on that one key, so the panel re-renders
     * when the first sample for this label is typed without depending on every other label's.
     */
    samplesFor(label: string): ReadonlyMap<string, string> {
      return samples.get(label) ?? NO_SAMPLES;
    },

    /** Set (or clear, with an empty string) one parameter's sample value. */
    setSample(label: string, param: string, value: string) {
      let m = samples.get(label);
      if (!m) {
        // Nothing to clear, and no reason to allocate a map for a field that was emptied.
        if (!value) return;
        m = new SvelteMap();
        samples.set(label, m);
      }
      if (value) m.set(param, value); else m.delete(param);
    },

    get request() { return request; },
    /** Ask the editor to write `insert` around the selection. */
    insert(what: Insert) { request = { insert: what, seq: ++seq }; },
    /** Called by the editor once it has applied (or declined) a request. */
    consume() { request = null; },
  };
}

export const bennuI18nStore = createBennuI18nStore();
