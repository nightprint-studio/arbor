/**
 * Cross-product "recently opened" history — the list Canopy shows.
 *
 * Shell-owned rather than per-product: the products keep their histories in
 * three different places, two of which need that product's backend running, and
 * the launcher can't start three backends to draw a list. Each product reports
 * here as it opens something.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface RecentProject {
  /** Canopy product id that opened it (`corvus` / `bennu` / `merula`). */
  product:   string;
  /** Absolute repository / project root. */
  path:      string;
  /** Display name as the product knows it. */
  name:      string;
  /** Unix seconds of the last open. */
  opened_at: number;
  /** Pinned to the top of the welcome page. */
  pinned?:   boolean;
}

/** Record an open. Re-opening an entry moves it to the top rather than duplicating it. */
export const recordRecentProject = (product: string, path: string, name: string) =>
  invoke<void>('record_recent_project', { product, path, name });

/** The whole history, newest first. */
export const listRecentProjects = () =>
  invoke<RecentProject[]>('list_recent_projects');

/** Drop one entry — "remove from recents". */
export const forgetRecentProject = (product: string, path: string) =>
  invoke<void>('forget_recent_project', { product, path });

/** Pin or unpin one entry: a pinned project stays at the top of the welcome page. */
export const pinRecentProject = (product: string, path: string, pinned: boolean) =>
  invoke<void>('pin_recent_project', { product, path, pinned });

/** Where in the project a request should land — passed along with the path, consumed once. A product acts on
 *  the parts it understands (Corvus: `commit`, `branch`). */
export interface OpenContext { file?: string; commit?: string; branch?: string }

/** Pull the context parked with this product's open-intent, if any. */
export const takeOpenContext = (product: string) =>
  invoke<OpenContext | null>('take_open_context', { product });

/**
 * Park "open this project" for a product. Pair it with `openProduct`: the
 * product's shell pulls the path as it mounts, so the click lands on the
 * project rather than on an empty product.
 */
export const setOpenIntent = (product: string, path: string, context?: OpenContext) =>
  invoke<void>('set_open_intent', { product, path, context: context ?? null });

/** Pull this product's parked project path, if any. Returns null once consumed. */
export const takeOpenIntent = (product: string) =>
  invoke<string | null>('take_open_intent', { product });

/** Broadcast when an open-intent is parked — for products already running,
 *  whose shell has long since done its mount-time pull. */
export const OPEN_INTENT_EVENT = 'arbor://open-intent';

export interface OpenIntentPayload { product: string; path: string; context?: OpenContext | null }

/**
 * Run `open` whenever this product is asked to open a project — once as the
 * shell mounts (the parked intent) and then on every later request. Returns a
 * disposer. The intent is consumed either way, so it can never fire twice.
 */
export function onOpenIntent(product: string, open: (path: string, context?: OpenContext) => void): () => void {
  void takeOpenIntent(product).then(async (p) => { if (p) open(p, (await takeOpenContext(product).catch(() => null)) ?? undefined); }).catch(() => {});
  let un: (() => void) | null = null;
  let disposed = false;
  void listen<OpenIntentPayload>(OPEN_INTENT_EVENT, (e) => {
    if (e.payload?.product !== product) return;
    // Consume the parked copy so a later mount doesn't re-open it.
    void takeOpenIntent(product).catch(() => {});
    void takeOpenContext(product).catch(() => {});
    open(e.payload.path, e.payload.context ?? undefined);
  }).then((off) => {
    if (disposed) off();
    else un = off;
  }).catch(() => {});
  return () => { disposed = true; un?.(); };
}
