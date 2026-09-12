/**
 * The `bennu/config.toml` document, as one store.
 *
 * Every settings page that writes a typed field of the config used to carry its own copy of the
 * same three lines — read the config, spread a patch over it, write it back — and its own snapshot
 * of the result. Two pages open at once then held two snapshots, and the second write put back what
 * the first had changed.
 *
 * So the document lives here: one snapshot, one writer, and a read-modify-write per patch so a field
 * another writer owns (autosave through {@link bennuSettingsStore}, the build type through the run
 * store) is never clobbered by a stale copy.
 */

import { getBennuConfig, setBennuConfig, type BennuConfig } from '$lib/ipc/bennu/config';

function createBennuConfigStore() {
  let cfg = $state<BennuConfig | null>(null);
  /** In flight, so several pages mounting together ask once. */
  let loading: Promise<void> | null = null;

  async function read(): Promise<void> {
    const next = await getBennuConfig().catch(() => null);
    if (next) cfg = next;
  }

  return {
    /** The config as last read — `null` until the first {@link load} answers. */
    get cfg(): BennuConfig | null {
      return cfg;
    },

    /** Read it, at most once per burst of callers. */
    async load(): Promise<void> {
      if (cfg) return;
      loading ??= read().finally(() => {
        loading = null;
      });
      await loading;
    },

    /** Re-read it from the backend, whatever is held here. */
    async reload(): Promise<void> {
      await read();
    },

    /**
     * Write a partial change over a **fresh** read, and keep what was written.
     *
     * The fresh read is the point: this modal is not the only writer, and a patch applied to a
     * snapshot taken when the dialog opened would put back every field somebody else has changed
     * since.
     */
    async patch(patch: Partial<BennuConfig>): Promise<void> {
      const current = await getBennuConfig().catch(() => null);
      if (!current) return;
      const next = { ...current, ...patch };
      cfg = next;
      await setBennuConfig(next).catch(() => {});
    },
  };
}

export const bennuConfigStore = createBennuConfigStore();
