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

function createProvidersStore() {
  let descriptors = $state<DbProviderDescriptor[]>([]);
  let error = $state('');
  /**
   * Whether a read has been started. Deliberately outside `$state`: it guards
   * against a second fetch, and a reactive read of it inside the effects that call
   * `load` would be one more dependency able to re-trigger them.
   */
  let asked = false;

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

    /** Read the descriptors, once. Idempotent and safe to call from an effect. */
    async load() {
      if (asked) return;
      asked = true;
      try {
        descriptors = await listProviders();
        error = '';
      } catch (e) {
        // Re-askable: a backend that was not up yet is the ordinary reason, and
        // remembering the failure would keep every capability false for the session.
        asked = false;
        error = String(e);
      }
    },
  };
}

export const picusProvidersStore = createProvidersStore();
