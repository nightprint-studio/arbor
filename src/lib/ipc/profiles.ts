import { invoke } from '@tauri-apps/api/core';

// Profile management is a keep-shell concern (filesystem + relaunch on switch),
// so these go through `invoke` directly rather than the product router.
// See docs/profiles-and-product-config.md.

/** Every profile on disk, `default` first. */
export const listProfiles = () => invoke<string[]>('list_profiles');

/** The currently active profile name. */
export const getActiveProfile = () => invoke<string>('get_active_profile');

/** Create a new empty profile (loads built-in defaults until populated). */
export const createProfile = (name: string) =>
  invoke<void>('create_profile', { name });

/** Clone a profile: recursively copy `src`'s config into a fresh `newName`. */
export const cloneProfile = (src: string, newName: string) =>
  invoke<void>('clone_profile', { src, new: newName });

/** Rename a profile; if it was active, the pointer follows it. */
export const renameProfile = (oldName: string, newName: string) =>
  invoke<void>('rename_profile', { old: oldName, new: newName });

/** Delete a profile (not the active one, not the last remaining one). */
export const deleteProfile = (name: string) =>
  invoke<void>('delete_profile', { name });

/**
 * Switch the active profile. On success the app relaunches, so this call never
 * resolves on the happy path — the webview is torn down with the process.
 */
export const switchProfile = (name: string) =>
  invoke<void>('switch_profile', { name });
