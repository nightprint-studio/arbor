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
 * The **project** settings (declared encoding, line ending, version table) are
 * still in memory. They deliberately do NOT belong in the per-profile config: they
 * describe the repository, so a colleague opening it must inherit them. They land
 * in the project's own config when the script half of `picus-be` does.
 */

import {
  getPicusConfig,
  setPicusConfig,
  type PicusConfig,
} from '$lib/ipc/picus/config';
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
    get defaultEncoding() { return defaultEncoding; },
    get inheritAsciiEncoding() { return inheritAsciiEncoding; },
    get confirmBeforeWrite() { return confirmBeforeWrite; },
    get backupBeforeWrite() { return backupBeforeWrite; },
    get insertionRuleInit() { return insertionRuleInit; },
    get insertionRuleUpdate() { return insertionRuleUpdate; },
    get lowercasePostgres() { return lowercasePostgres; },
    get rowLimit() { return rowLimit; },

    // Project settings are in-memory only until the project config lands — no
    // `persist()` here, deliberately: writing them to the profile would make the
    // same repository behave differently per user.
    setProjectEncoding(v: string) { projectEncoding = v; },
    setProjectEol(v: 'CRLF' | 'LF') { projectEol = v; },

    /** Patch one field of the version-table configuration. */
    setVersionTable(patch: Partial<VersionTableConfig>) {
      versionTable = { ...versionTable, ...patch };
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
