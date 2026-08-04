/**
 * SQL language descriptors for the shared CodeMirror host.
 *
 * Highlighting rides on the shared per-dialect stream modes
 * ({@link sqlHighlight} — `code-editor/sql-modes.ts`), which is where the dialect
 * data and the Oracle backslash fix live now that Bennu highlights `.sql` files
 * too. What stays here is the half that is Picus's: binding a descriptor to a
 * **connection**.
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

import { sqlHighlight } from '$lib/components/shared/ui/code-editor';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
import type { Dialect } from '$lib/types/picus';
import { createSqlIntel } from './sql-intel';
import { escapeQuotesOnPaste } from './sql-intel/paste-escape';

/** The tree-sitter half of the descriptor is unused for `cmExtension` languages. */
const NO_TREE = {
  createParser: () => Promise.reject(new Error('picus SQL highlights through CodeMirror, not tree-sitter')),
  classify: () => null,
} as const;

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
 * An unknown dialect resolves to `portable`, never to a specific engine — see the
 * note in the body.
 */
export function sqlLanguage(
  dialect: Dialect | null | undefined,
  connectionId?: string,
): LanguageDescriptor {
  // `null` is a portable script, or one nobody has classified — not Oracle. It
  // used to fall through to Oracle silently, which is how a whole `COMMON` folder
  // came to be highlighted by the one dialect it is specifically not written in.
  const resolved: Dialect | 'portable' =
    dialect === 'postgres' ? 'postgres' : dialect === 'oracle' ? 'oracle' : 'portable';
  const key = `${resolved}|${connectionId ?? ''}`;
  const cached = descriptors.get(key);
  if (cached) return cached;

  // The dialect everything that has to *emit* speaks. A portable script is written
  // in the intersection of the two, and PostgreSQL is the side of that intersection
  // which invents nothing: quoting a name its way is valid in a file that must run
  // on Oracle too. There is no third emitter to pick.
  const spoken: Dialect = resolved === 'portable' ? 'postgres' : resolved;

  const descriptor: LanguageDescriptor = {
    id: `sql-${resolved}`,
    // Highlighting, plus the one editing behaviour that belongs to the *language*
    // rather than to the editor: a paste into a string literal has its quotes
    // escaped. CodeMirror accepts an array as an extension, so this composes here
    // without the shared editor having to learn what a SQL string is.
    cmExtension: [sqlHighlight(resolved), escapeQuotesOnPaste(spoken)],
    // No `commentTokens` here on purpose: a `cmExtension` language already carries
    // its own (the legacy SQL modes declare `--`), so `Ctrl+/` works without one.
    intel: createSqlIntel(spoken, connectionId),
    ...NO_TREE,
  };
  descriptors.set(key, descriptor);
  return descriptor;
}
