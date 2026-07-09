import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { projectStore } from './project.svelte';

/** The context needed to fetch a decompiled tab's real sources: the ORIGIN file/buffer/type the
 *  go-to was issued from (so the backend resolves the same imports), plus whether a download is
 *  offered (a stub for a third-party dependency). Keyed by the decompiled view's on-disk path. */
interface DecompiledCtx {
  originFile: string;
  originSource: string;
  name: string;
  canDownload: boolean;
}

/** Tracks the open decompiled-stub tabs and drives the "Download sources" banner: it remembers the
 *  origin context per decompiled path, reflects an in-flight download, and — on the backend's
 *  `arbor://bennu/sources-ready` — reloads the tab (stub → real source) and hides the banner. */
function createDecompiledStore() {
  const tabs = new SvelteMap<string, DecompiledCtx>();
  const downloading = new SvelteSet<string>();
  let attached = false;
  let unlisten: UnlistenFn | null = null;

  return {
    /** Record that `decompiledPath` is a decompiled view produced from `ctx`'s origin file. */
    register(decompiledPath: string, ctx: DecompiledCtx) {
      tabs.set(decompiledPath, ctx);
    },
    /** The download context for the tab at `path`, if it's a tracked decompiled view. */
    ctx(path: string | null): DecompiledCtx | undefined {
      return path ? tabs.get(path) : undefined;
    },
    /** Whether a "Download sources" fetch is in flight for `path`. */
    isDownloading(path: string | null): boolean {
      return path ? downloading.has(path) : false;
    },
    /** Mark a download started for `path` (optimistic UI: disables the button). */
    markDownloading(path: string) {
      downloading.add(path);
    },
    /** Clear the in-flight flag for `path` (on an immediate request failure). */
    clearDownloading(path: string) {
      downloading.delete(path);
    },

    async attach(): Promise<UnlistenFn> {
      if (attached) return () => {};
      attached = true;
      unlisten = await listen<{ path: string; ok: boolean }>(
        'arbor://bennu/sources-ready',
        (e) => {
          const { path, ok } = e.payload;
          if (!path) return;
          downloading.delete(path); // spinner off either way
          if (!ok) return; // failure — a toast already explained; keep the banner for a retry
          const ctx = tabs.get(path);
          if (ctx) tabs.set(path, { ...ctx, canDownload: false }); // banner gone
          void projectStore.reload(path); // stub → real source
        },
      );
      return () => {
        unlisten?.();
        attached = false;
      };
    },
  };
}

export const decompiledStore = createDecompiledStore();
