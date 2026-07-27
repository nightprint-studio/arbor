/**
 * Picus settings — the preferences that change how the product behaves.
 *
 * MOCK: held in memory for now. When `picus-be` lands these become a typed
 * `PicusConfig` persisted to `…/picus/config.toml` in the active profile, read
 * and written through `get_picus_config` / `set_picus_config` — never
 * `localStorage`, which is reserved for ephemeral UI state (panel sizes, the
 * active sidebar section).
 *
 * Every entry here corresponds to a decision the product would otherwise make
 * silently. That is the bar for adding one: a setting exists to make an
 * assumption visible and changeable, not to defer a design choice.
 */

import { DEFAULT_VERSION_TABLE, type VersionTableConfig } from '$lib/types/picus';

/** Where a generated block is inserted into a destination file. */
export type InsertionRule = 'end-of-file' | 'after-last-on-table' | 'before-final-commit';

export const INSERTION_RULE_LABELS: Record<InsertionRule, string> = {
  'end-of-file': 'At the end of the file',
  'after-last-on-table': 'After the last statement touching the same table',
  'before-final-commit': 'Before the final COMMIT',
};

function createSettingsStore() {
  // ── Project ────────────────────────────────────────────────────────────────
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

  // ── Encoding ───────────────────────────────────────────────────────────────
  /** Fallback for files the heuristics cannot decide (pure ASCII, no BOM). */
  let defaultEncoding = $state('windows-1252');
  /** Treat a pure-ASCII file as neutral and inherit the folder's encoding. */
  let inheritAsciiEncoding = $state(true);

  // ── Writing ────────────────────────────────────────────────────────────────
  /** Ask before writing to disk. Turning it off is deliberate, and rare. */
  let confirmBeforeWrite = $state(true);
  /** Copy every file to `.arbor/backup/<timestamp>/` before rewriting it. */
  let backupBeforeWrite = $state(true);
  let insertionRuleInit = $state<InsertionRule>('after-last-on-table');
  let insertionRuleUpdate = $state<InsertionRule>('end-of-file');

  // ── Generation ─────────────────────────────────────────────────────────────
  /** Lowercase identifiers when emitting PostgreSQL. */
  let lowercasePostgres = $state(true);

  // ── Queries ────────────────────────────────────────────────────────────────
  /** Rows fetched before the rest is loaded on demand. */
  let rowLimit = $state(500);

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

    setDefaultEncoding(v: string) { defaultEncoding = v; },
    setInheritAsciiEncoding(v: boolean) { inheritAsciiEncoding = v; },
    setConfirmBeforeWrite(v: boolean) { confirmBeforeWrite = v; },
    setBackupBeforeWrite(v: boolean) { backupBeforeWrite = v; },
    setInsertionRuleInit(v: InsertionRule) { insertionRuleInit = v; },
    setInsertionRuleUpdate(v: InsertionRule) { insertionRuleUpdate = v; },
    setLowercasePostgres(v: boolean) { lowercasePostgres = v; },
    setRowLimit(v: number) { rowLimit = Math.max(1, Math.min(100_000, v)); },
  };
}

export const picusSettingsStore = createSettingsStore();
