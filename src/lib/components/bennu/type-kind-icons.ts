/**
 * The icon per **type** kind in a palette-style picker of types.
 *
 * Shared so a record looks the same in every one of them: the move-member target picker and the class
 * picker both draw from here, and a second table is how the two would start to disagree.
 */

import { Box, Braces, Command, Hexagon, Rows3 } from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';

const ICONS: Record<string, IconComponent> = {
  class: Box,
  interface: Braces,
  enum: Hexagon,
  record: Rows3,
  annotation: Braces,
};

/** A kind slug to its icon — the neutral one for a kind this table does not know. For
 *  `CommandPaletteShell`'s `iconResolver`. */
export function typeKindIcon(kind: string): IconComponent {
  return ICONS[kind] ?? Command;
}
