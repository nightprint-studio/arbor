import { invoke } from '@tauri-apps/api/core';
import { platform } from './rpc';
import type { DeepLinkConfig, DeepLinkLookup } from '../types/deep-link';

/**
 * Look up a repo by its remote git URL using the backend's fuzzy canonical
 * key matcher.  Used by the deep-link dispatcher to decide between
 * switch / open-here / clone-prompt.
 */
export const findRepoByRemoteUrl = (url: string) =>
  platform<DeepLinkLookup>('find_repo_by_remote_url', { url });

/**
 * Tell the backend its `arbor://deep-link` listener is mounted — drains the
 * cold-start URL buffer and switches to direct-emit mode.  Call exactly
 * once, from `AppShell.onMount`, AFTER `listen('arbor://deep-link', …)`.
 */
export const deepLinkReady = () =>
  invoke<void>('deep_link_ready');

/**
 * Dispatch an `arbor://…` deep link the user typed MANUALLY into the File
 * Explorer address bar. Brings the main window forward and emits the trusted
 * `arbor://deep-link-manual` channel: the dispatcher treats it as explicit
 * intent and skips the enable gates (the per-action confirm still applies).
 * Works from the standalone explorer window too (no dispatcher of its own).
 */
export const dispatchDeepLink = (url: string) =>
  invoke<void>('dispatch_deep_link', { url });

export const getDeepLinkConfig = () =>
  platform<DeepLinkConfig>('get_deep_link_config');

export const setDeepLinkConfig = (config: DeepLinkConfig) =>
  platform<void>('set_deep_link_config', { config });
