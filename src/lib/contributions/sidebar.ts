/**
 * Pure mapping `PluginContribution → PluginSidebarSection`.
 *
 * Consumers (ActivityBarLeft, ActivityBarRight, CustomizeActivityBarModal,
 * AppShell, PluginTreeSidebar) read
 * `contributionStore.forPoint('arbor:sidebar')`, filter out items from
 * disabled plugins, then run each entry through `parseSidebarSection` to
 * get a typed shape.
 */
import type { PluginContribution } from '$lib/types/contribution';
import type { PluginSidebarSection, PluginSidebarSearch } from '$lib/types/plugin';

export const SIDEBAR_POINT = 'arbor:sidebar';

interface SidebarPayload {
  action?:      string;
  label?:       string;
  icon?:        string;
  collapsable?: boolean;
  side?:        'left' | 'right';
  position?:    'top' | 'bottom';
  tooltip?:     string;
  kind?:        'form' | 'tree';
  search?:      unknown;
}

function parseSearch(raw: unknown): PluginSidebarSearch | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const r = raw as Record<string, unknown>;
  const modesRaw = Array.isArray(r.modes) ? r.modes : [];
  const modes = modesRaw.filter((m): m is 'local' | 'remote' => m === 'local' || m === 'remote');
  if (modes.length === 0) return undefined;
  const def = (r.default === 'local' || r.default === 'remote')
    ? (modes.includes(r.default as 'local' | 'remote') ? r.default as 'local' | 'remote' : modes[0])
    : modes[0];
  return {
    modes,
    default:            def,
    remote_action:      typeof r.remote_action      === 'string' ? r.remote_action      : undefined,
    placeholder_local:  typeof r.placeholder_local  === 'string' ? r.placeholder_local  : undefined,
    placeholder_remote: typeof r.placeholder_remote === 'string' ? r.placeholder_remote : undefined,
    wildcard_hint:      typeof r.wildcard_hint      === 'boolean' ? r.wildcard_hint     : modes.includes('remote'),
  };
}

export function parseSidebarSection(c: PluginContribution): PluginSidebarSection {
  const p = c.payload as SidebarPayload;
  return {
    plugin_name: c.plugin_name,
    id:          c.item_id,
    action:      p.action ?? `panel:open:${c.item_id}`,
    label:       p.label  ?? c.item_id,
    icon:        p.icon,
    collapsable: !!p.collapsable,
    side:        p.side ?? 'right',
    position:    p.position ?? 'top',
    tooltip:     p.tooltip,
    kind:        p.kind ?? 'form',
    search:      parseSearch(p.search),
  };
}
