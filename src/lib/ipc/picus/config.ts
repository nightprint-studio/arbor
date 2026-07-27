/**
 * Picus product-config IPC — the typed per-profile `…/picus/config.toml` the studio
 * persists (encoding fallbacks, write guards, emission defaults, query row limit).
 * Round-trips through the generic `picus(...)` rpc bridge to the `get_picus_config`
 * / `set_picus_config` handlers in `picus-be`.
 *
 * The shape mirrors the BE `PicusConfig` field-for-field, in snake_case — it crosses
 * the wire verbatim. The settings store keeps the camelCase surface the UI reads.
 *
 * NOT here, on purpose: a script project's own settings (declared encoding, line
 * ending, version table). Those belong to the project so a colleague opening the
 * same repository inherits them, and land in the project's config when the script
 * half of the backend does. See `docs/picus-design.md`.
 */

import { picus } from '../rpc';

/** How undecidable file encodings are resolved. Detection itself is per file. */
export interface PicusEncodingConfig {
  /** Fallback for files the heuristics cannot decide (pure ASCII, no BOM). */
  default: string;
  /** Treat a pure-ASCII file as neutral and inherit the folder's dominant encoding. */
  inherit_ascii: boolean;
}

/** The guards between a generated block and the disk. Both default to on. */
export interface PicusWritingConfig {
  /** Show the diff and ask before touching disk. */
  confirm_before_write: boolean;
  /** Copy every file to `.arbor/backup/<timestamp>/` before rewriting it. */
  backup_before_write: boolean;
}

/** Emission defaults. The insertion rules are the wire strings of `InsertionRule`. */
export interface PicusGenerationConfig {
  /** Where a generated block lands in an initialisation script. */
  insertion_rule_init: string;
  /** Where a generated block lands in an update script. */
  insertion_rule_update: string;
  /** Lowercase identifiers when emitting PostgreSQL (Oracle is never affected). */
  lowercase_postgres: boolean;
}

/** Result-grid fetch behaviour. */
export interface PicusQueryConfig {
  /** Rows fetched per page. The BE clamps to 1…100 000 rather than trusting it. */
  row_limit: number;
}

/** Mirrors the BE `PicusConfig` (snake_case, section-for-section). */
export interface PicusConfig {
  encoding: PicusEncodingConfig;
  writing: PicusWritingConfig;
  generation: PicusGenerationConfig;
  queries: PicusQueryConfig;
}

/** Read the typed picus config (BE returns defaults on a missing/corrupt file). */
export function getPicusConfig(): Promise<PicusConfig> {
  return picus('get_picus_config', {});
}

/** Persist the typed picus config. */
export function setPicusConfig(config: PicusConfig): Promise<void> {
  return picus('set_picus_config', { config });
}
