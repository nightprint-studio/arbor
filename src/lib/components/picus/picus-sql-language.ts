/**
 * SQL language descriptors for the shared CodeMirror host.
 *
 * Highlighting rides on CodeMirror's legacy SQL modes (already a dependency),
 * picked **per dialect**: `plSQL` for Oracle, `pgSQL` for PostgreSQL. That
 * matters beyond keyword lists — the two disagree about string escapes, dollar
 * quoting and comment forms, and a preview that colours `DO $$ … $$` as one
 * broken string reads as an error that isn't there.
 *
 * This is display only. The real parse — the one the inventory, the analysis and
 * the rewriter are built on — belongs to `picus-parse` (Tree-sitter) in the
 * backend, and never to the editor.
 */

import { StreamLanguage } from '@codemirror/language';
import { plSQL, pgSQL } from '@codemirror/legacy-modes/mode/sql';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
import type { Dialect } from '$lib/types/picus';

/** The tree-sitter half of the descriptor is unused for `cmExtension` languages. */
const NO_TREE = {
  createParser: () => Promise.reject(new Error('picus SQL highlights through CodeMirror, not tree-sitter')),
  classify: () => null,
} as const;

const ORACLE_LANGUAGE: LanguageDescriptor = {
  id: 'sql-oracle',
  cmExtension: StreamLanguage.define(plSQL),
  ...NO_TREE,
};

const POSTGRES_LANGUAGE: LanguageDescriptor = {
  id: 'sql-postgres',
  cmExtension: StreamLanguage.define(pgSQL),
  ...NO_TREE,
};

/** The descriptor for a dialect. Defaults to Oracle when the dialect is unknown
 *  (an unbound query tab still needs to highlight something sensible). */
export function sqlLanguage(dialect: Dialect | null | undefined): LanguageDescriptor {
  return dialect === 'postgres' ? POSTGRES_LANGUAGE : ORACLE_LANGUAGE;
}
