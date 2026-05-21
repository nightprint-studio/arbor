import { invoke } from '@tauri-apps/api/core';
import type { DeepLinkConfig, DeepLinkLookup } from '../types/deep-link';

/**
 * Look up a repo by its remote git URL using the backend's fuzzy canonical
 * key matcher.  Used by the deep-link dispatcher to decide between
 * switch / open-here / clone-prompt.
 */
export const findRepoByRemoteUrl = (url: string) =>
  invoke<DeepLinkLookup>('find_repo_by_remote_url', { url });

/**
 * Tell the backend its `arbor://deep-link` listener is mounted — drains the
 * cold-start URL buffer and switches to direct-emit mode.  Call exactly
 * once, from `AppShell.onMount`, AFTER `listen('arbor://deep-link', …)`.
 */
export const deepLinkReady = () =>
  invoke<void>('deep_link_ready');

export const getDeepLinkConfig = () =>
  invoke<DeepLinkConfig>('get_deep_link_config');

export const setDeepLinkConfig = (config: DeepLinkConfig) =>
  invoke<void>('set_deep_link_config', { config });
