/**
 * The **Profile** submenu, for any product titlebar.
 *
 * A profile is an isolated environment — its own settings, plugins, repos and per-product state
 * under `arbor/profiles/<name>/` (see `docs/profiles-and-product-config.md`). Which one is active is
 * a *window-independent* fact, so every product window that has a settings menu should be able to
 * read it and switch it: a product you can only reach the switcher from Corvus for is a product you
 * cannot tell which profile it is writing into.
 *
 * Here rather than in each titlebar because it was already written twice by the second product that
 * wanted it, and the list has real behaviour in it (the active tick, the manage/new split) that must
 * not drift between two copies.
 */
import { User, UserCog, Plus } from 'lucide-svelte';
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import { profileStore } from '$lib/stores/profiles.svelte';

/**
 * Quick-switch rows for every profile on disk, then the two doors into the manager.
 *
 * `onManage` opens `ProfileManagerModal` — the host owns it, because a modal belongs to the window
 * that renders it rather than to a menu builder. Both rows lead to the same modal: "New profile…"
 * is the verb people look for, and hiding creation inside a manager they have to discover is how a
 * one-profile install stays a one-profile install.
 *
 * Call inside a `$derived` so the tick follows the active profile.
 */
export function profileMenuItems(onManage: () => void): DropdownItem[] {
  return [
    ...profileStore.list.map((name) => ({
      kind: 'item' as const,
      id: `profile:${name}`,
      label: name,
      icon: User,
      active: profileStore.active === name,
      onclick: () => void profileStore.switchTo(name),
    })),
    { kind: 'separator' as const },
    { kind: 'item' as const, id: 'new-profile', label: 'New profile…', icon: Plus, onclick: onManage },
    { kind: 'item' as const, id: 'manage-profiles', label: 'Manage profiles…', icon: UserCog, onclick: onManage },
  ];
}
