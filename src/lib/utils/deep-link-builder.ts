/**
 * Build & copy shareable deep-links from in-app context.
 *
 * Pairs with `deep-link-dispatcher.svelte.ts` (the consumer side).  Every
 * builder produces a URL whose `?url=` parameter carries the active tab's
 * **remote** git URL — that's what makes the link shareable across machines.
 *
 * Most chat platforms (Google Chat, Slack, Teams, …) refuse to render
 * `arbor://…` as a clickable link, so by default Arbor emits an
 * `https://<worker_base_url>/<path>?<query>` variant. The worker (a tiny
 * Cloudflare Worker, by default) 302-redirects the request back to the
 * equivalent `arbor://` URL, where the OS hands it to Arbor. Users can
 * point `worker_base_url` at their own redirect host, or clear it to
 * fall back to raw `arbor://` URLs.
 *
 * The remote URL is fetched lazily via `listRemotes(tabId)` and cached
 * per-tab so the copy buttons don't pay an IPC round-trip on every click.
 */

import { listRemotes } from '$lib/ipc/remote';
import { getDeepLinkConfig } from '$lib/ipc/deep-link';
import { uiStore } from '$lib/stores/ui.svelte';
import type { RemoteInfo } from '$lib/types/git';

// ---------------------------------------------------------------------------
// Origin URL resolver — cached per tab id
// ---------------------------------------------------------------------------

const remoteUrlCache = new Map<string, string | null>();

/** Drop the cached origin URL for a tab — call after a remote add/edit/remove. */
export function invalidateRemoteUrlCache(tabId: string): void {
  remoteUrlCache.delete(tabId);
}

/**
 * Resolve the remote URL Arbor should embed in deep links for `tabId`.
 *
 * Preference order:
 *   1. The remote literally named `origin`
 *   2. The first remote in `listRemotes()` order
 *
 * Returns `null` (and caches it) when the repo has no remotes — UI then
 * disables the copy button rather than producing a non-shareable link.
 */
export async function getDeepLinkRemoteUrl(tabId: string): Promise<string | null> {
  if (remoteUrlCache.has(tabId)) return remoteUrlCache.get(tabId)!;
  let remotes: RemoteInfo[];
  try {
    remotes = await listRemotes(tabId);
  } catch {
    remoteUrlCache.set(tabId, null);
    return null;
  }
  const origin = remotes.find(r => r.name === 'origin') ?? remotes[0];
  const url = origin?.url?.trim() || null;
  remoteUrlCache.set(tabId, url);
  return url;
}

// ---------------------------------------------------------------------------
// URL construction
// ---------------------------------------------------------------------------

/** Discriminated union mirroring `parseDeepLink` in the dispatcher. */
export type DeepLinkSpec =
  | { kind: 'repo_open' }
  | { kind: 'commit_jump';     sha: string }
  | { kind: 'branch_checkout'; branch: string }
  | { kind: 'branch_worktree'; branch: string }
  | { kind: 'mr_open';         number: number }
  | { kind: 'pipeline_open';   runId: string };

/**
 * Build a path-and-query suffix (everything after the URI scheme + host)
 * for `spec`, embedding `remoteUrl` as the `?url=` payload. Path segments
 * (sha / branch / number / run-id) are `encodeURIComponent`'d so weird
 * names (slashes in branches, `#` in unicode commit messages, …) survive
 * the round-trip.
 */
function buildDeepLinkSuffix(spec: DeepLinkSpec, remoteUrl: string): string {
  const url = `url=${encodeURIComponent(remoteUrl)}`;
  switch (spec.kind) {
    case 'repo_open':
      return `repo/open?${url}`;
    case 'commit_jump':
      return `commit/${encodeURIComponent(spec.sha)}?${url}`;
    case 'branch_checkout':
      return `branch/${encodeURIComponent(spec.branch)}?${url}&checkout=1`;
    case 'branch_worktree':
      return `branch/${encodeURIComponent(spec.branch)}?${url}&worktree=1`;
    case 'mr_open':
      return `mr/open/${spec.number}?${url}`;
    case 'pipeline_open':
      return `pipeline/${encodeURIComponent(spec.runId)}?${url}`;
  }
}

