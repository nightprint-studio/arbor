/**
 * Picus's SQL intelligence — completion, hover, live diagnostics and ghost text.
 *
 * Everything in this folder derives from two inputs and nothing else: the
 * **catalogue the connection already reported** and the **text in the buffer**.
 * Most of it reads that catalogue locally through `schemaStore`; the abbreviation
 * expander reads the same one in the backend, because the language that resolves it
 * lives there. There is no language model anywhere in the flow, by product
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
 * | `builtins.ts` | The engines' own functions: signature, return type, one sentence, and the trap. The only facts here that do not come from a connection — a function's meaning belongs to the engine, not to the database. |
 * | `continuations.ts` | What the grammar allows at the caret, before anything is looked up. |
 * | `schema-view.ts` | The single gate between "does not exist" and "not read yet". |
 * | `completion.ts` | The `CompletionSource`. |
 * | `hover.ts` | The `hoverTooltip` source. |
 * | `diagnostics.ts` | The four live rules, in UTF-8 byte offsets. |
 * | `ghost.ts` | The deterministic continuations. |
 * | `paste-escape.ts` | Pasting into a string literal doubles its quotes — and leaves dollar-quoted and `q'[…]'` bodies alone, where doubling would corrupt rather than protect. |
 * | `abbrev.ts` | The abbreviation shorthand — one backend verb for the expansion, the caret context and the refusal. |
 * | `binds.ts` | The placeholders a statement wants values for, and the positional list they are sent as. Read off the scanner's tokens, so `::`, `:=`, `:NEW` and anything inside a string or a comment are not one. |
 *
 * They are separate files because they fail separately: the scanner's limits are
 * not the analysis's limits, and the diagnostics' conservatism is a policy that
 * has to be readable on its own. `picus-sql-language.ts` stays a descriptor.
 */

import type { CompletionContext, CompletionSource } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import type {
  CodeEditorIntel,
  InlineCompletionSource,
} from '$lib/components/shared/ui/code-editor';
import type { Dialect } from '$lib/types/picus';
import { createAbbrevIntel } from './abbrev';
import { createSqlCompletion } from './completion';
import { createSqlGhostText } from './ghost';
import { createSqlHover } from './hover';

export { sqlDiagnostics } from './diagnostics';
export type { SchemaView } from './schema-view';

/**
 * Ask the first source, fall back to the second.
 *
 * Composition rather than a flag inside either one: the SQL sources have no reason
 * to learn what an abbreviation is, and the abbreviation source has no reason to
 * learn SQL. Each answers for the lines it recognises and `null` everywhere else,
 * and the precedence is stated here, once, where both are visible.
 */
function firstAnswer(preferred: CompletionSource, fallback: CompletionSource): CompletionSource {
  return async (ctx: CompletionContext) => (await preferred(ctx)) ?? fallback(ctx);
}

function firstProposal(
  preferred: InlineCompletionSource,
  fallback: InlineCompletionSource,
): InlineCompletionSource {
  return async (view: EditorView, pos: number) =>
    (await preferred(view, pos)) ?? fallback(view, pos);
}

/**
 * The `intel` bag for one dialect, bound to one connection.
 *
 * Both are fixed here and never read from a global. The dialect comes from the tab
 * — a connection's engine for a query, the folder's engine for a script — because
 * "the dialect is a property of the folder, never a current mode" is the product's
 * one structural invariant. The connection decides which catalogue, if any, this
 * buffer may be measured against; without one every feature quietly degrades to
 * keywords and buffer text rather than inventing a schema.
 *
 * The abbreviation layer is the sharpest case of that: with no connection there is
 * no schema, and with no schema there is no type to decide a quote and no foreign
 * key to decide a join — so it is simply not installed, and the SQL sources are the
 * whole of the intelligence.
 */
export function createSqlIntel(dialect: Dialect, connectionId?: string): CodeEditorIntel {
  const completion = createSqlCompletion(dialect, connectionId);
  const ghost = createSqlGhostText(dialect, connectionId);
  const abbrev = createAbbrevIntel(dialect, connectionId);

  return {
    completion: abbrev?.completion ? firstAnswer(abbrev.completion, completion) : completion,
    hover: createSqlHover(dialect, connectionId),
    inlineCompletion: abbrev?.inlineCompletion
      ? firstProposal(abbrev.inlineCompletion, ghost)
      : ghost,
  };
}
