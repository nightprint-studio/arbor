/**
 * Launcher version sources — the seam between the Canopy and "what version is
 * installed / what's the latest" for each product.
 *
 * Today Corvus, Sitta and Merula are all surfaces of the **same Arbor binary**,
 * so they share its version (from `tauri.conf.json`, via `get_app_info`), and
 * there's no per-product update channel yet — every product reports up-to-date.
 *
 * These two functions are deliberately the only place that knowledge lives, so a
 * real per-product version + release feed plugs in here without touching the
 * launcher UI: when `fetchLatestVersions` starts returning a version newer than
 * `fetchInstalledVersions`, the Canopy node automatically flips to the "update"
 * state (see `canopy.ts` `decorate`).
 */
import { getAppInfo } from '$lib/ipc/app';

/** Installed version per product id. Currently the shared Arbor version. */
export async function fetchInstalledVersions(ids: string[]): Promise<Record<string, string>> {
  const { version } = await getAppInfo();
  return Object.fromEntries(ids.map((id) => [id, version]));
}

/**
 * Latest available version per product id. No release feed exists yet, so this
 * equals the installed version (everything up-to-date). When a per-product
 * update channel lands, query it here and return the newer version to light up
 * the "Da aggiornare" state — the rest of the launcher already reacts to it.
 */
export async function fetchLatestVersions(ids: string[]): Promise<Record<string, string>> {
  return fetchInstalledVersions(ids);
}
