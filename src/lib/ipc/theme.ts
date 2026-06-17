import { invoke } from '@tauri-apps/api/core';
import { platform } from './rpc';
import type { Theme } from '$lib/types/theme';

export function listCustomThemes(): Promise<Theme[]> {
  return platform('list_custom_themes');
}

export function getActiveThemeId(): Promise<string> {
  return platform('get_active_theme_id');
}

export function setActiveThemeId(id: string): Promise<void> {
  return platform('set_active_theme_id', { id });
}

export function saveCustomTheme(theme: Theme): Promise<void> {
  return platform('save_custom_theme', { theme });
}

export function deleteCustomTheme(id: string): Promise<void> {
  return platform('delete_custom_theme', { id });
}

/** Tell the backend that the active theme just changed (or that a plugin
 *  applied an in-memory token overlay). The backend fans out the
 *  `on_theme_changed` hook to every loaded plugin. `vars` is the merged
 *  effective stylesheet — active theme first, plugin overlays on top.
 *  `source` lets handlers ignore changes they triggered themselves. */
export function notifyThemeChanged(
  themeId:   string,
  themeName: string,
  vars:      Record<string, string>,
  source:    'user' | 'plugin' | 'init',
): Promise<void> {
  return invoke('notify_theme_changed', { themeId, themeName, vars, source });
}
