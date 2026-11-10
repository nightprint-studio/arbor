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
  Server, Sparkles, Wrench, CircleHelp, Terminal, SquareTerminal, Zap, Plug,
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
  script: SquareTerminal,
  // A socket going into a socket: the debugger plugging into a JVM that is already there.
  attach: Plug,
};

/** Per **script extension** — what interprets the file, so a list of deploy scripts separates the
 *  PowerShell ones from the bash ones at a glance. */
const SCRIPT_ICONS: Record<string, unknown> = {
  sh: SquareTerminal,
  bash: SquareTerminal,
  ps1: Zap,
  bat: Terminal,
  cmd: Terminal,
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

/**
 * The colour of each **kind**, from the theme's own accents — so a themed palette recolours them and
 * nothing here is a hex. One hue per ecosystem, which is what lets a list of mixed configurations be
 * read by colour before it is read by name: Spring is green, a plain JVM application blue, tests
 * purple, Tomcat amber, Cargo rust, scripts teal, an attach (not a launch) violet.
 */
const KIND_COLORS: Record<string, string> = {
  application: 'var(--info)',
  springboot: 'var(--success)',
  junit: 'var(--color-tag)',
  cargo: 'var(--color-file-deleted)',
  tomcat: 'var(--warning)',
  script: 'var(--color-reflog)',
  attach: 'var(--color-workspace)',
};

/**
 * Inside the Cargo kind the subcommand decides, for the reason it decides the icon: a list of crates
 * all in one colour is the monochrome list this exists to replace. The same split as the icons —
 * what *produces* (run, build, install) against what *judges* (test, check, lint).
 */
const CARGO_COLORS: Record<string, string> = {
  run: 'var(--success)',
  test: 'var(--color-tag)',
  bench: 'var(--warning)',
  build: 'var(--warning)',
  check: 'var(--color-reflog)',
  clippy: 'var(--info)',
  fmt: 'var(--color-reflog)',
  doc: 'var(--info)',
  install: 'var(--color-workspace)',
};

/** The Cargo subcommand a configuration runs, as written — `cargo +nightly test` and `test --doc` both
 *  read as `test`. */
function cargoSubcommand(config: Pick<RunConfig, 'cargoCommand'>): string | undefined {
  return (config.cargoCommand ?? '')
    .trim()
    .split(/\s+/)
    .find((w: string) => w && !w.startsWith('+'));
}

/** The icon for `kind` alone — the coarse answer, kept for callers that have only a kind (a group
 *  header, an empty-state row). */
export function runKindIcon(kind: string): IconComponent {
  return (KIND_ICONS[kind] ?? CircleHelp) as IconComponent;
}

/** The colour for `kind` alone — what a group header wears. A kind this table has never heard of is
 *  muted rather than borrowing another kind's hue. */
export function runKindColor(kind: string): string {
  return KIND_COLORS[kind] ?? 'var(--text-muted)';
}

/** The colour for one configuration: its kind's, refined by the Cargo subcommand where there is one.
 *  Pair with [`runConfigIcon`] — the two answer the same question at the same grain. */
export function runConfigColor(config: Pick<RunConfig, 'kind' | 'cargoCommand'>): string {
  if (config.kind === 'cargo') {
    const sub = cargoSubcommand(config);
    const hit = sub ? CARGO_COLORS[sub] : undefined;
    if (hit) return hit;
  }
  return runKindColor(config.kind);
}

/**
 * The icon for one configuration — the specific answer, and what every list should use.
 *
 * Falls back to the kind's icon whenever the configuration carries nothing finer, so a shape this
 * table has never heard of degrades to "a Cargo thing" rather than to a question mark.
 */
export function runConfigIcon(
  config: Pick<RunConfig, 'kind' | 'cargoCommand'> & Partial<Pick<RunConfig, 'scriptFile'>>,
): IconComponent {
  if (config.kind === 'script') {
    const ext = (config.scriptFile ?? '').split('.').pop()?.toLowerCase() ?? '';
    const hit = SCRIPT_ICONS[ext];
    if (hit) return hit as IconComponent;
  }
  if (config.kind === 'cargo') {
    const first = cargoSubcommand(config);
    const hit = first ? CARGO_ICONS[first] : undefined;
    if (hit) return hit as IconComponent;
  }
  return runKindIcon(config.kind);
}
