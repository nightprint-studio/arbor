/**
 * Bennu run configurations — the IntelliJ-style list of NAMED run targets for a
 * project (each: main class + program/VM args + working dir + env vars), plus the
 * notion of an ACTIVE config that the titlebar ▶ / Shift+F10 path launches.
 *
 * SEAM — IN-MEMORY ONLY (MOCK persistence). Per CLAUDE.md rule 11 run configs are
 * a per-repo preference and must live on the filesystem, NOT localStorage. Until
 * the backend serves a per-repo `[bennu.run]` config section + `get_bennu_run_config`
 * / `set_bennu_run_config` IPC, this holds everything in a rune keyed by project
 * root for the session. The {@link RunConfig} shape below is deliberately the shape
 * the future TOML `[[bennu.run.configurations]]` array maps to 1:1 (env as a
 * key/value table, args as string lists) — wiring it up is: replace `persist(root)`
 * to call `set_bennu_run_config(root, snapshot(root))` (debounced) and add a
 * `loadConfig(root)` invoked when a project opens, exactly like the other config
 * stores. The consumer surface (getters/methods) stays identical.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store
 * pattern). Keyed by project root via SvelteMap so `configsFor` / `activeFor`
 * stay reactive as the user switches projects.
 */

import { SvelteMap } from 'svelte/reactivity';

/** One key/value environment-variable row. Empty keys are dropped on the way to
 *  the run flow (see {@link envRecord}); kept in the draft so a half-typed row
 *  doesn't vanish mid-edit. */
export interface EnvVar {
  key: string;
  value: string;
}

/**
 * A single named run configuration. Maps 1:1 to a future
 * `[[bennu.run.configurations]]` TOML entry:
 *   name / main_class / program_args (list) / vm_args (list) /
 *   working_dir / env (table).
 * Program & VM args are stored as the raw single-line strings the user types
 * (shell-style, space-separated) — splitting into an argv is done at launch so
 * the round-trip to TOML is lossless and the editor stays a plain Input.
 */
export interface RunConfig {
  /** Stable id (never shown) — the map/selection key. */
  id: string;
  name: string;
  mainClass: string;
  /** Program arguments (passed after the main class), raw single-line string. */
  programArgs: string;
  /** JVM arguments (`-Xmx…`, `-D…`), raw single-line string. */
  vmArgs: string;
  /** Working directory; empty = project root. */
  workingDir: string;
  env: EnvVar[];
}

/** The per-project run-config bundle — the ordered list plus which one is active.
 *  This is the shape a future `set_bennu_run_config(root, …)` would persist. */
export interface RunConfigSet {
  configs: RunConfig[];
  /** Id of the active config (what ▶ Run launches), or null if none/empty. */
  activeId: string | null;
}

let idSeq = 0;
/** Session-unique id. MOCK — the BE would assign/keep stable ids across restarts. */
function nextId(): string {
  idSeq += 1;
  return `rc-${Date.now().toString(36)}-${idSeq}`;
}

/** A blank config with a sensible name. Free-text main class for now (discovery is
 *  a BE follow-up). */
export function emptyConfig(name = 'Unnamed'): RunConfig {
  return {
    id: nextId(),
    name,
    mainClass: '',
    programArgs: '',
    vmArgs: '',
    workingDir: '',
    env: [],
  };
}

/** Split a raw shell-style arg string into an argv. Whitespace-separated, honours
 *  single/double quotes so `-Dfoo="a b"` stays one token. Kept here (not the
 *  consumer) so the run path and any preview share one splitter. */
export function splitArgs(raw: string): string[] {
  const out: string[] = [];
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw)) !== null) {
    out.push(m[1] ?? m[2] ?? m[3] ?? '');
  }
  return out;
}

/** Collapse the env-var rows into a record, dropping rows with an empty key and
 *  trimming keys (values are passed verbatim). Later duplicate keys win. */
export function envRecord(env: EnvVar[]): Record<string, string> {
  const rec: Record<string, string> = {};
  for (const { key, value } of env) {
    const k = key.trim();
    if (k) rec[k] = value;
  }
  return rec;
}

