/**
 * The languages a fenced code block in a Markdown file is coloured with.
 *
 * A README is mostly prose *around* code: the `mvn` invocation, the `docker run`, the snippet of
 * `pom.xml` that has to go in. Without this list every one of those blocks renders in the same
 * flat monospace as the paragraph above it — `@codemirror/lang-markdown` parses the fence and its
 * info string perfectly well, it simply has no languages to hand the body to unless it is given
 * some.
 *
 * ## Why these, and why they are all eager
 *
 * Every parser here is **already imported** by `languages.ts` for files of that type, so listing it
 * costs nothing at load time and nothing in the bundle: the alternative — `LanguageDescription`'s
 * lazy `load()` — would buy a saving that has already been spent, and pay for it with fences that
 * colour a frame after the document renders.
 *
 * The set is the vocabulary of a JVM project's documentation, not every language CodeMirror has a
 * mode for. A fence tagged with something not in here stays plain, which is the honest outcome: a
 * wrong grammar paints half a block as an unterminated string, and that reads as a broken file
 * rather than as an unknown language.
 *
 * Names follow the tags people actually write — `sh`, `bash`, `zsh`, `console` and `shell` all
 * reach the shell mode, because a README uses whichever one its author learned first.
 *
 * ## The other half of the same file
 *
 * A `.md` opens **rendered**, and in that view the fences are tokenised by **Prism**
 * (`utils/markdown-editor.ts`, `PRISM_LANG_ALIAS`). This list is the **source** view, reached with
 * *Markdown: edit the source*. Neither can serve the other — one paints a string, the other a
 * buffer — so the pair is deliberate rather than duplicated; but the two vocabularies should stay
 * in step, because a tag that colours in one view and not the other reads as a bug in whichever
 * one you happened to look at second.
 */

import { LanguageDescription, LanguageSupport, StreamLanguage } from '@codemirror/language';
import type { StreamParser } from '@codemirror/language';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { batch } from './batch-lang';
import { dockerFile } from '@codemirror/legacy-modes/mode/dockerfile';
import { xml } from '@codemirror/legacy-modes/mode/xml';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { rust } from '@codemirror/legacy-modes/mode/rust';
import { python } from '@codemirror/legacy-modes/mode/python';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { go } from '@codemirror/legacy-modes/mode/go';
import { java, kotlin, c, cpp } from '@codemirror/legacy-modes/mode/clike';
import { json } from '@codemirror/lang-json';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { javascript } from '@codemirror/lang-javascript';
import { sqlLanguage } from '$lib/components/shared/ui/code-editor';

/** A fence language backed by a legacy stream mode. */
function streamFence(name: string, alias: string[], parser: StreamParser<unknown>) {
  return LanguageDescription.of({
    name,
    alias,
    support: new LanguageSupport(StreamLanguage.define(parser)),
  });
}

/** A fence language backed by a real `LanguageSupport` (a Lezer grammar, or a composed one). */
function fence(name: string, alias: string[], support: LanguageSupport) {
  return LanguageDescription.of({ name, alias, support });
}

/**
 * The fence vocabulary, built once.
 *
 * The identity matters for the same reason every descriptor in `languages.ts` is a module
 * singleton: `markdown()` is called with this array, and a fresh one per read would hand
 * `CodeEditor` a different extension on every keystroke and remount the editor under the caret.
 */
export const MARKDOWN_FENCE_LANGUAGES: readonly LanguageDescription[] = [
  // The one a README has more of than any other: every "how do I run this" block.
  streamFence('shell', ['sh', 'bash', 'zsh', 'ksh', 'console', 'shell-session', 'terminal'], shell),
  // The other half of "how do I run this", on the projects that ship a `.bat` beside the `.sh`.
  streamFence('batch', ['bat', 'cmd', 'dosbatch', 'winbatch'], batch),
  streamFence('dockerfile', ['docker', 'containerfile'], dockerFile),
  streamFence('java', [], java),
  streamFence('kotlin', ['kt', 'kts'], kotlin),
  streamFence('xml', ['pom', 'xsd', 'wsdl', 'svg'], xml),
  streamFence('yaml', ['yml'], yaml),
  streamFence('toml', [], toml),
  streamFence('properties', ['ini', 'conf', 'cfg', 'editorconfig'], properties),
  streamFence('rust', ['rs'], rust),
  streamFence('python', ['py'], python),
  streamFence('lua', [], lua),
  streamFence('go', ['golang'], go),
  streamFence('c', ['h'], c),
  streamFence('cpp', ['c++', 'cc', 'hpp'], cpp),
  fence('json', ['json5', 'jsonc'], json()),
  fence('html', ['htm', 'xhtml'], html()),
  fence('css', [], css()),
  fence('javascript', ['js', 'mjs', 'cjs', 'node'], javascript()),
  fence('jsx', [], javascript({ jsx: true })),
  fence('typescript', ['ts'], javascript({ typescript: true })),
  fence('tsx', [], javascript({ typescript: true, jsx: true })),
  // The portable dialect, deliberately: a fence carries no more evidence of which engine it
  // targets than a `.sql` file does, and `languages.ts` explains at length why guessing is worse
  // than the rules valid on every engine.
  fence('sql', [], new LanguageSupport(sqlLanguage('portable'))),
];
