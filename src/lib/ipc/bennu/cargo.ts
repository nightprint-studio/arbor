/**
 * Cargo IPC — the crate graph, the command catalogue, and launching one.
 *
 * Mirrors `bennu-cargo`'s own types in **snake_case**; the Rust side
 * (`crates/products/bennu/cargo/src/{workspace,commands}.rs`) is authoritative.
 *
 * ## Why the command catalogue is fetched rather than declared here
 *
 * {@link cargoCommands} returns a table the frontend could perfectly well hard-code. It does not,
 * because the per-command capability flags are what the backend uses to *build the command line* —
 * so a second copy here would eventually offer `--release` on a `cargo fmt` row that the backend
 * then drops, and the button would quietly do something other than what it says.
 *
 * Same convention as the rest of `ipc/bennu`: every call wraps its fields under `{ args: … }`.
 */

import { bennu } from '../rpc';

/** One thing a crate builds. */
export interface CargoTarget {
  /** What `--bin <name>` takes. */
  name: string;
  /** `lib` · `bin` · `example` · `test` · `bench`. */
  kind: string;
  /** Source file, relative to the crate's directory. */
  path: string;
  /** Whether the manifest declares it, as opposed to Cargo discovering it by convention. A
   *  declared target has settings to go and edit; a discovered one has no manifest entry. */
  declared: boolean;
  /** A `[lib]` that is a procedural macro. */
  proc_macro: boolean;
  /** The target does not build unless these features are on. */
  required_features: string[];
}

/** One feature of a crate. */
export interface CargoFeature {
  name: string;
  /** What turning it on turns on, verbatim (`dep:serde`, `serde/derive`, another feature). */
  enables: string[];
  /** Whether `default` reaches it, transitively. */
  default: boolean;
}

/** One crate of the workspace. */
export interface CargoCrate {
  name: string;
  /** The version, or `inherited` when it comes from `[workspace.package]` — which is a different
   *  fact from having none, and the common one. */
  version: string;
  /** Path relative to the workspace root. Empty for the root crate itself. */
  rel_path: string;
  /** Absolute path of the crate's `Cargo.toml`. */
  manifest: string;
  edition: string;
  description: string;
  is_root: boolean;
  /** `false` when the manifest says `publish = false`. */
  publish: boolean;
  targets: CargoTarget[];
  features: CargoFeature[];
  deps: number;
  dev_deps: number;
  build_deps: number;
}

/** A Cargo workspace, as its manifests describe it. */
export interface CargoWorkspace {
  root: string;
  name: string;
  /** `[workspace]` with no `[package]` — the root compiles nothing of its own. */
  virtual_manifest: boolean;
  /** Whether the root declares a `[workspace]` at all. A single-crate project is a workspace of
   *  one, which is worth saying rather than showing an empty panel. */
  is_workspace: boolean;
  edition: string;
  resolver: string;
  /** Root first, then members in declaration order. */
  crates: CargoCrate[];
  /** Manifests found but unreadable — said out loud, because a crate missing from the list is
   *  otherwise indistinguishable from one that does not exist. */
  unreadable: string[];
  /** Whether a `Cargo.lock` is next to the root manifest. */
  locked: boolean;
  /** Crate directories under the root that no `members` pattern covers. The failure is silent
   *  otherwise: the crate builds when you build it directly and is invisible to `--workspace`. */
  orphans: string[];
}

/** One cargo subcommand, with what it accepts. */
export interface CargoCommandDef {
  id: string;
  label: string;
  doc: string;
  /** Whether `-p <crate>` / `--workspace` mean anything for it. */
  scoped: boolean;
  profiled: boolean;
  featured: boolean;
  targeted: boolean;
  /** Whether arguments after `--` reach a program rather than cargo. */
  passes_args: boolean;
  /** The rustup component it needs, empty when built into cargo. */
  component: string;
  /** Whether it belongs in the panel's front row. */
  common: boolean;
}

/** What `cargo` is available, and what it can do. */
export interface CargoToolchain {
  /** `cargo --version`, verbatim. Empty when cargo could not be run at all. */
  version: string;
  components: string[];
  /** Whether `rustup` answered — the difference between "clippy is missing" and "we cannot tell".
   *  When false, every command is offered: refusing one on a guess is worse than letting it fail
   *  and say why. */
  components_known: boolean;
  toolchain: string;
}

/** Which target a cargo command is aimed at. */
export interface CargoTargetSelector {
  /** `lib` · `bin` · `example` · `test` · `bench` · `all-targets`, or empty for the default. */
  kind: string;
  /** The target's name, for the kinds that take one. */
  name: string;
}

/** One request to run a cargo command. Mirrors `bennu_cargo::commands::Invocation`. */
export interface CargoInvocation {
  command: string;
  /** `-p <name>`. Empty means the manifest in the working directory decides. */
  package: string;
  /** `--workspace`. Ignored when `package` is set. */
  workspace: boolean;
  target: CargoTargetSelector;
  release: boolean;
  /** A named `--profile`, which wins over `release`. */
  profile: string;
  features: string[];
  all_features: boolean;
  no_default_features: boolean;
  /** Extra cargo flags, already split into tokens. */
  extra: string[];
  /** Arguments after `--`. */
  args: string[];
}

/** A blank invocation — every field explicit, so a partial object can never reach the wire. */
export function emptyInvocation(command = 'check'): CargoInvocation {
  return {
    command,
    package: '',
    workspace: false,
    target: { kind: '', name: '' },
    release: false,
    profile: '',
    features: [],
    all_features: false,
    no_default_features: false,
    extra: [],
    args: [],
  };
}

