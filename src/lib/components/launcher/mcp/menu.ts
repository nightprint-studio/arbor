/**
 * The "AI access" entry of the settings menu, authored once.
 *
 * Arbor has two homes and only ever shows one of them: the Canopy launcher window
 * when products open in their own windows, and the Welcome tab when they open as
 * tabs — in that mode Canopy never appears at all. So anything that belongs to
 * "Arbor's own settings" has to be in both menus, and a menu authored twice is a
 * menu that grows an entry in one of them and quietly not in the other.
 */
import { Activity, Bot, ListChecks } from 'lucide-svelte';

import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import { mcpStore } from '$lib/stores/mcp.svelte';

/**
 * Call inside a `$derived` and spread into the menu — the label reads the live
 * endpoint status, so it doubles as the indicator that something is listening.
 */
export function mcpMenuItems(open: () => void): DropdownItem[] {
  return [
    { kind: 'separator', label: 'AI access' },
    {
      kind: 'item',
      id: 'mcp',
      label: mcpStore.status.running ? 'AI tool access — on' : 'AI tool access…',
      icon: Bot,
      onclick: open,
    },
    {
      kind: 'item',
      id: 'mcp-activity',
      label: 'AI activity…',
      icon: Activity,
      onclick: () => window.dispatchEvent(new CustomEvent('arbor:open-mcp-activity')),
    },
    {
      kind: 'item',
      id: 'mcp-tools',
      label: 'Show AI tools…',
      icon: ListChecks,
      // No callback: the modal is mounted by `GlobalOverlays` in every window and
      // listens for this event. The launcher would otherwise be the one surface that
      // cannot see the list — and in tabbed mode it is the only surface there is,
      // which is precisely when the question "what can it do" gets asked.
      onclick: () => window.dispatchEvent(new CustomEvent('arbor:open-mcp-tools')),
    },
  ];
}
