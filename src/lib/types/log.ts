/**
 * The interpreted-log wire shapes — the frontend twin of the `arbor-logscan` crate.
 *
 * A backend that streams a child process's output interprets each line on the way out
 * (`crates/foundation/logscan`, plus whatever resolution needs the host's index) and sends
 * the line **already cut up**. Nothing here carries offsets: Rust counts UTF-8 bytes and
 * JavaScript counts UTF-16 code units, so a range crossing the seam would be a bug waiting
 * for the first accented log line.
 *
 * Rendering is {@link LogLine}; what a link *means* is the host's (a URL opens in the
 * browser, a file opens in the editor).
 */

/** Severity, once something on the line said so — or inherited from the line above, which
 *  is how a stack trace stays part of its error. */
export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';

/** What a piece IS. Unknown values render as plain text, so the backend may add one. */
export type LogToken =
  | 'text'
  | 'level'
  | 'timestamp'
  | 'thread'
  | 'package'
  | 'exception'
  | 'frame'
  | 'url'
  | 'path';

/** An ANSI hue the program itself asked for (bright and normal collapse to one case). */
export type LogColour =
  | 'black' | 'red' | 'green' | 'yellow' | 'blue' | 'magenta' | 'cyan' | 'white';

/**
 * Where a piece points.
 *
 * `source` is a stack frame in a class this project does **not** declare — the JDK, a
 * dependency. It is deliberately unresolved on the wire: finding the source of
 * `java.lang.Thread` means reading jars, and that is worth doing for the one frame someone
 * clicks rather than for all forty as they stream past. `bennu_frame_source` answers it on
 * the click; see `log-link.ts`.
 */
export type LogLink =
  | { kind: 'source'; class: string; method?: string; file?: string; line?: number }
  | { kind: 'file'; path: string; line?: number }
  | { kind: 'url'; url: string };

/** One consecutive stretch of a line. Concatenating every `text` reproduces the line. */
export interface LogPiece {
  text: string;
  /** Absent means plain text. */
  token?: LogToken;
  /** Absent means the theme's default — the program asked for no colour. */
  colour?: LogColour;
  bold?: boolean;
  link?: LogLink;
}
