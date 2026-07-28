/**
 * SQL language descriptors for the shared CodeMirror host.
 *
 * Highlighting rides on CodeMirror's legacy SQL modes (already a dependency),
 * picked **per dialect**: `plSQL` for Oracle, `pgSQL` for PostgreSQL. That
 * matters beyond keyword lists — the two disagree about string escapes, dollar
 * quoting and comment forms, and a preview that colours `DO $$ … $$` as one
 * broken string reads as an error that isn't there.
 *
 * Intelligence — completion, hover and ghost text — comes from `sql-intel/`,
 * attached here through `intel`. With a `cmExtension` descriptor the *tree*-driven
 * hooks (`resolveGoto`, `foldNode`) are inactive because there is no live tree;
 * the `intel` hooks and the `diagnostics` prop do not care and work as they are.
 * Wiring the real `picus-parse` grammar into the editor is a separate, later
 * decision — and the one that unlocks in-buffer navigation.
 *
 * The real parse — the one the inventory, the analysis and the rewriter are built
 * on — belongs to `picus-parse` in the backend, and never to the editor.
 */

import { StreamLanguage } from '@codemirror/language';
import { plSQL, pgSQL } from '@codemirror/legacy-modes/mode/sql';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
import type { Dialect } from '$lib/types/picus';
import { createSqlIntel } from './sql-intel';

/** The tree-sitter half of the descriptor is unused for `cmExtension` languages. */
const NO_TREE = {
  createParser: () => Promise.reject(new Error('picus SQL highlights through CodeMirror, not tree-sitter')),
  classify: () => null,
} as const;

const HIGHLIGHT: Record<Dialect, LanguageDescriptor['cmExtension']> = {
  oracle: StreamLanguage.define(plSQL),
  postgres: StreamLanguage.define(pgSQL),
};

/**
 * Descriptors are cached per `(dialect, connection)` pair.
 *
 * Identity matters: `CodeEditor` builds its extensions from the descriptor at
 * mount, so handing it a freshly-allocated object on every reactive read would
 * churn the editor for no reason. Two keys and not one because the intelligence is
 * bound to a connection — the same Oracle dialect against two databases is two
 * different sets of facts.
 */
const descriptors = new Map<string, LanguageDescriptor>();

/**
 * The descriptor for a dialect, optionally bound to a connection's catalogue.
 *
 * `connectionId` is what turns colouring into intelligence: with it, completion
 * offers this database's tables and columns, hover states their types and the
 * diagnostics can tell an unknown table from an unread schema. Without it the
 * editor still completes keywords and closes blocks, and reports nothing about
 * objects — which is the correct behaviour for a script file with no database
 * open, and much better than measuring it against somebody else's schema.
 *
 * Defaults to Oracle when the dialect is unknown (an unbound query tab still needs
 * to highlight something sensible).
 */
export function sqlLanguage(
  dialect: Dialect | null | undefined,
  connectionId?: string,
): LanguageDescriptor {
  const resolved: Dialect = dialect === 'postgres' ? 'postgres' : 'oracle';
  const key = `${resolved}|${connectionId ?? ''}`;
  const cached = descriptors.get(key);
  if (cached) return cached;

  const descriptor: LanguageDescriptor = {
    id: `sql-${resolved}`,
    cmExtension: HIGHLIGHT[resolved],
    // No `commentTokens` here on purpose: a `cmExtension` language already carries
    // its own (the legacy SQL modes declare `--`), so `Ctrl+/` works without one.
    intel: createSqlIntel(resolved, connectionId),
    ...NO_TREE,
  };
  descriptors.set(key, descriptor);
  return descriptor;
}