/** Normalise a user-provided worker host: strip whitespace, leading
 *  scheme, and trailing slashes. Returns `''` when nothing usable is left,
 *  which triggers the raw-`arbor://` fallback. */
function normaliseWorkerBaseUrl(raw: string | null | undefined): string {
  const trimmed = (raw ?? '').trim();
  if (!trimmed) return '';
  return trimmed.replace(/^https?:\/\//i, '').replace(/\/+$/, '');
}

/**
 * Build the shareable URL for `spec`. When `workerBaseUrl` is non-empty,
 * the result is `https://<workerBaseUrl>/<path>?<query>` (the redirect
 * worker rewrites it back to `arbor://…`). Otherwise the function emits a
 * raw `arbor://…` URL — useful for offline / private builds, or when the
 * user wants to skip the worker.
 */
export function buildDeepLink(
  spec: DeepLinkSpec,
  remoteUrl: string,
  workerBaseUrl: string,
): string {
  const suffix = buildDeepLinkSuffix(spec, remoteUrl);
  const host   = normaliseWorkerBaseUrl(workerBaseUrl);
  return host ? `https://${host}/${suffix}` : `arbor://${suffix}`;
}

// ---------------------------------------------------------------------------
// Copy-to-clipboard convenience
// ---------------------------------------------------------------------------

/** Human-readable label for the success/failure toast. */
function specLabel(spec: DeepLinkSpec): string {
  switch (spec.kind) {
    case 'repo_open':       return 'open repository';
    case 'commit_jump':     return `commit ${spec.sha.slice(0, 8)}`;
    case 'branch_checkout': return `checkout "${spec.branch}"`;
    case 'branch_worktree': return `worktree on "${spec.branch}"`;
    case 'mr_open':         return `MR !${spec.number}`;
    case 'pipeline_open':   return `pipeline ${spec.runId}`;
  }
}

/**
 * Cached `worker_base_url` from `DeepLinkConfig`. Loaded lazily on the
 * first copy and refreshed when the user changes the value via the
 * settings panel (see `invalidateWorkerBaseUrlCache`).
 */
let workerBaseUrlCache: string | null = null;

/** Drop the cached worker host — call from the settings panel right after
 *  persisting a new `worker_base_url`, so the next copy picks it up. */
export function invalidateWorkerBaseUrlCache(): void {
  workerBaseUrlCache = null;
}

async function getWorkerBaseUrl(): Promise<string> {
  if (workerBaseUrlCache !== null) return workerBaseUrlCache;
  try {
    const cfg = await getDeepLinkConfig();
    workerBaseUrlCache = cfg.worker_base_url ?? '';
  } catch {
    workerBaseUrlCache = '';
  }
  return workerBaseUrlCache;
}

/**
 * Resolve the active tab's remote URL, build the link, copy it to the
 * clipboard, and toast the outcome.  All buttons funnel through this so
 * the no-remote / clipboard-failed paths render the same UX everywhere.
 *
 * Returns the URL on success, or `null` when the repo has no remote.
 */
export async function copyDeepLink(
  spec: DeepLinkSpec,
  tabId: string,
): Promise<string | null> {
  const remoteUrl = await getDeepLinkRemoteUrl(tabId);
  if (!remoteUrl) {
    uiStore.showToast('This repository has no remote configured', 'warning');
    return null;
  }
  const workerBaseUrl = await getWorkerBaseUrl();
  const link = buildDeepLink(spec, remoteUrl, workerBaseUrl);
  try {
    await navigator.clipboard.writeText(link);
    uiStore.showToast(`Copied deep link (${specLabel(spec)})`, 'success');
    return link;
  } catch (e) {
    uiStore.showToast(`Copy failed: ${e}`, 'error');
    return null;
  }
}
