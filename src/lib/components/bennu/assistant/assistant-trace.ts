/**
 * Finding a stack trace in a program's output, and the request that asks the assistant about it.
 *
 * The console already knows how to colour a line and make a frame clickable; what it did not do was offer to
 * *do* something about the trace. This finds each trace — an exception line followed by its frames, with any
 * `Caused by:` chains — so the console can put one button on the line that starts it, and builds the message
 * that button sends: the trace itself, and what to do with it.
 *
 * Java first (the `at pkg.Class.method(File.java:12)` frames), then Rust's panics and backtraces. Pure, so
 * what counts as a trace is tested.
 */

export interface StackTrace {
  /** Index of the line the trace starts on: the exception, or the panic message. */
  head: number;
  /** Index of its last line, inclusive. */
  end: number;
}

/** A Java frame, or the `... 12 more` line that closes a nested cause. */
const JAVA_FRAME = /^\s+at\s+\S+\(.*\)\s*$|^\s*\.\.\.\s+\d+\s+(more|common frames omitted)\s*$/;
const JAVA_CAUSE = /^(Caused by|Suppressed):\s/;
/** What starts a trace: `java.lang.NullPointerException: …`, `Exception in thread "main" …`. */
const JAVA_HEAD = /(^|\s)([\w$]+\.)*[\w$]*(Exception|Error|Throwable)\b|^Exception in thread/;

/** Rust: the panic line, then `stack backtrace:` and numbered frames, each with an `at path:line` line. */
const RUST_HEAD = /panicked at /;
const RUST_FRAME = /^\s+\d+:\s+\S|^\s+at\s+\S+:\d+/;

/** The longest trace sent, in lines. Deeper than this is the same few frames repeated, rarely more useful. */
const MAX_TRACE_LINES = 120;

/** Every stack trace in `lines`, in order. */
export function findStackTraces(lines: readonly string[]): StackTrace[] {
  const out: StackTrace[] = [];
  let i = 0;
  while (i < lines.length) {
    const text = lines[i];
    const javaHead = !JAVA_FRAME.test(text) && JAVA_HEAD.test(text);
    const rustHead = RUST_HEAD.test(text);
    if (!javaHead && !rustHead) { i++; continue; }

    const frame = rustHead ? RUST_FRAME : JAVA_FRAME;
    let end = i;
    let frames = 0;
    // A Rust panic puts its message on the lines after the head; those belong to it.
    let gap = rustHead ? 3 : 0;
    for (let j = i + 1; j < lines.length; j++) {
      const t = lines[j];
      if (frame.test(t)) { frames++; end = j; continue; }
      if (!rustHead && JAVA_CAUSE.test(t)) { end = j; continue; }
      // A Rust backtrace announces itself on its own line before the frames.
      if (rustHead && /^stack backtrace:|^note: run with/.test(t)) { end = j; continue; }
      if (gap > 0 && frames === 0 && t.trim() !== '') { gap--; end = j; continue; }
      break;
    }
    // A panic is worth asking about with or without a backtrace; a Java exception needs its frames to be one.
    if (frames > 0 || rustHead) {
      out.push({ head: i, end });
      i = end + 1;
    } else {
      i++;
    }
  }
  return out;
}

/** The message for the button: the trace, and what to do about it. */
export function tracePrompt(lines: readonly string[], trace: StackTrace): string {
  const slice = lines.slice(trace.head, trace.end + 1);
  const shown = slice.length > MAX_TRACE_LINES ? [...slice.slice(0, MAX_TRACE_LINES), `… (${slice.length - MAX_TRACE_LINES} more lines)`] : slice;
  return [
    'This stack trace came out of the program I am running. Find the cause in the project and fix it.',
    '',
    '```',
    shown.join('\n'),
    '```',
    '',
    'Start from the frames that are in this project\'s own files — read them and what they call — and work out why it fails, rather than guessing from the exception name. Fix it with edit_file, changing only what the cause needs, and say what you found.',
  ].join('\n');
}
