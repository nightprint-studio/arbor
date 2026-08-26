/**
 * Bennu naming-convention IPC — the per-repo `[naming]` section, and the catalog a settings screen
 * draws itself from.
 *
 * The catalog is what makes the UI data-driven: packs, targets and conventions all come from the
 * BE, so a language pack added in Rust appears as a new column with no change here. Routes through
 * the generic `bennu(...)` bridge.
 */

import { bennu } from '../rpc';
import type { RenameEdit, RenameFileMove } from './nav';

/**
 * How a name is spelled. The value is its own example — `"camelCase"` *is* what camelCase looks
 * like — so a dropdown needs no second column and a config file explains itself.
 *
 * `"any"` is the off switch, and the default for every target.
 */
export type NamingConvention =
  | 'any'
  | 'camelCase'
  | 'PascalCase'
  | 'UPPER_SNAKE_CASE'
  | 'snake_case'
  | 'lowercase';

/** The kind of declaration a convention applies to. Mirrors the BE `Target`. */
export type NamingTarget =
  | 'type'
  | 'method'
  | 'field'
  | 'constant'
  | 'parameter'
  | 'local'
  | 'type-parameter'
  | 'enum-constant'
  | 'package';

/** One pack's rules: a convention per target. An absent target means `"any"`. */
export type NamingRules = Partial<Record<NamingTarget, NamingConvention>>;

/**
 * A rule set that applies only to the paths it names.
 *
 * Only the targets it lists are replaced; the rest still come from {@link NamingConfig.rules}.
 * That is what separates it from `ignore`: a test tree can free up method names — they are mixed
 * camelCase and snake_case by convention there — while its type and constant rules stay in force.
 */
export interface NamingOverride {
  /** A label, for this list only. Free text; it never affects matching. */
  name: string;
  /** Project-relative path globs. An override with no path claims nothing. */
  paths: string[];
  /** The conventions this override replaces, per language pack id. */
  rules: Record<string, NamingRules>;
}

/** The `[naming]` section of `<repo>/.arbor/bennu/config.toml`. */
export interface NamingConfig {
  /** Master switch. Off by default — a project opts in. */
  enabled: boolean;
  /** Project-relative path globs (`*`, `?`, `**`) that are skipped entirely. */
  ignore: string[];
  /** Rules per language pack id. */
  rules: Record<string, NamingRules>;
  /** Path-scoped rule sets, in order — a later match wins. */
  overrides: NamingOverride[];
}

/**
 * Where a pack's declarations come from.
 *
 * `grammar` — parsed by Bennu itself, so **every** declaration is visible, locals and parameters
 * included. `symbols` — taken from a language server's document outline, which lists types and
 * their members and nothing else: those two targets can never fire, and the pack says so through
 * {@link NamingPack.supported}.
 */
export type NamingSource = 'grammar' | 'symbols';

/** One language pack, as the settings screen needs it. */
export interface NamingPack {
  id: string;
  label: string;
  /** Extensions it claims, without the dot. */
  extensions: string[];
  /** What "Use the standard convention" fills in. Offered, never applied on its own. */
  standard: NamingRules;
  source: NamingSource;
  /** The targets this pack can actually report. Anything else is greyed out rather than offered
   *  as a rule that would silently never fire. */
  supported: NamingTarget[];
  /** Whether the open project actually contains a file this pack claims. Project Configuration is
   *  a screen about *this* project — a pure-Java tree is not asked about TypeScript. */
  present: boolean;
}

/** One configurable target plus how to name it in a UI. */
export interface NamingTargetInfo {
  id: NamingTarget;
  label: string;
  /** Whether renaming it can only ever touch the file it is declared in — which is what decides
   *  whether its fix applies straight away or through the rename preview. */
  fileLocal: boolean;
}

/** Everything the settings screen renders from, in one round-trip. */
export interface NamingCatalog {
  packs: NamingPack[];
  targets: NamingTargetInfo[];
  conventions: NamingConvention[];
}

/** One name a bulk fix would change. */
export interface RenamedName {
  file: string;
  /** 1-based line of the declaration. One file can hold several with the same name. */
  line: number;
  from: string;
  to: string;
  /** The target slug (`method`, `local`, …). */
  target: NamingTarget;
  /** The file this one rename also has to move, or `null` — renaming a public top-level type
   *  without its file leaves code that does not compile. Skipped when the name is unticked. */
  file_rename: RenameFileMove | null;
  /** The edits THIS rename contributes, and only those — so the review can drop individual
   *  names and apply the union of what is left. */
  edits: RenameEdit[];
}

/** One name a bulk fix would NOT change, and why. */
export interface FixRefusal {
  file: string;
  /** 1-based line of the declaration. */
  line: number;
  name: string;
  reason: string;
}

/**
 * What a bulk fix would do. Nothing is written — the editor applies the edits of the names still
 * selected in the review, so one Undo takes the whole thing back.
 */
export interface NamingFixPlan {
  /** Every name the fix would change, each carrying its own edits. There is deliberately no flat
   *  pool of edits beside this: the review drops individual names, and two lists that have to
   *  agree about which edits belong to which name is one list too many. */
  renamed: RenamedName[];
  refused: FixRefusal[];
  /** The distinct files the edits touch — not the files scanned: renaming a method edits its
   *  callers, wherever they live. */
  files: string[];
  /** Whether a project-wide scan stopped at its file cap. */
  capped: boolean;
  /** Whether the user stopped it. What is here is still valid — it is simply not everything. */
  cancelled: boolean;
}

/**
 * Plan the fix for every violation in `file` (with its live buffer in `source`), or — with no
 * `file` — for the whole project. Wire: `bennu_naming_fix_plan`.
 */
export function namingFixPlan(
  root: string,
  file?: string,
  source?: string,
): Promise<NamingFixPlan> {
  return bennu('bennu_naming_fix_plan', {
    args: { root, file: file ?? null, source: source ?? null },
  });
}

/**
 * A progress tick while a fix is being planned (`arbor://bennu/naming-fix-progress`).
 *
 * `phase` is `"reading"` (project sources being scanned) or `"planning types"` — the two halves
 * cost differently and a bar that jumps from one to the other with no label reads as stuck.
 */
export interface FixProgress {
  root: string;
  phase: string;
  done: number;
  total: number;
}

/** Ask the backend to stop planning a fix. Wire: `bennu_cancel_naming_fix`. */
export function cancelNamingFix(root: string): Promise<void> {
  return bennu('bennu_cancel_naming_fix', { args: { root, file: null, source: null } });
}

/** The default section — what a project that never configured naming has. */
export function emptyNamingConfig(): NamingConfig {
  return { enabled: false, ignore: [], rules: {}, overrides: [] };
}

/** Read `[naming]` for the project at `root`. Wire: `bennu_get_naming_config`. */
export function getNamingConfig(root: string): Promise<NamingConfig> {
  return bennu('bennu_get_naming_config', { args: { root } });
}

/** Persist `[naming]`, leaving every other section intact. Wire: `bennu_set_naming_config`. */
export function setNamingConfig(root: string, config: NamingConfig): Promise<void> {
  return bennu('bennu_set_naming_config', { args: { root, config } });
}

/**
 * The packs, targets and conventions. Wire: `bennu_naming_catalog`.
 *
 * `root` only decides each pack's {@link NamingPack.present} flag; without it every pack reads as
 * present, which is what a caller asking in the abstract wants.
 */
export function namingCatalog(root?: string): Promise<NamingCatalog> {
  return bennu('bennu_naming_catalog', { args: { root: root ?? null } });
}
