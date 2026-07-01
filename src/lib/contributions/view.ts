/**
 * Pure mapping `PluginContribution → PluginViewSection`.
 *
 * Consumers (ActivityBarLeft, AppShell, CommandPalette) read
 * `contributionStore.forPoint('arbor:view')`, filter out items from disabled
 * plugins, then run each entry through `parseViewSection` to get a typed shape.
 *
 * A view is a main-area surface (it occupies the body of the window where the
 * commit graph lives) rather than a side rail. Its body is form-DSL content
 * pushed via `arbor.ui.set_panel_content(<view_id>, …)` (the same channel
 * sidebar panels use) and rendered by the full `FormNodeRenderer`.
 */
import type { PluginContribution } from '$lib/types/corvus/contribution';
import type { PluginViewSection, ViewPlacement } from '$lib/types/plugin';

export const VIEW_POINT = 'arbor:view';

interface ViewPayload {
  label?:     string;
  icon?:      string;
  placement?: ViewPlacement;
  tooltip?:   string;
}

export function parseViewSection(c: PluginContribution): PluginViewSection {
  const p = c.payload as ViewPayload;
  return {
    plugin_name: c.plugin_name,
    id:          c.item_id,
    label:       p.label ?? c.item_id,
    icon:        p.icon,
    placement:   p.placement === 'main' ? 'main' : 'graph',
    tooltip:     p.tooltip,
  };
}
