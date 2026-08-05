/**
 * ANSI escape sequences → styled spans, for the Run console.
 *
 * A program's stdout is not plain text. Anything using jansi, logback's colour layout, or a
 * logger someone forced colour on writes SGR escapes, and a console that renders them
 * literally shows `[32m` in front of every line — which reads as the program being broken.
 * So the sequences are either turned into styling or dropped, never printed.
 *
 * **Deliberately partial.** It handles SGR (the colour/weight codes) and *discards* every
 * other escape — cursor movement, erase-line, the alternate screen. Honouring those means
 * being a terminal emulator: a screen buffer, a cursor, reflow on resize. Bennu has a real
 * terminal one panel over (xterm.js, Alt+F12) for when that is what you want; this is a
 * transcript of what a program printed, and a transcript has no cursor to move. A progress
 * bar that redraws itself with `\r` therefore appears as its successive states rather than
 * animating in place — the honest rendering of the same bytes.
 *
 * Pure and synchronous.
 */

/** A run of text sharing one style. `cls` is empty for the default style. */
export interface AnsiSpan {
  text: string;
  /** Space-separated class names — `a-red`, `a-bold`, … See the console's stylesheet. */
  cls: string;
}

/** The escape character, never written literally in this file — a raw control byte in a
 *  source file is invisible in every diff it ever appears in. */
const ESC = '\u001b';

/** SGR colour codes → the class suffix. 30-37 normal, 90-97 bright, rendered the same:
 *  eight hues is what a themed console should show, sixteen is two of everything. */
const FG: Record<number, string> = {
  30: 'black', 31: 'red', 32: 'green', 33: 'yellow',
  34: 'blue', 35: 'magenta', 36: 'cyan', 37: 'white',
  90: 'black', 91: 'red', 92: 'green', 93: 'yellow',
  94: 'blue', 95: 'magenta', 96: 'cyan', 97: 'white',
};

/**
 * One escape sequence: a CSI (`ESC [ params letter`), an OSC (`ESC ] … BEL | ST`), or a
 * two-character escape. Only a CSI ending in `m` carries styling; everything else matched
 * here is dropped, which is the point — it is matched so it can be removed.
 */
const ESCAPE = /\u001b(?:\[([0-9;?]*)([a-zA-Z])|\][\s\S]*?(?:\u0007|\u001b\\)|[@-_])/g;

/**
 * Split `line` into styled spans, consuming the escape sequences.
 *
 * Style is per line by design: a colour opened on one line and never closed does not leak
 * into the rest of the console. A log line that opens a colour, prints its level and closes
 * it is the overwhelmingly common shape and is fully served by this.
 */
export function ansiSpans(line: string): AnsiSpan[] {
  // Fast path — the vast majority of lines carry no escapes at all.
  if (!line.includes(ESC)) return line ? [{ text: line, cls: '' }] : [];

  const spans: AnsiSpan[] = [];
  let colour = '';
  let bold = false;
  let last = 0;
  let m: RegExpExecArray | null;

  const cls = () => [colour ? `a-${colour}` : '', bold ? 'a-bold' : ''].filter(Boolean).join(' ');
  const emit = (text: string) => {
    if (text) spans.push({ text, cls: cls() });
  };

  ESCAPE.lastIndex = 0;
  while ((m = ESCAPE.exec(line)) !== null) {
    emit(line.slice(last, m.index));
    last = m.index + m[0].length;
    // Only `ESC [ … m` changes the style; every other sequence is simply removed.
    if (m[2] !== 'm') continue;
    for (const raw of (m[1] || '0').split(';')) {
      const code = Number(raw || '0');
      if (code === 0) { colour = ''; bold = false; }
      else if (code === 1) bold = true;
      else if (code === 22) bold = false;
      else if (code === 39) colour = '';
      else if (FG[code]) colour = FG[code];
    }
  }
  emit(line.slice(last));
  return spans;
}

/** The line with every escape sequence removed — for copying, or for matching against the
 *  text, where the styling is noise. */
export function stripAnsi(line: string): string {
  return line.includes(ESC) ? line.replace(ESCAPE, '') : line;
}
