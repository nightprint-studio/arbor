/**
 * Picus's SQL intelligence — completion, hover, live diagnostics and ghost text.
 *
 * Everything in this folder derives from two inputs and nothing else: the
 * **catalogue the connection already reported** (`schemaStore`) and the **text in
 * the buffer**. There is no language model anywhere in the flow, by product
 * requirement — and the constraint is what makes the result trustworthy rather
 * than merely plausible: every proposal is either a fact or absent.
 *
 * The split, and why:
 *
 * | Module | Responsibility |
 * |---|---|
 * | `tokens.ts` | Scanning: where the text is code and where it is a string, a comment or a `$$` body. Statement splitting. |
 * | `analysis.ts` | One statement's meaning: table references, **alias resolution**, and what the caret is in the middle of. |
 * | `keywords.ts` | The per-dialect vocabularies — one for suggesting, a wider one for *excluding*. |
 * | `schema-view.ts` | The single gate between "does not exist" and "not read yet". |
 * | `completion.ts` | The `CompletionSource`. |
 * | `hover.ts` | The `hoverTooltip` source. |
 * | `diagnostics.ts` | The four live rules, in UTF-8 byte offsets. |
 * | `ghost.ts` | The deterministic continuations. |
 *
 * They are separate files because they fail separately: the scanner's limits are
 * not the analysis's limits, and the diagnostics' conservatism is a policy that
 * has to be readable on its own. `picus-sql-language.ts` stays a descriptor.
 */

import type { CodeEditorIntel } from '$lib/components/shared/ui/code-editor';
import type { Dialect } from '$lib/types/picus';
import { createSqlCompletion } from './completion';
import { createSqlGhostText } from './ghost';
import { createSqlHover } from './hover';

export { sqlDiagnostics } from './diagnostics';
export type { SchemaView } from './schema-view';

/**
 * The `intel` bag for one dialect, bound to one connection.
 *
 * Both are fixed here and never read from a global. The dialect comes from the tab
 * — a connection's engine for a query, the folder's engine for a script — because
 * "the dialect is a property of the folder, never a current mode" is the product's
 * one structural invariant. The connection decides which catalogue, if any, this
 * buffer may be measured against; without one every feature quietly degrades to
 * keywords and buffer text rather than inventing a schema.
 */
export function createSqlIntel(dialect: Dialect, connectionId?: string): CodeEditorIntel {
  return {
    completion: createSqlCompletion(dialect, connectionId),
    hover: createSqlHover(dialect, connectionId),
    inlineCompletion: createSqlGhostText(dialect, connectionId),
  };
}
