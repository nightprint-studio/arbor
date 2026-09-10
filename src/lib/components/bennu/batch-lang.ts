/**
 * Windows batch (`.bat` / `.cmd`) — a stream mode, written here because there isn't one.
 *
 * `@codemirror/legacy-modes` ships fifty modes and none of them is batch, so these files opened as
 * plain text: in a legacy Java tree that is the whole build-and-deploy story — `setenv.bat`, the
 * Tomcat launchers, the one-line wrappers around `mvn` — and a wall of grey is exactly the file you
 * are least willing to read carefully. Written rather than added as a dependency (CLAUDE.md #7),
 * which is also why it is small.
 *
 * The same parser serves both views: `languages.ts` for a `.bat` buffer, and `markdown-fences.ts`
 * for a ```` ```bat ```` block in a README, so one vocabulary answers both.
 *
 * ## What it colours, and what it deliberately does not
 *
 * Batch has no grammar worth the name — quoting is positional, `^` escapes the *newline*, and
 * `for /f` reparses its own body — so this is a lexer over the shapes that carry meaning:
 * comments, labels, the control words, the variable expansions and the redirections. It does not
 * try to know which command is being run: an unknown word stays uncoloured rather than being
 * guessed at as a program, because a batch file is mostly words and colouring them all says
 * nothing.
 *
 * Everything is case-insensitive, which is not a nicety: `IF EXIST` and `if exist` are the same
 * line, and half the files in an old tree shout.
 */

import type { StreamParser, StringStream } from '@codemirror/language';

/** Control flow and the words that only mean something to `cmd` itself. */
const KEYWORDS = new Set([
  'if', 'else', 'for', 'in', 'do', 'goto', 'call', 'exit', 'set', 'setlocal', 'endlocal',
  'shift', 'defined', 'exist', 'not', 'errorlevel', 'pause', 'rem',
  // The comparison operators of `if`, which are words rather than symbols.
  'equ', 'neq', 'lss', 'leq', 'gtr', 'geq',
]);

/** Commands `cmd` provides — worth marking because they are the ones that DO something to the
 *  machine, and a reader scanning a script is looking for exactly those lines. */
const BUILTINS = new Set([
  'echo', 'cd', 'chdir', 'md', 'mkdir', 'rd', 'rmdir', 'del', 'erase', 'copy', 'xcopy', 'robocopy',
  'move', 'ren', 'rename', 'dir', 'type', 'more', 'find', 'findstr', 'sort', 'cls', 'title',
  'color', 'prompt', 'pushd', 'popd', 'start', 'timeout', 'choice', 'attrib', 'assoc', 'ftype',
  'path', 'ver', 'vol', 'date', 'time', 'tree', 'where', 'mklink', 'fc', 'comp', 'subst',
  // Not `cmd`'s own, but present in practically every deployment script that exists.
  'taskkill', 'tasklist', 'sc', 'net', 'reg', 'wmic', 'schtasks', 'icacls', 'powershell', 'curl',
]);

interface BatchState {
  /** Nothing but whitespace has been seen since the start of the current *command*.
   *
   *  Not "since the start of the line": `&` and `|` begin a new command on the same line, and a
   *  `goto` after one is as much a keyword as a `goto` at the margin. */
  start: boolean;
}

/** A `%…%` expansion, a `%1` argument, `%*`, or a `%~dp0` modifier run. */
function eatPercent(stream: StringStream): boolean {
  if (!stream.eat('%')) return false;
  // `%%` is a literal percent inside a `for` body — and `%%i` is that body's variable.
  stream.eat('%');
  if (stream.eat('*')) return true;
  if (stream.match(/^~[a-zA-Z$:]*[0-9a-zA-Z_]/)) return true;
  if (stream.match(/^[0-9]/)) return true;
  // A named variable closes with its own `%`. An unterminated one is a real and common typo, so
  // it is consumed to the end of the word rather than swallowing the rest of the line.
  if (stream.match(/^[^%\s]+%/)) return true;
  stream.match(/^[^%\s]*/);
  return true;
}

export const batch: StreamParser<BatchState> = {
  name: 'batch',

  startState: () => ({ start: true }),

  token(stream, state) {
    if (stream.sol()) state.start = true;
    if (stream.eatSpace()) return null;

    const atStart = state.start;
    state.start = false;

    // ── comments ──────────────────────────────────────────────────────────────
    // `::` is a label that can never be jumped to, which is why it works as a comment — and why
    // it is only one at the start of a command, where a label is legal.
    if (atStart && stream.match(/^::/)) {
      stream.skipToEnd();
      return 'comment';
    }
    if (atStart && stream.match(/^rem\b/i)) {
      stream.skipToEnd();
      return 'comment';
    }

    // `@` suppresses the echo of the command it prefixes — the command still follows, so this is
    // not the start of anything being consumed.
    if (stream.eat('@')) {
      state.start = true;
      return 'operator';
    }

    // ── a label, and the only definition batch has ─────────────────────────────
    if (atStart && stream.match(/^:[^\s:][^\s]*/)) {
      return 'def';
    }

    // ── expansions ────────────────────────────────────────────────────────────
    if (stream.peek() === '%') {
      eatPercent(stream);
      return 'variable-2';
    }
    // Delayed expansion — `!VAR!` under `setlocal enabledelayedexpansion`, which is the only way
    // to read a variable written inside the same block.
    if (stream.match(/^![^!\s]+!/)) {
      return 'variable-2';
    }

    // ── strings ───────────────────────────────────────────────────────────────
    // No escape inside a batch string: the closing quote is the next `"`, full stop. An
    // unterminated one ends at the line, because it does.
    if (stream.eat('"')) {
      stream.match(/^[^"]*"?/);
      return 'string';
    }

    // ── operators, redirections, grouping ─────────────────────────────────────
    if (stream.match(/^(\|\||&&|>>|[|&])/)) {
      // All four begin a new command.
      state.start = true;
      return 'operator';
    }
    if (stream.match(/^(>|<|==|\^)/)) {
      return 'operator';
    }
    if (stream.eat('(')) {
      // A block body is a sequence of commands, so what follows starts one.
      state.start = true;
      return 'bracket';
    }
    if (stream.eat(')')) {
      return 'bracket';
    }

    // A switch — `/f`, `/i`, `/d`. Marked because on a `for` or an `xcopy` the switches are the
    // whole meaning of the line.
    if (stream.match(/^\/[a-zA-Z?][a-zA-Z0-9:]*/)) {
      return 'attribute';
    }

    if (stream.match(/^-?\d+\b/)) {
      return 'number';
    }

    // ── words ─────────────────────────────────────────────────────────────────
    const word = stream.match(/^[^\s|&<>()%!"^=,;]+/) as RegExpMatchArray | null;
    if (word) {
      const lower = word[0].toLowerCase();
      if (KEYWORDS.has(lower)) return 'keyword';
      // Only where a command can begin: `copy` is a command at the start of one and a filename
      // called `copy` anywhere else, and colouring the argument would be inventing a fact.
      if (atStart && BUILTINS.has(lower)) return 'builtin';
      return null;
    }

    // Anything left is punctuation batch gives no meaning to (`=`, `,`, `;` — argument
    // separators). Consumed so the stream always advances; a mode that returns without
    // consuming hangs the editor.
    stream.next();
    return null;
  },

  languageData: {
    // `REM` and not `::`: it is the documented one, it works in every position a comment can
    // legally appear, and `::` inside a parenthesised block is a syntax error — which would make
    // a comment-toggle break the script it was used on.
    commentTokens: { line: 'REM ' },
  },
};
