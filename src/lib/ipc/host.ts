/**
 * The RPC helper for **the backend that hosts this window's plugins**.
 *
 * Calls used to name `corvus` outright. That was true when Corvus was the only product with
 * a plugin host, and became wrong the moment Bennu got one: a window asking corvus-be about
 * its own plugins gets `unknown command`, or nothing at all when corvus-be is not running.
 *
 * Lives here rather than inside one IPC module because more than one of them needs it — the
 * plugin host itself and the contribution registry — and a second copy is a second place for
 * a product to be forgotten.
 *
 * Falls back to `corvus` from a surface that hosts no plugins (the launcher, the welcome
 * tab), which is where none of this is reachable from anyway.
 */
import { getCurrentWindow } from '@tauri-apps/api/window';
import { rpc, type Program } from './rpc';
import { surfaceStore } from '$lib/stores/surfaces.svelte';
import { currentProduct } from '$lib/utils/products';

export const host = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> => {
  let product: Program | null = null;
  try {
    // `ProductId` and `Program` are the same set of names for the products that host
    // plugins; the cast is where the two vocabularies meet rather than a claim about either.
    product = currentProduct(
      getCurrentWindow().label,
      surfaceStore.inContainer,
      surfaceStore.active,
    ) as Program | null;
  } catch {
    // No window (a test, an SSR pass): the caller is not a product window either.
  }
  return rpc<R>(product ?? 'corvus', method, params);
};
