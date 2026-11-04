/**
 * The icon a kind of declaration wears in the naming review — the same glyph wherever a name is shown
 * with its kind, so a `constant` is a padlock in the list, in the rules table and in the filter chips.
 */
import { Box, Braces, Hash, Lock, Package, Parentheses, Tag, Type, Variable } from 'lucide-svelte';

const ICONS: Record<string, typeof Box> = {
  type: Box,
  method: Parentheses,
  field: Braces,
  constant: Lock,
  parameter: Hash,
  local: Variable,
  'type-parameter': Type,
  'enum-constant': Tag,
  package: Package,
};

/** The icon for a target slug (`method`, `local`, …); a plain box for one nobody has drawn yet. */
export function targetIcon(target: string): typeof Box {
  return ICONS[target] ?? Box;
}
