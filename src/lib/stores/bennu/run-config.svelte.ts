/**
 * Bennu run configurations — the IntelliJ-style list of NAMED run targets for a
 * project (each: main class + program/VM args + working dir + env vars), plus the
 * notion of an ACTIVE config that the titlebar ▶ / Shift+F10 path launches.
 *
 * **Persisted per repo**, in `<root>/.arbor/bennu/config.toml` under `[run]` — a
 * per-repo preference, on the filesystem (CLAUDE.md rule 11), in bennu's own file
 * under the `.arbor/` directory the repository already has. {@link load} hydrates when a project opens;
 * every mutation funnels through `commit`, which writes back on a short debounce so
 * a keystroke in the name field doesn't mean a file write per character.
 *
 * The wire shape is snake_case ({@link RunConfigDto}); the store's is camelCase.
 * The two conversions live here and nowhere else, so no consumer has to know the
 * seam exists.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store
 * pattern). Keyed by project root via SvelteMap so `configsFor` / `activeFor`
 * stay reactive as the user switches projects.
 */

import { SvelteMap } from 'svelte/reactivity';
import { getRunConfig, setRunConfig } from '$lib/ipc/bennu';
import type { RunConfigDto, RunConfigSetDto } from '$lib/types/bennu';

/** One key/value environment-variable row. Empty keys are dropped on the way to
 *  the run flow (see {@link envRecord}); kept in the draft so a half-typed row
 *  doesn't vanish mid-edit. */
export interface EnvVar {
  key: string;
  value: string;
}

/**
 * What a configuration LAUNCHES — the category the editor and the title-bar selector group
 * by. `application` runs a `main` class; `junit` runs a test scope through the test runner.
 *
 * Kept deliberately small. A category belongs here when Bennu can actually run that thing;
 * inventing empty groups to look like IntelliJ would be a menu of dead ends.
 */
export type RunConfigKind = 'application' | 'springboot' | 'junit';

/** How much a `junit` configuration runs. */
export type TestScopeKind = 'all' | 'module' | 'class';

/**
 * The kinds, in the order they are offered and grouped. One table, read by the editor, the
 * selector and the launcher, so the three cannot come to disagree about what a category is
 * called.
 *
 * `capability` names the project capability a kind needs to be OFFERED. A Spring Boot
 * configuration on a project with no Spring is a menu entry that can only disappoint;
 * existing ones are still listed and still run, because the file may be shared with someone
 * whose checkout has more in it.
 */
export const RUN_KINDS: {
  id: RunConfigKind;
  label: string;
  newName: string;
  capability?: 'spring';
}[] = [
  { id: 'application', label: 'Application', newName: 'Application' },
  { id: 'springboot', label: 'Spring Boot', newName: 'Application', capability: 'spring' },
  { id: 'junit', label: 'JUnit', newName: 'Tests' },
];

/** Whether `s` names a kind this build can run. Anything else came from a newer Bennu: it is
 *  listed but not launchable, because the file is shared and dropping what we don't
 *  understand would silently delete someone's configuration. */
export function isRunKind(s: string): s is RunConfigKind {
  return s === 'application' || s === 'springboot' || s === 'junit';
}

/** Whether a kind launches a JVM main class (and so wears the class / args / environment
 *  form). Spring Boot is an application with one extra field, not a different mechanism. */
export function isJvmKind(kind: string): boolean {
  return kind === 'application' || kind === 'springboot';
}

/** The category label for a kind — falls back to the raw string for one we don't know. */
export function runKindLabel(kind: string): string {
  return RUN_KINDS.find((k) => k.id === kind)?.label ?? kind;
}

/**
 * A single named run configuration. Maps 1:1 to a `[[bennu.run.configs]]` TOML entry.
 * Program & VM args are stored as the raw single-line strings the user types
 * (shell-style, space-separated) — splitting into an argv is done at launch so
 * the round-trip to TOML is lossless and the editor stays a plain Input.
 */