function createRunConfigStore() {
  // MOCK — session-only, keyed by project root. Replace with values hydrated from
  // `get_bennu_run_config(root)` when the per-repo `[bennu.run]` config lands.
  const sets = new SvelteMap<string, RunConfigSet>();

  function ensure(root: string): RunConfigSet {
    let set = sets.get(root);
    if (!set) {
      set = { configs: [], activeId: null };
      sets.set(root, set);
    }
    return set;
  }

  /** MOCK persistence — no-op today. Wire to `set_bennu_run_config(root,
   *  snapshot(root))` when the typed per-repo `[bennu.run]` config lands (rule 11).
   *  Every mutation funnels here so the wiring is a one-line change. */
  function persist(root: string) {
    // MOCK — in-memory only. Future: void setBennuRunConfig(root, snapshot(root)).catch(() => {});
    void root;
  }

  /** Re-store `set` under `root` so the SvelteMap notices the reference change and
   *  re-runs dependent `$derived`. Svelte's reactivity tracks map get/set, not deep
   *  mutation of the stored object. */
  function commit(root: string, set: RunConfigSet) {
    sets.set(root, { configs: set.configs.slice(), activeId: set.activeId });
    persist(root);
  }

  return {
    /** The ordered configs for `root` (empty array if none). */
    configsFor(root: string): RunConfig[] {
      return sets.get(root)?.configs ?? [];
    },

    /** The active config for `root`, or null. This is what ▶ Run / Shift+F10 runs. */
    activeFor(root: string): RunConfig | null {
      const set = sets.get(root);
      if (!set || !set.activeId) return null;
      return set.configs.find((c) => c.id === set.activeId) ?? null;
    },

    /** The active config's id for `root`, or null. */
    activeIdFor(root: string): string | null {
      return sets.get(root)?.activeId ?? null;
    },

    /** Create a new config (appended, auto-named uniquely) and return its id. If it's
     *  the first config for the project it becomes active. */
    create(root: string, seed?: Partial<RunConfig>): string {
      const set = ensure(root);
      const base = emptyConfig(uniqueName(set.configs, seed?.name ?? 'Application'));
      const cfg: RunConfig = { ...base, ...seed, id: base.id, name: base.name };
      set.configs.push(cfg);
      if (!set.activeId) set.activeId = cfg.id;
      commit(root, set);
      return cfg.id;
    },

    /** Duplicate `id` (name gets a " copy" suffix, uniquified) and return the new id. */
    duplicate(root: string, id: string): string | null {
      const set = ensure(root);
      const src = set.configs.find((c) => c.id === id);
      if (!src) return null;
      const copy: RunConfig = {
        ...src,
        id: nextId(),
        name: uniqueName(set.configs, `${src.name} copy`),
        env: src.env.map((e) => ({ ...e })),
      };
      const idx = set.configs.indexOf(src);
      set.configs.splice(idx + 1, 0, copy);
      commit(root, set);
      return copy.id;
    },

    /** Delete `id`. If it was active, the active pointer moves to the neighbour (or
     *  null when the list empties). Returns the id that should now be selected. */
    remove(root: string, id: string): string | null {
      const set = ensure(root);
      const idx = set.configs.findIndex((c) => c.id === id);
      if (idx === -1) return set.activeId;
      set.configs.splice(idx, 1);
      const next = set.configs[idx] ?? set.configs[idx - 1] ?? null;
      if (set.activeId === id) set.activeId = next?.id ?? null;
      commit(root, set);
      return next?.id ?? null;
    },

    /** Mark `id` as the active config (what ▶ Run launches). No-op if unknown. */
    setActive(root: string, id: string) {
      const set = ensure(root);
      if (!set.configs.some((c) => c.id === id)) return;
      set.activeId = id;
      commit(root, set);
    },

    /** Replace the fields of `id` with `patch` (name / main class / args / dir / env).
     *  Every editor keystroke funnels here so a single change persists. */
    update(root: string, id: string, patch: Partial<Omit<RunConfig, 'id'>>) {
      const set = ensure(root);
      const cfg = set.configs.find((c) => c.id === id);
      if (!cfg) return;
      Object.assign(cfg, patch);
      commit(root, set);
    },

    /** Full snapshot for `root` — the payload a future `set_bennu_run_config` sends. */
    snapshot(root: string): RunConfigSet {
      const set = sets.get(root);
      return set
        ? { configs: set.configs.map((c) => ({ ...c, env: c.env.map((e) => ({ ...e })) })), activeId: set.activeId }
        : { configs: [], activeId: null };
    },
  };
}

/** Append a numeric suffix until `name` is unique among `configs`. */
function uniqueName(configs: RunConfig[], name: string): string {
  const taken = new Set(configs.map((c) => c.name));
  if (!taken.has(name)) return name;
  let n = 2;
  while (taken.has(`${name} (${n})`)) n += 1;
  return `${name} (${n})`;
}

export const bennuRunConfigStore = createRunConfigStore();
