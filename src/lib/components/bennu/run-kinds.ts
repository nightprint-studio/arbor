/**
 * The icon for a run configuration, in one place.
 *
 * Two surfaces show these — the title bar's selector and the editor's grouped list — and a
 * category that is a leaf in one and a flask in the other is two categories as far as the reader is
 * concerned. The labels live beside them in `RUN_KINDS` (the store); only the icons are here,
 * because a store should not be importing components.
 *
 * ## The kind is not enough
 *
 * A kind was all this used to answer, and inside the Cargo kind that made every row identical: a
 * list of `app`, `editor-app`, `stub-gen` wore the same crate three times, in a list whose only job
 * is telling them apart. What distinguishes them is not the ecosystem, it is **what the
 * configuration does** — run a binary, run the tests, check, lint, format, benchmark — so that is
 * what the icon says, and the group header above it already says Cargo.
 */

import {
  AppWindow, BadgeCheck, FileCode2, FlaskConical, Gauge, Hammer, Leaf, Package, Play, Rocket,
  Server, Sparkles, Wrench, CircleHelp,
} from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';
import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';

/** Per **kind** — the fallback when the configuration says nothing more specific. */
const KIND_ICONS: Record<string, unknown> = {
  application: AppWindow,
  // Spring's own mark is a leaf; it is the one kind with a symbol everyone already knows.
  springboot: Leaf,
  junit: FlaskConical,
  // A crate. Cargo's own mark is a shipping crate, and `Package` is the closest lucide has.
  cargo: Package,
  tomcat: Server,
};

/**
 * Per **cargo subcommand** — what the configuration actually does.
 *
 * Chosen so a glance down the list separates the things that *produce* something from the things
 * that *judge* it: a play triangle runs, a hammer builds, a badge checks, a flask tests. `cargo run`
 * gets the triangle rather than the crate, because in a list of six binaries "this one launches" is
 * the fact you are looking for.
 */
const CARGO_ICONS: Record<string, unknown> = {
  run: Play,
  test: FlaskConical,
  bench: Gauge,
  build: Hammer,
  check: BadgeCheck,
  clippy: Sparkles,
  fmt: Wrench,
  doc: FileCode2,
  install: Rocket,
};

/** The icon for `kind` alone — the coarse answer, kept for callers that have only a kind (a group
 *  header, an empty-state row). */
export function runKindIcon(kind: string): IconComponent {
  return (KIND_ICONS[kind] ?? CircleHelp) as IconComponent;
}

/**
 * The icon for one configuration — the specific answer, and what every list should use.
 *
 * Falls back to the kind's icon whenever the configuration carries nothing finer, so a shape this
 * table has never heard of degrades to "a Cargo thing" rather than to a question mark.
 */
export function runConfigIcon(config: Pick<RunConfig, 'kind' | 'cargoCommand'>): IconComponent {
  if (config.kind === 'cargo') {
    // The subcommand as written, so `cargo +nightly test` and `test --doc` both land on `test`.
    const first = (config.cargoCommand ?? '')
      .trim()
      .split(/\s+/)
      .find((w: string) => w && !w.startsWith('+'));
    const hit = first ? CARGO_ICONS[first] : undefined;
    if (hit) return hit as IconComponent;
  }
  return runKindIcon(config.kind);
}
