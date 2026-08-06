/**
 * The editor language for **XML with a schema behind it**.
 *
 * Colouring is the stock CodeMirror mode; everything else here comes from outside the file —
 * the DTD or XSD the document names, found in the project or inside a dependency jar:
 *
 * - **completion** over the elements a parent may contain, the attributes an element may carry,
 *   and the values an attribute accepts where the schema closes the set;
 * - **ghost text** where exactly one thing can follow;
 * - **hover** with the schema's own documentation, the required attributes, and which grammar
 *   the answer came from.
 *
 * All three are backend calls, and all three are silent when no schema resolves. Nothing about
 * the vocabulary lives here — a frontend that guessed tag names from the ones already in the
 * file would confidently propose whatever typo is already there.
 *
 * The mechanics are `markup-intel.ts`, shared with the JSP descriptor: the two ask the same
 * backend the same three questions and differ only in what answers them.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { makeHoverSource } from './bennu-hover';
import {
  markupCompletionSource,
  markupExtHover,
  markupInlineHintSource,
} from './markup-intel';

// Straight to the framework extensions: an XML file has no language server of its own, so
// unlike Java there is no first answer to fall back from.
const intel = {
  completion: markupCompletionSource,
  hover: makeHoverSource(markupExtHover),
  inlineCompletion: markupInlineHintSource,
};

/** Build the descriptor. Called once at module load — the identity has to be stable, because
 *  `CodeEditor` rebuilds its extensions when the descriptor changes and a fresh object per read
 *  would remount the editor on every keystroke. */
export function xmlSchemaLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return {
    id,
    createParser: () => Promise.reject(new Error(`cm-language:${id} has no tree-sitter parser`)),
    classify: () => null,
    cmExtension: StreamLanguage.define(parser),
    intel,
  };
}
