/**
 * Lookup helper for the body of a plugin-registered panel pushed via
 * `arbor.ui.set_panel_content` (which contributes to `arbor:panel-content`).
 *
 * Consumers (PluginSidebarPanel) read
 * `contributionStore.forPoint('arbor:panel-content')` and resolve the
 * matching `(plugin_name, panel_id)` entry through `findPanelContent`.
 */
import type { PluginContribution } from '$lib/types/contribution';
import type { PluginPanelContent } from '$lib/types/plugin';

export const PANEL_CONTENT_POINT = 'arbor:panel-content';

interface PanelContentPayload {
  title?:   string;
  nodes?:   unknown;
  actions?: unknown;
}

export function findPanelContent(
  items:      PluginContribution[],
  pluginName: string,
  panelId:    string,
): PluginPanelContent | null {
  const c = items.find(it => it.plugin_name === pluginName && it.item_id === panelId);
  if (!c) return null;
  const p = c.payload as PanelContentPayload;
  return {
    plugin_name: pluginName,
    panel_id:    panelId,
    title:       p.title,
    nodes:       p.nodes,
    actions:     p.actions,
  };
}