export interface RunConfig {
  /** Stable id (never shown) — the map/selection key. */
  id: string;
  name: string;
  /** The Maven module this configuration belongs to, relative to the project root. Empty =
   *  the root module. What decides the run classpath on a reactor. */
  module: string;
  /** The category. Held as the wire string, not the union, so a kind written by a newer
   *  Bennu survives a round-trip through this one instead of being rewritten to something
   *  it is not. */
  kind: string;
  /** `application` only — the fully-qualified class to launch. */
  mainClass: string;
  /** Program arguments (passed after the main class), raw single-line string. */
  programArgs: string;
  /** JVM arguments (`-Xmx…`, `-D…`), raw single-line string. */
  vmArgs: string;
  /** Working directory; empty = project root. */
  workingDir: string;
  env: EnvVar[];
  /** `springboot` only — the active profiles as Spring spells them (`dev,local`). Becomes
   *  `-Dspring.profiles.active=…` at launch. */
  profiles: string;
  /** `junit` only — how much to run. */
  testScope: TestScopeKind;
  /** `junit` only — the module directory or the class selector, per {@link testScope}. */
  testTarget: string;
  /** Hold the VM before `main` when this configuration is launched with 🐞.
   *
   *  Off by default. It is the only way to stop in start-up code (a static initializer, a
   *  Spring context being built), and it means every debug launch begins frozen until you
   *  press Resume — which is not what the launch you press fifty times a day wants. */
  debugSuspend: boolean;
  /**
   * Which Maven scopes reach the **run** classpath: `runtime` (the default), `compile`,
   * `test`, or `''` for every scope.
   *
   * The index deliberately resolves *every* scope, because you edit tests and completion has
   * to see their dependencies. Launching with that same classpath hands the JVM test- and
   * provided-scoped libraries Maven would never supply — and a `@ConditionalOnClass` guarding
   * a bean on one of them then fires here and nowhere else, so the application refuses to
   * start in the IDE while `mvn spring-boot:run` is perfectly happy.
   *
   * `runtime` is what `spring-boot:run` and a packaged application see, so it is the default.
   * The others are here because the exceptions are real — a launcher that wants a test-scoped
   * H2 or a provided servlet API is a legitimate thing to want, and it should be a choice
   * rather than a reason to stop using the run panel.
   */
  classpathScope: string;
}

/** The per-project run-config bundle — the ordered list plus which one is active. */
export interface RunConfigSet {
  configs: RunConfig[];
  /** Id of the active config (what ▶ Run launches), or null if none/empty. */
  activeId: string | null;
}

let idSeq = 0;
/** A stable id. Generated here and persisted verbatim — the backend never re-assigns one,
 *  so the active pointer survives a restart. */
function nextId(): string {
  idSeq += 1;
  return `rc-${Date.now().toString(36)}-${idSeq}`;
}

/** Store shape → wire shape. */
function toDto(c: RunConfig): RunConfigDto {
  return {
    id: c.id,
    name: c.name,
    kind: c.kind,
    module: c.module,
    main_class: c.mainClass,
    program_args: c.programArgs,
    vm_args: c.vmArgs,
    working_dir: c.workingDir,
    env: c.env.map((e) => ({ key: e.key, value: e.value })),
    profiles: c.profiles,
    test_scope: c.testScope,
    test_target: c.testTarget,
    debug_suspend: c.debugSuspend,
    classpath_scope: c.classpathScope,
  };
}

/** Wire shape → store shape. Tolerant of a hand-edited TOML with fields missing: a config
 *  with no `vm_args` key is a config with no VM args, not a crash on opening the editor.
 *  A missing `kind` is an application — that is what every configuration was before kinds. */
function fromDto(d: RunConfigDto): RunConfig {
  const scope = d.test_scope ?? '';
  return {
    id: d.id || nextId(),
    name: d.name ?? '',
    kind: d.kind || 'application',
    module: d.module ?? '',
    mainClass: d.main_class ?? '',
    programArgs: d.program_args ?? '',
    vmArgs: d.vm_args ?? '',
    workingDir: d.working_dir ?? '',
    env: (d.env ?? []).map((e) => ({ key: e.key ?? '', value: e.value ?? '' })),
    profiles: d.profiles ?? '',
    testScope: scope === 'module' || scope === 'class' ? scope : 'all',
    testTarget: d.test_target ?? '',
    debugSuspend: d.debug_suspend ?? false,
    // A configuration written before scopes existed has no key, and it was getting the index's
    // every-scope classpath. It now gets `runtime` — deliberately a change in behaviour on the
    // next launch, because the old one was the wrong classpath.
    classpathScope: d.classpath_scope ?? 'runtime',
  };
}

