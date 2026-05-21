/**
 * Shortcut helpers shared across menus, tooltips, and command-palette rows.
 *
 * Always resolve through the live `keybindingsStore` so user remaps in
 * Settings → Keybindings flow into every consumer without code changes.
 * For inline rendering inside markup, prefer `<Kbd action="..." />`.
 * `tooltipWithShortcut` exists for native `title=` attributes that can't
 * embed elements.
 */
import { keybindingsStore } from '$lib/stores/keybindings.svelte';
import { formatBinding, type Keybinding } from '$lib/utils/keybindings';
import type { TooltipInput } from '$lib/stores/tooltip.svelte';

/** Live shortcut for a built-in action id, or `null` if unbound. */
export function shortcutFor(action: string | null | undefined): string | null {
  if (!action) return null;
  const b = keybindingsStore.getBinding(action);
  if (!b || !b.key) return null;
  return formatBinding(b);
}

/** Format an explicit binding (e.g. plugin-registered) — graceful empty. */
export function shortcutForBinding(b: Keybinding | null | undefined): string | null {
  if (!b || !b.key) return null;
  return formatBinding(b);
}

/**
 * Append a parenthesised shortcut to a tooltip label, IntelliJ-style:
 *   "Open repository (Ctrl+O)"
 * Returns the label unchanged when the action has no binding.
 */
export function tooltipWithShortcut(
  label: string,
  action: string | null | undefined,
): string {
  const sc = shortcutFor(action);
  return sc ? `${label} (${sc})` : label;
}

/**
 * Build a `use:tooltip` input from a label + action id. When the action has a
 * binding, the shortcut is rendered as styled <kbd> chips (instead of
 * parenthesised text). Falls back to a plain string when unbound.
 */
export function tooltipForAction(
  label: string,
  action: string | null | undefined,
): TooltipInput {
  const sc = shortcutFor(action);
  return sc ? { content: label, shortcut: sc } : label;
}
