/**
 * A tiny, language-agnostic syntax highlighter for editor CHROME that renders code OUTSIDE the main
 * text flow — the sticky-scroll header and the scrollbar-overview preview lens. Those show lines that
 * are scrolled off-screen (so CodeMirror hasn't rendered their DOM), yet they should still read as
 * code, not flat text.
 *
 * It emits `<span class="cm-tok-…">` using the SAME token classes the editor theme already colours
 * (`theme.ts`), so the chrome matches the buffer under any theme overlay for free. Deliberately
 * approximate (a single regex pass over comments / strings / annotations / numbers / words) — it
 * only ever COLOURS text, never changes it, so a wrong guess is a wrong colour, never wrong content.
 */

/** Keywords coloured as language scaffolding — a broad Java/JS/TS/C-family set (harmless extras in a
 *  language that doesn't use one; the goal is legibility, not a real lexer). */
const KEYWORDS = new Set([
  'abstract', 'assert', 'boolean', 'break', 'byte', 'case', 'catch', 'char', 'class', 'const',
  'continue', 'default', 'do', 'double', 'else', 'enum', 'extends', 'final', 'finally', 'float',
  'for', 'goto', 'if', 'implements', 'import', 'instanceof', 'int', 'interface', 'long', 'native',
  'new', 'package', 'private', 'protected', 'public', 'record', 'return', 'sealed', 'permits',
  'short', 'static', 'strictfp', 'super', 'switch', 'synchronized', 'this', 'throw', 'throws',
  'transient', 'try', 'var', 'void', 'volatile', 'while', 'yield', 'true', 'false', 'null',
  'function', 'let', 'const', 'typeof', 'in', 'of',
]);

/** Escape the four HTML-significant characters so buffer content is never interpreted as markup. */
function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'));
}

/** One match per: line/block comment | string/char | annotation | number | word (identifier). Gaps
 *  between matches (operators, punctuation, whitespace) are emitted escaped, uncoloured. */
const TOKEN =
  /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(@[A-Za-z_$][\w$.]*)|(\b\d[\w.]*)|([A-Za-z_$][\w$]*)/g;

/** Render one line (or a short block) of code as highlighted HTML using the editor's `cm-tok-*`
 *  classes. Safe to drop straight into `innerHTML` — every character is HTML-escaped. */
export function highlightToHtml(code: string): string {
  let out = '';
  let last = 0;
  TOKEN.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TOKEN.exec(code)) !== null) {
    if (m.index > last) out += esc(code.slice(last, m.index));
    const [full, comment, str, anno, num, word] = m;
    if (comment !== undefined) {
      out += `<span class="cm-tok-comment">${esc(comment)}</span>`;
    } else if (str !== undefined) {
      out += `<span class="cm-tok-string">${esc(str)}</span>`;
    } else if (anno !== undefined) {
      out += `<span class="cm-tok-annotation">${esc(anno)}</span>`;
    } else if (num !== undefined) {
      out += `<span class="cm-tok-number">${esc(num)}</span>`;
    } else if (word !== undefined) {
      const cls = KEYWORDS.has(word)
        ? 'cm-tok-keyword'
        : /^[A-Z]/.test(word)
          ? 'cm-tok-type' // Capitalized → a type name (heuristic)
          : 'cm-tok-ident';
      out += `<span class="${cls}">${esc(word)}</span>`;
    } else {
      out += esc(full);
    }
    last = m.index + full.length;
  }
  if (last < code.length) out += esc(code.slice(last));
  return out;
}
