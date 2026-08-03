/**
 * Picus engine descriptors — what each engine *has*, as data.
 *
 * The point of the capability matrix is that the interface reads it instead of
 * branching on the engine's name: a third engine should be a crate plus a
 * descriptor, not an edit to six `if (dialect === 'oracle')` in six components. That
 * only holds if there is one place the descriptors are read, which is this store.
 *
 * Loaded once and kept. Descriptors are static per build — they describe what the
 * *code* supports, not what a server happens to be doing — so a reload would only
 * ever fetch the same document.
 *
 * A feature whose capability is false must be **absent** from the interface, not
 * present and failing. `false` is therefore the answer while the descriptors are
 * still loading: a control that appears a moment late is better than one that
 * appears and then refuses.
 */

import type { Dialect } from '$lib/types/picus';
import { listProviders, type DbProviderDescriptor, type EngineCapabilities } from '$lib/ipc/picus/db';

/** Floor between two attempts after a failure. */
const RETRY_MS = 5000;

function createProvidersStore() {
  let descriptors = $state<DbProviderDescriptor[]>([]);
  let error = $state('');
  /**
   * Whether a read has been started. Deliberately outside `$state`: it guards
   * against a second fetch, and a reactive read of it inside the effects that call
   * `load` would be one more dependency able to re-trigger them.
   */
  let asked = false;
  /** When the last attempt started, so a failure backs off instead of spinning. */
  let lastAttempt = 0;

  return {
    get all() { return descriptors; },
    get error() { return error; },

    /** The descriptor for one engine, or `null` before the read has landed. */
    byKind(kind: Dialect | null | undefined): DbProviderDescriptor | null {
      return descriptors.find((d) => d.kind === kind) ?? null;
    },

    /**
     * What this engine can do. `null` until the descriptors are in — callers read a
     * specific flag off it with `?? false`, so an unknown engine and an unloaded
     * store both mean "do not offer it".
     */
    capabilities(kind: Dialect | null | undefined): EngineCapabilities | null {
      return this.byKind(kind)?.capabilities ?? null;
    },

    /**
     * Read the descriptors, once.
     *
     * Idempotent, and safe to call from an effect. **Not** safe to call from a
     * getter that a `$derived` reads, which is how this was first wired and what it
     * cost: a failure re-armed the read *and* wrote `error`, `error` is `$state`,
     * the write re-rendered, the render re-read the getter, and the getter asked
     * again — an unbounded storm of RPCs at a backend that was already not
     * answering. Priming belongs in one effect, and it is in `PicusShell`.
     *
     * A failure is re-askable, because "the backend is not up yet" is the ordinary
     * reason and remembering it would keep every capability false for the session.
     * But it is re-askable on a **floor**, so that even a caller that does ask on
     * every render costs one attempt every few seconds rather than one per frame.
     */
    async load() {
      if (asked) return;
      if (Date.now() - lastAttempt < RETRY_MS) return;
      asked = true;
      lastAttempt = Date.now();
      try {
        descriptors = await listProviders();
        error = '';
      } catch (e) {
        asked = false;
        error = String(e);
      }
    },
  };
}

export const picusProvidersStore = createProvidersStore();
