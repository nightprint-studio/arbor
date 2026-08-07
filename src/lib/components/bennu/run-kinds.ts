/**
 * The icon for a run-configuration kind, in one place.
 *
 * Two surfaces show these — the title bar's selector and the editor's grouped list — and a
 * category that is a leaf in one and a flask in the other is two categories as far as the
 * reader is concerned. The labels live beside them in `RUN_KINDS` (the store); only the
 * icons are here, because a store should not be importing components.
 */

import { AppWindow, Leaf, FlaskConical, Package, CircleHelp } from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';

const ICONS: Record<string, unknown> = {
  application: AppWindow,
  // Spring's own mark is a leaf; it is the one kind with a symbol everyone already knows.
  springboot: Leaf,
  junit: FlaskConical,
  // A crate. Cargo's own mark is a shipping crate, and `Package` is the closest lucide has — the
  // same box the Cargo tool window and the crate rows in it use, so the category reads as one
  // thing across the three surfaces.
  cargo: Package,
};

/** The icon for `kind` — a question mark for one written by a newer Bennu, which is honest:
 *  the configuration is listed, and we cannot say what it launches. */
export function runKindIcon(kind: string): IconComponent {
  return (ICONS[kind] ?? CircleHelp) as IconComponent;
}
