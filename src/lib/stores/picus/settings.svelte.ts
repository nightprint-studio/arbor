/**
 * Picus settings — the preferences that change how the product behaves.
 *
 * Every entry here corresponds to a decision the product would otherwise make
 * silently. That is the bar for adding one: a setting exists to make an
 * assumption visible and changeable, not to defer a design choice.
 *
 * ## Persistence
 *
 * The **product** settings (encoding fallbacks, write guards, emission defaults,
 * query row limit) are real: they hydrate from `…/picus/config.toml` in the active
 * profile via `get_picus_config` and write back on every change through
 * `set_picus_config`. Never `localStorage`, which is reserved for ephemeral UI
 * state (panel sizes, the active sidebar section).
 *
 * The **project** settings (declared encoding, line ending, version table, which
 * rules to run) live in the repository's own `.arbor/picus/project.toml`. They
 * deliberately do NOT belong in the per-profile config: they describe the
 * repository, so a colleague opening it must inherit them — a version table that
 * was per-user would mean the same update script is guarded for one person and
 * unguarded for another.
 *
 * They are also the one part of this store that is **not** written as you type.
 * That file is committed into somebody's repository, so it is saved on an
 * explicit Save and not before — the same rule that governs every other write
 * Picus makes to a user's tree.
 */

import {
  getPicusConfig,
  setPicusConfig,
  type PicusConfig,
} from '$lib/ipc/picus/config';
import {
  projectSettings as readProjectSettings,
  setProjectSettings,
  type InitialisationModel,
  type ProjectSettings,
} from '$lib/ipc/picus/project';
import { DEFAULT_VERSION_TABLE, type VersionTableConfig } from '$lib/types/picus';

/** Where a generated block is inserted into a destination file. */
export type InsertionRule = 'end-of-file' | 'after-last-on-table' | 'before-final-commit';

export const INSERTION_RULE_LABELS: Record<InsertionRule, string> = {
  'end-of-file': 'At the end of the file',
  'after-last-on-table': 'After the last statement touching the same table',
  'before-final-commit': 'Before the final COMMIT',
};

/** The wire strings the BE accepts, so an unknown value from disk is ignored
 *  rather than fed back into the UI as a broken radio group. */
const INSERTION_RULES = Object.keys(INSERTION_RULE_LABELS) as InsertionRule[];

function asInsertionRule(v: string, fallback: InsertionRule): InsertionRule {
  return (INSERTION_RULES as string[]).includes(v) ? (v as InsertionRule) : fallback;
}

/** Coalescing window for writes, so dragging a number field doesn't hammer the BE. */
const PERSIST_DEBOUNCE_MS = 300;

