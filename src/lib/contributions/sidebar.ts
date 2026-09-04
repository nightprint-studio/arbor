/**
 * The `arbor:sidebar` contribution point: the mapping to a typed section, and the two
 * lookups every consumer of it needs.
 *
 * A plugin registers a panel with `arbor.ui.add_sidebar{…}` and says where it wants to
 * live (`side`, `position`) — the product puts a button on the matching rail and renders
 * the panel when it is pressed. Corvus does that, and so does Bennu.
 *
 * `enabledSidebarSections()` and `findSidebarSection()` are here rather than restated at
 * each call site because "the sections that count" is one rule — registered, from a plugin
 * that is enabled — and it was already written out four times inside Corvus's shell alone.
 * A `plugin:<name>:<id>` key is the identity a panel is addressed by across the two products
 * (rail button, active-panel state, `arbor.ui.open_panel`), so it is built here too.
 */
import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
import { pluginStore } from '$lib/stores/plugin.svelte';
import type { PluginContribution } from '$lib/types/corvus/contribution';
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

/** The `plugin:<name>:<id>` key a panel is addressed by — rail button id, active-panel
 *  state, and the pair `arbor.ui.open_panel` arrives as.
 *
 *  Typed as the template literal it actually builds, not as `string`: every panel union
 *  (`LeftPanel`, `RightPanel`, `BottomPanel`) admits a `plugin:${string}` member, so a plain
 *  `string` here made each of the six calls that dock a plugin panel a type error at the call
 *  site — for a value that was always in the union. */
export function sidebarKey(s: { plugin_name: string; id: string }): `plugin:${string}` {
  return `plugin:${s.plugin_name}:${s.id}`;
}

/** Split a `plugin:<name>:<id>` key back into its halves; `null` for anything else (a
 *  product's own panel id, or a key from a plugin that has since gone). */
export function parsePluginKey(
  key: string | null | undefined,
): { plugin_name: string; panel_id: string } | null {
  if (!key || !key.startsWith('plugin:')) return null;
  const rest  = key.slice('plugin:'.length);
  const colon = rest.indexOf(':');
  if (colon < 0) return null;
  return { plugin_name: rest.slice(0, colon), panel_id: rest.slice(colon + 1) };
}

/** Every sidebar panel registered by a plugin that is currently enabled. Reads the
 *  contribution store, so it is reactive inside a `$derived` / `$effect`. */
export function enabledSidebarSections(): PluginSidebarSection[] {
  return contributionStore.forPoint(SIDEBAR_POINT)
    .filter((c) => pluginStore.isEnabled(c.plugin_name))
    .map(parseSidebarSection);
}

/** The registration behind a `plugin:<name>:<id>` key, or `null` when there is none —
 *  a stale key (plugin disabled, panel unregistered) is not an error, it just has
 *  nothing to render. */
export function findSidebarSection(
  key: { plugin_name: string; panel_id: string } | null,
): PluginSidebarSection | null {
  if (!key) return null;
  return enabledSidebarSections()
    .find((s) => s.plugin_name === key.plugin_name && s.id === key.panel_id) ?? null;
}
