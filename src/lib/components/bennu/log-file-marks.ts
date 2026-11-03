/**
 * Reading one line of a **log file** opened in the editor — the pure half of {@link ./log-file-highlight}.
 *
 * The console reads program output with `arbor-logscan` in the backend, which knows every dialect it
 * has been taught. A log file opened as text has no backend between it and the editor, and a file of
 * thirty thousand lines is not one to round-trip, so this is the same idea at the size a highlighter
 * can afford: the level a line says it is, and where its timestamp, thread and logger are.
 *
 * What it knows is what the backend's rules know — `ERROR`, `[WARN]`, `SEVERE`, Tomcat's own lines on a
 * localised JVM (`GRAVE`, `AVVERTENZA`, `INFORMAZIONI`) — and the one thing a highlighter needs that a
 * console does not: a **continuation** (a stack frame, `Caused by:`) belongs to the line above it and
 * is painted with its level.
 */

export type FileLogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';
export type LogMarkKind = 'ts' | 'level' | 'thread' | 'logger';

export interface LogMark {
  from: number;
  to: number;
  kind: LogMarkKind;
}

/** Upper case only, as in the backend: `error` and `warning` are ordinary words in a message. */
const WORDS: Record<string, FileLogLevel> = {
  TRACE: 'trace', FINEST: 'trace', FINER: 'trace', VERBOSE: 'trace',
  DEBUG: 'debug', FINE: 'debug', CONFIG: 'debug',
  INFO: 'info', NOTICE: 'info', INFORMAZIONI: 'info', INFORMATIONS: 'info', INFORMATIONEN: 'info', INFORMACIÓN: 'info', INFORMAÇÕES: 'info',
  WARN: 'warn', WARNING: 'warn', AVVERTENZA: 'warn', AVERTISSEMENT: 'warn', WARNUNG: 'warn', ADVERTENCIA: 'warn', AVISO: 'warn',
  ERROR: 'error', SEVERE: 'error', GRAVE: 'error', SCHWERWIEGEND: 'error',
  FATAL: 'fatal', CRITICAL: 'fatal',
};

const LEVEL_WORD = new RegExp(`(?<![\\p{L}\\p{N}_.])(${Object.keys(WORDS).join('|')})(?![\\p{L}\\p{N}_])`, 'u');
const STAMP =
  /^\s*\[?(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?|\d{2}-[A-Za-z]{3,4}\.?-\d{4} \d{2}:\d{2}:\d{2}(?:[.,]\d+)?|\d{2}:\d{2}:\d{2}(?:[.,]\d+)?)\]?/;
const THREAD = /^\s*(\[[^\]\s=.:/\\]{2,64}\])/;
const LOGGER = /^\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$<>]*)+)/;
/** Where a level word can be: a line that says something about its severity says it near the start. */
const HEAD = 160;

export function levelOfWord(word: string): FileLogLevel | null {
  return WORDS[word] ?? null;
}

/** Whether a line belongs to the one above it: indented, a `Caused by:` head, or `… 23 more`. */
export function continues(text: string): boolean {
  return /^\s+\S/.test(text) || /^(?:Caused by:|Suppressed:|\.\.\. \d+ more)/.test(text);
}

export interface ReadLine {
  /** The level the line itself names, or `null`. */
  level: FileLogLevel | null;
  marks: LogMark[];
}

/** The level a line names and where its columns are. Offsets are into `text`. */
export function readLogLine(text: string): ReadLine {
  const head = text.slice(0, HEAD);
  const found = LEVEL_WORD.exec(head);
  if (!found) return { level: null, marks: [] };
  const marks: LogMark[] = [];
  const levelFrom = found.index;
  const levelTo = levelFrom + found[1].length;

  const stamp = STAMP.exec(head);
  if (stamp) {
    const from = stamp.index + stamp[0].indexOf(stamp[1]);
    // A date after the level is not this line's own stamp.
    if (from + stamp[1].length <= levelFrom) marks.push({ from, to: from + stamp[1].length, kind: 'ts' });
  }
  marks.push({ from: levelFrom, to: levelTo, kind: 'level' });

  let at = levelTo;
  const rest = (): string => text.slice(at);
  // `]` of a bracketed level (`[ERROR]`) is not part of what follows.
  if (text[at] === ']') at++;
  const thread = THREAD.exec(rest());
  if (thread) {
    const from = at + thread[0].indexOf(thread[1]);
    marks.push({ from, to: from + thread[1].length, kind: 'thread' });
    at = from + thread[1].length;
  }
  const logger = LOGGER.exec(rest());
  if (logger) {
    const from = at + logger[0].indexOf(logger[1]);
    marks.push({ from, to: from + logger[1].length, kind: 'logger' });
  }
  return { level: WORDS[found[1]], marks };
}
