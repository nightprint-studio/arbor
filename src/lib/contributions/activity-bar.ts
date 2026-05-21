/**
 * Pure mapping `PluginContribution → ActivityBarEntry | null`.
 *
 * Consumers (ActivityBarLeft, ActivityBarRight, CustomizeActivityBarModal,
 * RepoActions) read `contributionStore.forPoint('arbor:activitybar')`,
 * filter out items from disabled plugins, then run each entry through
 * `parseActivityBarEntry` to get a typed shape.
 */
import type { PluginContribution } from '$lib/types/contribution';
import type { ActivityBarEntry, ComboOption } from '$lib/types/plugin';

export const ACTIVITY_BAR_POINT = 'arbor:activitybar';

interface ActivityBarPayload {
  kind?:          string;
  target?:        string;
  action?:        string;
  label?:         string;
  icon?:          string;
  run_action?:    string;
  select_action?: string;
  run_icon?:      string;
  tooltip?:       string;
  variant?:       string;
  options?:       ComboOption[];
}

export function parseActivityBarEntry(c: PluginContribution): ActivityBarEntry | null {
  const p = c.payload as ActivityBarPayload;
  if (p.kind === 'separator') {
    return { kind: 'separator', plugin_name: c.plugin_name };
  }
  if (p.kind === 'combo') {
    return {
      kind:          'combo',
      plugin_name:   c.plugin_name,
      id:            c.item_id,
      run_action:    p.run_action ?? '',
      select_action: p.select_action,
      run_icon:      p.run_icon,
      tooltip:       p.tooltip,
      target:        p.target ?? 'activity_bar',
      variant:       p.variant,
      options:       p.options ?? [],
    };
  }
  if (p.kind === 'action') {
    return {
      kind:        'action',
      plugin_name: c.plugin_name,
      action:      p.action ?? '',
      label:       p.label  ?? '',
      icon:        p.icon,
    };
  }
  return null;
}