/** Whether `toolchain` can run a command needing `component`. Permissive when nothing is known —
 *  see {@link CargoToolchain.components_known}. */
export function hasComponent(toolchain: CargoToolchain | null, component: string): boolean {
  if (!component) return true;
  if (!toolchain || !toolchain.components_known) return true;
  return toolchain.components.includes(component);
}

/** The crate graph. Reads manifests and the filesystem — never `cargo metadata` — so it is safe to
 *  call on every panel open. Wire: `bennu_cargo_workspace`. */
export function cargoWorkspace(root: string): Promise<CargoWorkspace> {
  return bennu('bennu_cargo_workspace', { args: { root } });
}

/** The cargo subcommands Bennu offers. Wire: `bennu_cargo_commands`. */
export function cargoCommands(): Promise<CargoCommandDef[]> {
  return bennu('bennu_cargo_commands', { args: {} });
}

/** The active toolchain. `refresh` re-probes — what to send after telling the user to install a
 *  component. Wire: `bennu_cargo_toolchain`. */
export function cargoToolchain(refresh = false): Promise<CargoToolchain> {
  return bennu('bennu_cargo_toolchain', { args: { refresh } });
}

/**
 * The command line an invocation would run.
 *
 * A round-trip for a preview, deliberately: the alternative is the editor assembling a second
 * command line to show, and the two drifting the first time a flag is added to one of them. A
 * preview that disagrees with what runs is worse than no preview. Wire: `bennu_cargo_preview`.
 */
export function cargoPreview(invocation: CargoInvocation): Promise<string> {
  return bennu('bennu_cargo_preview', { args: { invocation } });
}

/** Launch a cargo command, streaming into the Run console. Returns immediately with the handle the
 *  console correlates by; Stop and stdin work on it exactly as they do for a JVM run.
 *  Wire: `bennu_cargo_run`. */
export function cargoRun(
  root: string,
  invocation: CargoInvocation,
  opts: { workingDir?: string; env?: Record<string, string> } = {},
): Promise<{ run_id: string; main_class: string; command: string; working_dir: string }> {
  return bennu('bennu_cargo_run', {
    args: {
      root,
      invocation,
      working_dir: opts.workingDir ?? '',
      env: opts.env ?? {},
    },
  });
}

// ── crates.io ─────────────────────────────────────────────────────────────────
//
// The three calls that reach the network. All of them are answered from an on-disk cache with a TTL,
// all of them return the empty answer when the user has turned the index off (see the `[cargo]`
// config section), and none of them is on a path that blocks the editor.

/** One published version of a crate. */
export interface CrateRelease {
  version: string;
  /** Withdrawn by its author — still listed, but marked: a lockfile may pin one. */
  yanked: boolean;
  /** A pre-release (`1.0.0-rc.1`). Decided by the backend, so the frontend needs no semver parser. */
  prerelease: boolean;
  /** The features **this version** declares, `default` first. Per version rather than per crate
   *  because they change between releases. */
  features: string[];
}

/** "This dependency is behind", located in the manifest. */
export interface CargoVersionHint {
  /** The crate, by its real name (`package = "…"` resolved). */
  name: string;
  /** Byte offset of the dependency's name — where the hint is drawn. */
  offset: number;
  /** 1-based line of the dependency. */
  line: number;
  /** Byte span of the version value as written, **quotes included** — what an update replaces. */
  start: number;
  end: number;
  /** The requirement in the file. */
  current: string;
  /** The newest release on crates.io. */
  latest: string;
}

/** What `cargo add` did. */
export interface CargoAddResult {
  ok: boolean;
  /** The command line that ran, so a failure can be repeated in a terminal. */
  command: string;
  /** Cargo's own report — which version it resolved and which features it enabled. */
  output: string;
}

/** Every published version of a crate, newest first.
 *
 *  Empty for a crate that does not exist, for an unreachable index with nothing cached, and when the
 *  user has turned crates.io off — all three are states, not errors. `refresh` ignores the cache.
 *  Wire: `bennu_cargo_versions` — `{ name, refresh }`. */
export function cargoVersions(name: string, refresh = false): Promise<CrateRelease[]> {
  return bennu('bennu_cargo_versions', { args: { name, refresh } });
}

/** Which dependencies in a manifest buffer have a newer release.
 *
 *  Only crates.io dependencies with a readable requirement: a `path`, a `git` or an inherited
 *  dependency has no version here to be behind, and a deliberate pin or a range is left alone.
 *  Wire: `bennu_cargo_version_hints` — `{ file, source }`. */
export function cargoVersionHints(file: string, source: string): Promise<CargoVersionHint[]> {
  return bennu('bennu_cargo_version_hints', { args: { file, source } });
}

/** Add a dependency by running the real `cargo add`.
 *
 *  Captured rather than streamed into the Run console, unlike the other cargo commands: a build is
 *  something you watch, this is a one-line edit you want a verdict on. The caller reloads the manifest
 *  afterwards. Wire: `bennu_cargo_add`. */
export function cargoAdd(
  root: string,
  name: string,
  opts: {
    version?: string;
    features?: string[];
    noDefaultFeatures?: boolean;
    /** `'dev'` · `'build'` · `''` for a normal dependency. */
    kind?: string;
    optional?: boolean;
    /** Which workspace member to add it to (`-p`). Empty for the root manifest. */
    packageName?: string;
  } = {},
): Promise<CargoAddResult> {
  return bennu('bennu_cargo_add', {
    args: {
      root,
      name,
      version: opts.version ?? '',
      features: opts.features ?? [],
      no_default_features: opts.noDefaultFeatures ?? false,
      kind: opts.kind ?? '',
      optional: opts.optional ?? false,
      package: opts.packageName ?? '',
    },
  });
}
