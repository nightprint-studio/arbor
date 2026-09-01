/**
 * The `.dev` {@link LanguageDescriptor} — geode's playtest scenarios.
 *
 * A `.dev` is **not** a program: it is a batch of console commands, the same ones you type while
 * playing (`unlock while`, `set money 0`, `play playtest/crescita.dig`, `watch 30m --every 2m`).
 * The format belongs to fulcrum's console, not to geode — any game on that engine has it.
 *
 * ## Two layers, and the split is the point
 *
 * - **Colour is local and instant.** A stream mode that knows the *shape* of a line — a `#`
 *   comment, the first word is a command, `--flags`, quoted strings, numbers and durations — and
 *   nothing about which commands exist. It cannot be wrong, because it never claims a name is
 *   real.
 * - **Correctness comes from the server.** Whether `unlokc` exists, whether `Ametosta` is a
 *   crystal, whether `play x.dig` names a file that is there — that is `nd-dig-lsp`, reading the
 *   descriptor geode generates from its live command registry.
 *
 * ⚠️ The division matters: a local highlighter that tried to know the command names would carry a
 * copy of a list that changes whenever geode adds a command, and the copy is the one nobody
 * updates. Here the local layer knows only what cannot go stale.
 *
 * ## Why there is no tree-sitter grammar
 *
 * Because a grammar could only see the same shape this mode sees — it would cost a third grammar
 * and a third wasm to buy nothing. The vocabulary is data, and it arrives from the server.
 */

import { StreamLanguage } from '@codemirror/language';
import type { StreamParser } from '@codemirror/language';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { eagerBackendCompletionSource, backendHoverSource } from '../lsp-lang';

/** Where the tokenizer is on the line: the first word of a command is special. */
interface DevState {
  /** The next word starts a command — true at the start of a line and after `&&`. */
  head: boolean;
}

/** `30s`, `15m`, `2h` — a number and a unit, which a scenario is full of. */
const DURATION = /^\d+(\.\d+)?[smh]\b/;

/**
 * The `.dev` line mode.
 *
 * ⚠️ A `#` comment is the **whole line**, `&&` included: a commented-out line stays commented
 * even when there is an operator inside it. That is the rule the game's own colourizer applies,
 * and the two must agree or the same file reads differently in the two places.
 */
export const devMode: StreamParser<DevState> = {
  name: 'dig-dev',

  startState: () => ({ head: true }),

  token(stream, state): string | null {
    if (stream.sol()) state.head = true;
    if (stream.eatSpace()) return null;

    // A comment swallows the rest of the line.
    if (stream.peek() === '#') {
      stream.skipToEnd();
      return 'comment';
    }
    // The chain operator: after it, a new command begins.
    if (stream.match('&&')) {
      state.head = true;
      return 'operator';
    }
    if (state.head) {
      state.head = false;
      if (stream.match(/^[\w.-]+/)) return 'keyword';
    }
    // An option: `--focus`, `-f`. The `=` form keeps its value separate.
    if (stream.match(/^--?[A-Za-z][\w-]*/)) return 'attribute';
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return 'string';
    if (stream.match(DURATION)) return 'unit';
    if (stream.match(/^-?\d+(\.\d+)?\b/)) return 'number';
    if (stream.match(/^[^\s&#"]+/)) return 'variable';

    stream.next();
    return null;
  },
};

/** The `.dev` language: shape from here, truth from the server. */
export const devLanguage: LanguageDescriptor = {
  id: 'dev',
  createParser: () => Promise.reject(new Error('dev: highlighted by a stream mode')),
  classify: () => null,
  cmExtension: StreamLanguage.define(devMode),
  // A scenario is a flat list of lines: there is nothing to fold, and the server offers no ranges.
  cmFold: false,
  serverFold: false,
  commentTokens: { line: '#' },
  // A scenario is a batch of commands, so the vocabulary sits after a SPACE and never after a
  // dot: `unlock ` with the caret at the end is exactly where the server knows the answer. The
  // eager source asks it there, which the dotted-path gate of the default one never would.
  intel: { completion: eagerBackendCompletionSource, hover: backendHoverSource },
};
