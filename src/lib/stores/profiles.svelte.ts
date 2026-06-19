import {
  listProfiles,
  getActiveProfile,
  switchProfile,
  createProfile,
  renameProfile,
  deleteProfile,
} from '$lib/ipc/profiles';
import { listen } from '@tauri-apps/api/event';

// Profile switcher state. A profile is an isolated environment (own settings,
// plugins, repos) — see docs/profiles-and-product-config.md. Switching is a
// live operation: the backend re-resolves its caches against the new profile
// and emits `arbor://profile-switched`, on which every window reloads its
// stores wholesale (the simplest correct way to re-derive all of them).

let _list      = $state<string[]>(['default']);
let _active    = $state<string>('default');
let _ready     = $state(false);
let _switching = $state(false);

async function refresh() {
  try {
    const [list, active] = await Promise.all([listProfiles(), getActiveProfile()]);
    _list   = list;
    _active = active;
  } catch {
    // Backend not ready (dev mode) — keep the optimistic default.
  }
}

async function init() {
  await refresh();
  await listen('arbor://profile-switched', () => {
    // Re-resolve every store against the new profile by reloading the webview;
    // the backend has already swapped its in-memory caches by the time this
    // fires. Cheaper than a process restart and avoids a window flash.
    window.location.reload();
  });
  _ready = true;
}

/** Switch to `name`. On success the backend broadcasts `profile-switched`,
 *  which reloads this window — so this never resolves normally on success. */
async function switchTo(name: string) {
  if (name === _active || _switching) return;
  _switching = true;
  try {
    await switchProfile(name);
  } catch (e) {
    _switching = false;
    throw e;
  }
}

async function create(name: string) {
  await createProfile(name);
  await refresh();
}

async function rename(oldName: string, newName: string) {
  await renameProfile(oldName, newName);
  await refresh();
}

async function remove(name: string) {
  await deleteProfile(name);
  await refresh();
}

export const profileStore = {
  get list()      { return _list; },
  get active()    { return _active; },
  get ready()     { return _ready; },
  get switching() { return _switching; },
  init,
  refresh,
  switchTo,
  create,
  rename,
  remove,
};
