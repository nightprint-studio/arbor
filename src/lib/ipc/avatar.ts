import { invoke } from '@tauri-apps/api/core';

/** Ask the backend to resolve an avatar URL for `email` via the
 *  GitProvider bound to `tabId`'s repo. Returns null when no provider,
 *  no token, or no match — caller falls back to a generated initials
 *  avatar. Never throws (errors collapse to null on the backend). */
export async function resolveAvatarForEmail(tabId: string, email: string): Promise<string | null> {
  try {
    const url = await invoke<string | null>('resolve_avatar_for_email', { tabId, email });
    return url ?? null;
  } catch {
    return null;
  }
}