function createSettingsStore() {
  // ── Project (in memory — belongs to the project's own config) ───────────────
  /**
   * The encoding this project's scripts are written in. Distinct from the
   * per-file detection: this is the declared intent the detector is compared
   * against, so a file coming back in another encoding is a finding (`ENC001`)
   * rather than the new normal.
   */
  let projectEncoding = $state('windows-1252');
  /** The line ending new content is written with. */
  let projectEol = $state<'CRLF' | 'LF'>('CRLF');
  /** Where the installed version lives, and what to stamp on upgrade. */
  let versionTable = $state<VersionTableConfig>({ ...DEFAULT_VERSION_TABLE });
  /**
   * What the initialisation folders are, relative to the updates.
   *
   * Not derivable from the SQL — it is a fact about how the team works — and it
   * decides which half of the propagation check is even a question. See the
   * `InitialisationModel` doc in `ipc/picus/project.ts`.
   */
  let initialisation = $state<InitialisationModel>('cumulative');
  /** Rule ids this repository has decided not to run. */
  let disabledRules = $state<string[]>([]);
  /** The project settings differ from what is on disk. */
  let projectDirty = $state(false);
  let projectSaving = $state(false);

  /** The project half of this store, in the shape the backend takes. */
  function projectSnapshot(): ProjectSettings {
    return {
      encoding: projectEncoding,
      eol: projectEol,
      versionTable: versionTable.table,
      versionColumn: versionTable.versionColumn,
      dateColumn: versionTable.dateColumn ?? '',
      versionFilter: versionTable.filter,
      initialisation,
      disabledRules,
    };
  }

  function applyProject(s: ProjectSettings) {
    projectEncoding = s.encoding;
    projectEol = s.eol === 'LF' ? 'LF' : 'CRLF';
    versionTable = {
      table: s.versionTable,
      versionColumn: s.versionColumn,
      // The wire has no room for the difference between "absent" and "named the
      // empty string", and only one of them is a real answer.
      dateColumn: s.dateColumn ? s.dateColumn : null,
      filter: s.versionFilter,
    };
    initialisation = s.initialisation;
    disabledRules = [...s.disabledRules];
    projectDirty = false;
  }

  // ── Encoding (persisted) ───────────────────────────────────────────────────
  /** Fallback for files the heuristics cannot decide (pure ASCII, no BOM). */
  let defaultEncoding = $state('windows-1252');
  /** Treat a pure-ASCII file as neutral and inherit the folder's encoding. */
  let inheritAsciiEncoding = $state(true);

  // ── Writing (persisted) ────────────────────────────────────────────────────
  /** Ask before writing to disk. Turning it off is deliberate, and rare. */
  let confirmBeforeWrite = $state(true);
  /** Copy every file to `.arbor/backup/<timestamp>/` before rewriting it. */
  let backupBeforeWrite = $state(true);
  let insertionRuleInit = $state<InsertionRule>('after-last-on-table');
  let insertionRuleUpdate = $state<InsertionRule>('end-of-file');

  // ── Generation (persisted) ─────────────────────────────────────────────────
  /** Lowercase identifiers when emitting PostgreSQL. */
  let lowercasePostgres = $state(true);

  // ── Queries (persisted) ────────────────────────────────────────────────────
  /** Rows fetched per page. */
  let rowLimit = $state(500);

  /** The wire payload — the persisted half of this store, and only that half. */
  function snapshot(): PicusConfig {
    return {
      encoding: { default: defaultEncoding, inherit_ascii: inheritAsciiEncoding },
      writing: {
        confirm_before_write: confirmBeforeWrite,
        backup_before_write: backupBeforeWrite,
      },
      generation: {
        insertion_rule_init: insertionRuleInit,
        insertion_rule_update: insertionRuleUpdate,
        lowercase_postgres: lowercasePostgres,
      },
      queries: { row_limit: rowLimit },
    };
  }

  let persistTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Write the product settings to `…/picus/config.toml`, coalesced.
   * Fire-and-forget: a persistence hiccup (backend still coming up) must never
   * block the UI — the in-memory value still applies for this session, and the
   * next change retries.
   */
  function persist() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      void setPicusConfig(snapshot()).catch(() => {
        /* non-critical — picus-be absent or not attached yet */
      });
    }, PERSIST_DEBOUNCE_MS);
  }

  return {
    get projectEncoding() { return projectEncoding; },
    get projectEol() { return projectEol; },
    get versionTable() { return versionTable; },
    get initialisation() { return initialisation; },
    get disabledRules() { return disabledRules; },
    get projectDirty() { return projectDirty; },
    get projectSaving() { return projectSaving; },
    get defaultEncoding() { return defaultEncoding; },
    get inheritAsciiEncoding() { return inheritAsciiEncoding; },
    get confirmBeforeWrite() { return confirmBeforeWrite; },
    get backupBeforeWrite() { return backupBeforeWrite; },
    get insertionRuleInit() { return insertionRuleInit; },
    get insertionRuleUpdate() { return insertionRuleUpdate; },
    get lowercasePostgres() { return lowercasePostgres; },
    get rowLimit() { return rowLimit; },

    // Project settings are never written to the profile, and never written as you
    // type: they describe the repository, they land in a file that gets committed,
    // and `saveProject` is the only thing that touches disk.
    setProjectEncoding(v: string) { projectEncoding = v; projectDirty = true; },
    setProjectEol(v: 'CRLF' | 'LF') { projectEol = v; projectDirty = true; },

    /** Patch one field of the version-table configuration. */
    setVersionTable(patch: Partial<VersionTableConfig>) {
      versionTable = { ...versionTable, ...patch };
      projectDirty = true;
    },

    setInitialisation(v: InitialisationModel) { initialisation = v; projectDirty = true; },

    /** Switch one rule off, or back on. */
    setRuleEnabled(rule: string, enabled: boolean) {
      const without = disabledRules.filter((r) => r.toUpperCase() !== rule.toUpperCase());
      disabledRules = enabled ? without : [...without, rule.toUpperCase()].sort();
      projectDirty = true;
    },

    /** Does this repository run this rule? */
    ruleEnabled(rule: string): boolean {
      return !disabledRules.some((r) => r.toUpperCase() === rule.toUpperCase());
    },

    /**
     * Read the project settings from `.arbor/picus/project.toml`.
     *
     * Keeps the current values on failure rather than resetting to defaults: a
     * backend that is not up yet must not look like a project that declares
     * nothing, because "declares nothing" is a meaningful state here — it is what
     * switches the version guards off.
     */
    async loadProject(root: string) {
      if (!root) return;
      try {
        applyProject(await readProjectSettings(root));
      } catch {
        /* picus-be absent / not attached yet — what is in memory stands */
      }
    },

    /**
     * Write them back. Returns the backend's reply so the caller can replace its
     * project tree with it, or `null` with the reason on refusal.
     */
    async saveProject(root: string): Promise<{ configPath: string; problems: string[] } | string> {
      if (!root) return 'This connection has no script repository attached.';
      projectSaving = true;
      try {
        const confirmed = await setProjectSettings(root, projectSnapshot());
        // Re-read rather than assuming: the backend normalises what it was given
        // (trims names, folds rule ids, drops the ones that name no rule), and the
        // form has to show what was actually written.
        applyProject(await readProjectSettings(root));
        return { configPath: confirmed.configPath, problems: confirmed.problems };
      } catch (e) {
        return String(e);
      } finally {
        projectSaving = false;
      }
    },

    /**
     * Guess the version table from a live schema.
     *
     * The heuristic is deliberately dull and reported rather than applied
     * silently: a table whose name looks version-ish, its first short text
     * column as the version, and a date column ONLY if the table actually has
     * one — plenty of projects never stamp a date, and inventing the column
     * would emit an UPDATE that fails.
     */
    detectVersionTable(
      tables: { name: string; columns: { name: string; type: string }[] }[],
    ): VersionTableConfig | null {
      const candidate =
        tables.find((t) => /^(VERSION|VERSIONE)/i.test(t.name)) ??
        tables.find((t) => /VERSION|VERSIONE/i.test(t.name));
      if (!candidate) return null;

      const isText = (type: string) => /CHAR|TEXT|VARCHAR/i.test(type);
      const isDate = (type: string) => /DATE|TIME/i.test(type);

      const versionColumn =
        candidate.columns.find((c) => /VERSION|VERSIONE/i.test(c.name) && isText(c.type)) ??
        candidate.columns.find((c) => isText(c.type));
      if (!versionColumn) return null;

      const dateColumn = candidate.columns.find((c) => isDate(c.type)) ?? null;

      return {
        table: candidate.name,
        versionColumn: versionColumn.name,
        dateColumn: dateColumn ? dateColumn.name : null,
        filter: '',
      };
    },

    setDefaultEncoding(v: string) { defaultEncoding = v; persist(); },
    setInheritAsciiEncoding(v: boolean) { inheritAsciiEncoding = v; persist(); },
    setConfirmBeforeWrite(v: boolean) { confirmBeforeWrite = v; persist(); },
    setBackupBeforeWrite(v: boolean) { backupBeforeWrite = v; persist(); },
    setInsertionRuleInit(v: InsertionRule) { insertionRuleInit = v; persist(); },
    setInsertionRuleUpdate(v: InsertionRule) { insertionRuleUpdate = v; persist(); },
    setLowercasePostgres(v: boolean) { lowercasePostgres = v; persist(); },
    setRowLimit(v: number) { rowLimit = Math.max(1, Math.min(100_000, v)); persist(); },

    /**
     * Hydrate the product settings from `…/picus/config.toml`. Call at window boot
     * and again on `arbor://picus-be-up` — the backend spawn races window creation,
     * so the first call can land before the backend is routable. Keeps the current
     * values on failure rather than resetting to defaults.
     */
    async loadConfig() {
      let cfg: PicusConfig;
      try {
        cfg = await getPicusConfig();
      } catch {
        return; // picus-be absent / not attached yet — defaults stand
      }
      defaultEncoding = cfg.encoding.default;
      inheritAsciiEncoding = cfg.encoding.inherit_ascii;
      confirmBeforeWrite = cfg.writing.confirm_before_write;
      backupBeforeWrite = cfg.writing.backup_before_write;
      insertionRuleInit = asInsertionRule(
        cfg.generation.insertion_rule_init,
        'after-last-on-table',
      );
      insertionRuleUpdate = asInsertionRule(cfg.generation.insertion_rule_update, 'end-of-file');
      lowercasePostgres = cfg.generation.lowercase_postgres;
      rowLimit = Math.max(1, Math.min(100_000, cfg.queries.row_limit));
    },
  };
}

export const picusSettingsStore = createSettingsStore();
