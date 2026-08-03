/**
 * The editor language for a Spring **property file** — `application*.yml`,
 * `application*.properties` and their `bootstrap*` siblings.
 *
 * Colouring is the stock CodeMirror mode; everything else here is the part a plain YAML mode
 * cannot do, because it comes from outside the file:
 *
 * - **completion** over the keys Spring and the project's libraries document (read out of the
 *   dependency jars) *plus* the project's own `@ConfigurationProperties` paths — the second
 *   half being the one that matters on a legacy tree, where nobody wrote documentation for
 *   `appaltiecontratti.application.*` and everybody misspells it;
 * - **value completion** where the set is closed (an enum, a boolean);
 * - **ghost text** where the answer is single-valued — a documented default for a key left
 *   empty, or a prefix exactly one key can continue;
 * - **hover** with the type, the default, the prose, and who reads it.
 *
 * All four are backend calls; nothing about the vocabulary lives here. That is deliberate —
 * the backend knows the classpath, and a property file is exactly the kind of file where a
 * frontend heuristic would be confidently wrong.
 *
 * ## The one thing this file decides
 *
 * Where the token being completed **starts**. The backend decides what the candidates are;
 * CodeMirror needs a `from` to replace, and that is a question about the buffer rather than
 * about Spring. The rule is the same one the backend classifies with — before the separator
 * you are typing a key, after it a value — kept deliberately shallow so the two cannot drift
 * in any way that matters: get it wrong and a completion is inserted at a slightly wrong
 * offset, never that the wrong candidates are offered.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { extCompletion, extHover, extInlineHint } from '$lib/ipc/bennu/ext';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { makeHoverSource } from './bennu-hover';

/** Whether a path is a Spring property source — the same test the backend applies, and the
 *  gate for handing out this descriptor at all. `messages.properties` is not one. */
export function isSpringPropertyFile(path: string): boolean {
  const name = (path.split(/[\\/]/).pop() ?? '').toLowerCase();
  return (
    (name.startsWith('application') || name.startsWith('bootstrap'))
    && /\.(ya?ml|properties)$/.test(name)
  );
}

/** Where the token under the caret begins, and whether it is a key or a value.
 *  `null` on a line that is a comment or a sequence item — nothing completes there. */
function tokenStart(ctx: CompletionContext): number | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const trimmed = before.trimStart();
  if (trimmed.startsWith('#') || trimmed.startsWith('!') || trimmed.startsWith('-')) return null;

  const sep = trimmed.search(/[:=]/);
  if (sep < 0) {
    // Still typing the key: the token starts where the indentation ends.
    return line.from + (before.length - trimmed.length);
  }
  // Past the separator: the token is the value, starting after the space that follows it.
  const afterSep = before.length - trimmed.length + sep + 1;
  const rest = before.slice(afterSep);
  return line.from + afterSep + (rest.length - rest.trimStart().length);
}

const propertyCompletionSource = async (
  ctx: CompletionContext,
): Promise<CompletionResult | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const from = tokenStart(ctx);
  if (from === null) return null;
  // With nothing typed, only answer an explicit request — otherwise the popup would open on
  // every newline in a config file, which is noise rather than help.
  if (!ctx.explicit && from === ctx.pos) return null;

  const src = ctx.state.doc.toString();
  const byteOffset = makeU16ToByte(src)(ctx.pos);
  const items = await extCompletion(path, src, byteOffset).catch(() => []);
  if (items.length === 0) return null;

  return {
    from,
    options: items.map((it) => ({
      label: it.label,
      type: it.kind === 'value' ? 'constant' : 'property',
      detail: it.detail ?? undefined,
    })),
    // The backend already filtered by what was typed; letting CodeMirror re-filter against a
    // dotted label would drop `spring.datasource.url` the moment the typed text spans a dot.
    filter: false,
  };
};

const propertyInlineSource = async (view: EditorView, pos: number): Promise<string | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const src = view.state.doc.toString();
  const byteOffset = makeU16ToByte(src)(pos);
  return await extInlineHint(path, src, byteOffset).catch(() => null);
};

// Hover goes straight to the framework extensions: a property file has no language server of
// its own, so unlike Java there is no first answer to fall back from.
const propertyHoverSource = makeHoverSource(async (path, src, byteOffset) => {
  const ext = await extHover(path, src, byteOffset).catch(() => null);
  return ext
    ? { signature: ext.signature, kind: ext.title, container: null, doc: ext.doc || null }
    : null;
});

const intel = {
  completion: propertyCompletionSource,
  hover: propertyHoverSource,
  inlineCompletion: propertyInlineSource,
};

/** Build the descriptor for one property syntax. Called once per syntax at module load — the
 *  identity has to be stable, because `CodeEditor` rebuilds its extensions when the descriptor
 *  changes and a fresh object per read would remount the editor on every keystroke. */
function springPropsLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return {
    id,
    createParser: () => Promise.reject(new Error(`cm-language:${id} has no tree-sitter parser`)),
    classify: () => null,
    cmExtension: StreamLanguage.define(parser),
    intel,
  };
}

export { springPropsLang };
