/**
 * Strips ALL ANSI/VT escape sequences from a string (plain text output).
 * Handles CSI (including DEC private), OSC, and bare ESC sequences.
 */
export function stripAnsi(raw: string): string {
  return raw
    // OSC sequences: ESC ] ... BEL  or  ESC ] ... ESC \
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    // CSI sequences: ESC [ ... final-byte  (including DEC private ?/!/> prefixes)
    .replace(/\x1b\[[^A-Za-z]*[A-Za-z]/g, '')
    // Any remaining two-char escape sequences: ESC + one char
    .replace(/\x1b./g, '');
}

/**
 * Converts a string containing ANSI escape codes to safe HTML with inline styles.
 *
 * Supported SGR attributes:
 *   0  reset           1  bold           2  dim            3  italic
 *   4  underline       5  blink (CSS)    7  reverse        8  hidden
 *   9  strikethrough   21 double-ul      22 bold/dim off   23 italic off
 *   24 underline off   25 blink off      27 reverse off    28 hidden off
 *   29 strikethrough off  39 default fg  49 default bg     53 overline
 *   55 overline off
 *   30–37/90–97  standard + bright fg (16-color palette)
 *   40–47/100–107 standard + bright bg
 *   38;5;n / 48;5;n  256-color fg/bg
 *   38;2;r;g;b / 48;2;r;g;b  truecolor fg/bg
 *
 * Non-SGR escape sequences (cursor movement, OSC, DEC private, etc.) are
 * stripped silently.  HTML special characters in text are escaped.
 */

// One Dark-inspired 16-color palette (matches the app's dark theme).
const PALETTE_16 = [
  '#3d4048', // 0  black
  '#e06c6c', // 1  red
  '#98c379', // 2  green
  '#e5c07b', // 3  yellow
  '#61afef', // 4  blue
  '#c678dd', // 5  magenta
  '#56b6c2', // 6  cyan
  '#abb2bf', // 7  white
  '#5c6370', // 8  bright black (gray)
  '#f47d7d', // 9  bright red
  '#b5d9a0', // 10 bright green
  '#f0d09e', // 11 bright yellow
  '#7dc4f5', // 12 bright blue
  '#d19fff', // 13 bright magenta
  '#7ecbd4', // 14 bright cyan
  '#ffffff', // 15 bright white
];

// Approximate dark-theme defaults used when reverse-video needs a fallback.
const DEFAULT_FG = '#abb2bf';
const DEFAULT_BG = '#21252b';

function color256(n: number): string {
  if (n < 16) return PALETTE_16[n];
  if (n >= 232) {
    const v = Math.round(((n - 232) / 23) * 255);
    return `rgb(${v},${v},${v})`;
  }
  const idx = n - 16;
  const r = Math.floor(idx / 36);
  const g = Math.floor((idx % 36) / 6);
  const b = idx % 6;
  return `rgb(${r * 51},${g * 51},${b * 51})`;
}

function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

interface AnsiState {
  fg: string | null;
  bg: string | null;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  overline: boolean;
  blink: boolean;
  hidden: boolean;
  reverse: boolean;
}

function resetState(): AnsiState {
  return {
    fg: null, bg: null,
    bold: false, dim: false, italic: false,
    underline: false, strikethrough: false, overline: false,
    blink: false, hidden: false, reverse: false,
  };
}

function buildStyle(s: AnsiState): string {
  const parts: string[] = [];

  // Reverse video swaps fg ↔ bg, falling back to theme defaults.
  let fg = s.reverse ? (s.bg ?? DEFAULT_BG) : s.fg;
  let bg = s.reverse ? (s.fg ?? DEFAULT_FG) : s.bg;

  if (s.hidden) {
    parts.push('opacity:0');
  } else {
    if (s.dim) parts.push('opacity:0.55');
  }
  if (fg) parts.push(`color:${fg}`);
  if (bg) parts.push(`background:${bg}`);
  if (s.bold)   parts.push('font-weight:700');
  if (s.italic) parts.push('font-style:italic');

  const deco: string[] = [];
  if (s.underline)     deco.push('underline');
  if (s.strikethrough) deco.push('line-through');
  if (s.overline)      deco.push('overline');
  if (deco.length)     parts.push(`text-decoration:${deco.join(' ')}`);

  if (s.blink) parts.push('animation:ansi-blink 1s step-end infinite');

  return parts.join(';');
}

// Unified regex that matches:
//   1. OSC sequences:  ESC ] ... BEL  |  ESC ] ... ESC \
//   2. CSI sequences:  ESC [ <params> <final>   (DEC private ?/!/> are captured too)
//   3. Other 2-char:   ESC <any single char>
//
// Capture groups: [1] = CSI param string (may start with ?, !, >), [2] = CSI final byte letter.
const ALL_ESC_RE =
  /\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[([^A-Za-z]*)([A-Za-z])|\x1b[^[\]]/g;

export function ansiToHtml(raw: string): string {
  // Strip the [stderr] tag — the parent element's CSS class handles stderr styling.
  const line = raw.startsWith('[stderr]') ? raw.slice(8).trimStart() : raw;

  let result = '';
  let lastIndex = 0;
  let state: AnsiState = resetState();

  const flush = (text: string) => {
    if (!text) return;
    const style = buildStyle(state);
    result += style
      ? `<span style="${style}">${escHtml(text)}</span>`
      : escHtml(text);
  };

  ALL_ESC_RE.lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = ALL_ESC_RE.exec(line)) !== null) {
    flush(line.slice(lastIndex, match.index));
    lastIndex = match.index + match[0].length;

    // match[2] is only defined for CSI sequences; only 'm' (SGR) is meaningful.
    if (!match[2] || match[2] !== 'm') continue;

    const paramStr = match[1] ?? '';
    // Ignore CSI sequences with DEC private / other non-digit prefixes (e.g. ?25h).
    if (paramStr.length && !/^[\d;]*$/.test(paramStr)) continue;

    const codes = paramStr === '' ? [0] : paramStr.split(';').map(Number);
    let i = 0;
    while (i < codes.length) {
      const c = codes[i];
      if (c === 0) {
        state = resetState();
      } else if (c === 1) {
        state.bold = true;
      } else if (c === 2) {
        state.dim = true;
      } else if (c === 3) {
        state.italic = true;
      } else if (c === 4 || c === 21) {
        state.underline = true;
      } else if (c === 5 || c === 6) {
        state.blink = true;
      } else if (c === 7) {
        state.reverse = true;
      } else if (c === 8) {
        state.hidden = true;
      } else if (c === 9) {
        state.strikethrough = true;
      } else if (c === 22) {
        state.bold = false; state.dim = false;
      } else if (c === 23) {
        state.italic = false;
      } else if (c === 24) {
        state.underline = false;
      } else if (c === 25) {
        state.blink = false;
      } else if (c === 27) {
        state.reverse = false;
      } else if (c === 28) {
        state.hidden = false;
      } else if (c === 29) {
        state.strikethrough = false;
      } else if (c >= 30 && c <= 37) {
        state.fg = PALETTE_16[c - 30];
      } else if (c === 38) {
        if (codes[i + 1] === 5 && i + 2 < codes.length) {
          state.fg = color256(codes[i + 2]);
          i += 2;
        } else if (codes[i + 1] === 2 && i + 4 < codes.length) {
          state.fg = `rgb(${codes[i + 2]},${codes[i + 3]},${codes[i + 4]})`;
          i += 4;
        }
      } else if (c === 39) {
        state.fg = null;
      } else if (c >= 40 && c <= 47) {
        state.bg = PALETTE_16[c - 40];
      } else if (c === 48) {
        if (codes[i + 1] === 5 && i + 2 < codes.length) {
          state.bg = color256(codes[i + 2]);
          i += 2;
        } else if (codes[i + 1] === 2 && i + 4 < codes.length) {
          state.bg = `rgb(${codes[i + 2]},${codes[i + 3]},${codes[i + 4]})`;
          i += 4;
        }
      } else if (c === 49) {
        state.bg = null;
      } else if (c === 53) {
        state.overline = true;
      } else if (c === 55) {
        state.overline = false;
      } else if (c >= 90 && c <= 97) {
        state.fg = PALETTE_16[c - 90 + 8];
      } else if (c >= 100 && c <= 107) {
        state.bg = PALETTE_16[c - 100 + 8];
      }
      i++;
    }
  }

  flush(line.slice(lastIndex));
  return result;
}