/** A blank configuration of `kind`. */
export function emptyConfig(name = 'Unnamed', kind: RunConfigKind = 'application'): RunConfig {
  return {
    id: nextId(),
    name,
    kind,
    module: '',
    mainClass: '',
    programArgs: '',
    vmArgs: '',
    workingDir: '',
    env: [],
    profiles: '',
    testScope: 'all',
    testTarget: '',
    debugSuspend: false,
    classpathScope: 'runtime',
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

/** How long after the last edit the bundle is written. Long enough that typing a name is one
 *  write, short enough that closing the window right after an edit still saves it. */
const SAVE_DEBOUNCE_MS = 400;

function createRunConfigStore() {
  // Keyed by project root, hydrated by `load(root)` when a project opens.
  const sets = new SvelteMap<string, RunConfigSet>();
  // Roots already hydrated (or being hydrated) — `load` is called from an effect that may
  // re-run, and re-reading the file would throw away edits made since.
  const hydrated = new Set<string>();
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function ensure(root: string): RunConfigSet {
    let set = sets.get(root);
    if (!set) {
      set = { configs: [], activeId: null };
      sets.set(root, set);
    }
    return set;
  }

  /** Write the bundle for `root` to `<root>/.arbor/bennu/config.toml`, debounced. Best-effort: a
   *  failed write leaves the session's configs intact and is not worth a modal — the next
   *  edit retries. */
  function persist(root: string) {
    const pending = saveTimers.get(root);
    if (pending) clearTimeout(pending);
    saveTimers.set(
      root,
      setTimeout(() => {
        saveTimers.delete(root);
        const snap = sets.get(root);
        if (!snap) return;
        const dto: RunConfigSetDto = {
          configs: snap.configs.map(toDto),
          active_id: snap.activeId,
        };
        void setRunConfig(root, dto).catch(() => {
          /* best-effort — the in-memory bundle still applies this session */
        });
      }, SAVE_DEBOUNCE_MS),
    );
  }

  /** Re-store `set` under `root` so the SvelteMap notices the reference change and
   *  re-runs dependent `$derived`. Svelte's reactivity tracks map get/set, not deep
   *  mutation of the stored object. */
  function commit(root: string, set: RunConfigSet) {
    sets.set(root, { configs: set.configs.slice(), activeId: set.activeId });
    persist(root);
  }

  return {
    /**
     * Hydrate `root`'s configurations from its `.arbor/bennu/config.toml`. Called when a project
     * opens. Idempotent per root: a second call is a no-op rather than a re-read, because
     * the effect that calls it re-runs on things that are not "a different project" and
     * re-reading would silently drop edits made since.
     */
    async load(root: string): Promise<void> {
      if (!root || hydrated.has(root)) return;
      hydrated.add(root);
      try {
        const dto = await getRunConfig(root);
        const configs = (dto.configs ?? []).map(fromDto);
        // An active id pointing at a config that is no longer there (hand-edited file)
        // would leave ▶ Run doing nothing with no way to see why.
        const activeId =
          dto.active_id && configs.some((c) => c.id === dto.active_id)
            ? dto.active_id
            : (configs[0]?.id ?? null);
        sets.set(root, { configs, activeId });
      } catch {
        // No file, no section, unreadable — an empty list, which is what a fresh project has.
        hydrated.delete(root);
      }
    },

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

    /**
     * The configurations for `root` grouped by category, in {@link RUN_KINDS} order, with
     * any unknown kind last. Empty groups are dropped — a category with nothing in it is a
     * heading pointing at nothing.
     *
     * One grouping, used by both the editor's list and the title-bar selector, so the two
     * always show the same shape.
     */
    groupedFor(root: string): { kind: string; label: string; configs: RunConfig[] }[] {
      const configs = sets.get(root)?.configs ?? [];
      const order = RUN_KINDS.map((k) => k.id as string);
      const kinds = [...new Set(configs.map((c) => c.kind))].sort((a, b) => {
        const ia = order.indexOf(a);
        const ib = order.indexOf(b);
        return (ia < 0 ? order.length : ia) - (ib < 0 ? order.length : ib);
      });
      return kinds.map((kind) => ({
        kind,
        label: runKindLabel(kind),
        configs: configs.filter((c) => c.kind === kind),
      }));
    },

    /** Create a new config of `kind` (appended, auto-named uniquely) and return its id. If
     *  it's the first config for the project it becomes active. */
    create(root: string, kind: RunConfigKind = 'application', seed?: Partial<RunConfig>): string {
      const set = ensure(root);
      const fallback = RUN_KINDS.find((k) => k.id === kind)?.newName ?? 'Unnamed';
      const base = emptyConfig(uniqueName(set.configs, seed?.name ?? fallback), kind);
      const cfg: RunConfig = { ...base, ...seed, id: base.id, name: base.name, kind };
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

    /**
     * Replace the fields of `id` with `patch` (name / module / main class / args / dir /
     * env / profiles / test scope). Every editor keystroke funnels here so a single change
     * persists.
     *
     * The configuration is REPLACED, not mutated. It used to be `Object.assign(cfg, patch)`,
     * and the editor's fields are read through a `$derived` that ends in
     * `configs.find(c => c.id === selectedId)` — which, after an in-place mutation, returns
     * the very same object. A derived whose value is `===` its previous one propagates
     * nothing, so the change reached the store and the disk but never the screen: typing in a
     * field still looked right (the DOM already held what you typed), while anything set
     * *programmatically* — the profile picker, the main-class picker — appeared to do
     * nothing at all.
     */
    update(root: string, id: string, patch: Partial<Omit<RunConfig, 'id'>>) {
      const set = ensure(root);
      const idx = set.configs.findIndex((c) => c.id === id);
      if (idx === -1) return;
      set.configs[idx] = { ...set.configs[idx], ...patch };
      commit(root, set);
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
