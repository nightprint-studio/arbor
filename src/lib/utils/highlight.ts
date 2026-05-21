/**
 * Single-source-of-truth Prism wrapper for "highlight a blob of code into
 * an HTML string". Used by:
 *   - `FormNodeRenderer` for read-only `code` form nodes (plugin forms)
 *   - `JsonStudioModal` for the pretty-printed text view
 *
 * Anything that needs *line-by-line* highlighting (DiffViewer's old-vs-new
 * gutters) goes through `diff-formatter.ts` instead — that path needs
 * per-language hooks that don't apply here.
 *
 * JSON has its own purpose-built tokenizer (`highlightJsonFast` below)
 * that is roughly 10x faster than Prism on multi-MB payloads — Prism's
 * JSON grammar leans on regex/backtracking while the hand-rolled scanner
 * is a single forward pass. The dispatcher intercepts `language === 'json'`
 * so every existing call site benefits without changes.
 */

import Prism from 'prismjs';
import './prism-shared';
import { CUSTOM_HIGHLIGHTERS } from './prism-languages';

/** Returns HTML — the caller is expected to drop it via `{@html …}` into a
 *  `<pre><code>` element. The result is safe by construction (escaped) so
 *  long as Prism's grammars stay well-formed (Prism.highlight escapes the
 *  raw text before applying tokenisation). */
export function highlightCode(text: string, language?: string | null): string {
  if (!text) return '';
  if (!language) return escapeHtml(text);

  // Fast path: JSON has a hand-rolled tokenizer because it's hot path
  // for the JSON Studio modal (multi-MB documents) and a textbook-simple
  // grammar. Same output token classes as Prism, so CSS stays shared.
  if (language === 'json') return highlightJsonFast(text);

  const custom = CUSTOM_HIGHLIGHTERS[language];
  if (custom) {
    try { return custom(text); } catch { return escapeHtml(text); }
  }
  const grammar = Prism.languages[language];
  if (!grammar) return escapeHtml(text);
  try {
    return Prism.highlight(text, grammar, language);
  } catch {
    return escapeHtml(text);
  }
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

/**
 * Hand-written JSON tokenizer. Emits the same `<span class="token KIND">`
 * markup Prism produces (`string`/`property`/`number`/`boolean`/`null`/
 * `punctuation`), so the token CSS in `app.css` styles both equally.
 *
 * Why: Prism's JSON grammar is regex-based and pays measurable per-token
 * overhead. On a 6 MB pretty-printed JSON Prism takes ~500 ms; this
 * scanner does the same job in ~50 ms because it is a single forward
 * pass with no backtracking. The savings matter inside the chunked
 * progressive highlight in `JsonStudioModal` — fewer/shorter yields,
 * faster final swap.
 *
 * The scanner is tolerant: input that is not valid JSON still produces
 * something readable — unrecognised characters just pass through escaped.
 * The expected caller is `JsonStudio` which has already round-tripped
 * the document through `serde_json`, so well-formedness is essentially
 * guaranteed.
 */
function highlightJsonFast(text: string): string {
  return highlightJsonChunk(text, 0, text.length).html;
}

/**
 * Chunked variant of the JSON highlighter. Processes tokens from
 * `start` until either `start + budget` chars have been consumed OR
 * the end of `text` is reached, whichever comes first. The cut always
 * happens on a token boundary (the budget is checked at the top of the
 * outer loop, never mid-token), so callers can resume by handing back
 * `nextPos` on the next call without any stitching logic.
 *
 * Used by `JsonStudioModal` for asynchronously highlighting very long
 * lines (e.g. raw mode on a minified document where a single line can
 * be megabytes wide). The chunk cost stays bounded so the main thread
 * yields between chunks; the user sees colour fill in progressively.
 */
export function highlightJsonChunk(
  text: string,
  start: number,
  budget: number,
): { html: string; nextPos: number } {
  const out: string[] = [];
  const n = text.length;
  let i = start;
  const limit = Math.min(start + budget, n);

  // Hoist literal escapes for tight-loop hot path.
  const SPAN_STR_OPEN  = '<span class="token string">';
  const SPAN_NUM_OPEN  = '<span class="token number">';
  const SPAN_BOOL_OPEN = '<span class="token boolean">';
  const SPAN_NULL_OPEN = '<span class="token null">';
  const SPAN_PUNC_OPEN = '<span class="token punctuation">';
  const SPAN_CLOSE     = '</span>';
  // Property-key spans are coloured by the kind of the value that
  // follows them (see lookahead below) so the text view matches the
  // colouring of the tree, where each key adopts its value's hue.
  // `.token.property` is the fallback when lookahead fails (e.g. last
  // line of a chunked input where the value lives on the next line).
  const SPAN_PROP_OBJ  = '<span class="token property property-object">';
  const SPAN_PROP_ARR  = '<span class="token property property-array">';
  const SPAN_PROP_STR  = '<span class="token property property-string">';
  const SPAN_PROP_NUM  = '<span class="token property property-number">';
  const SPAN_PROP_BOOL = '<span class="token property property-boolean">';
  const SPAN_PROP_NULL = '<span class="token property property-null">';
  const SPAN_PROP_DEF  = '<span class="token property">';

  function propSpanFor(firstByte: number): string {
    if (firstByte === 123) return SPAN_PROP_OBJ;
    if (firstByte === 91)  return SPAN_PROP_ARR;
    if (firstByte === 34)  return SPAN_PROP_STR;
    if (firstByte === 116 || firstByte === 102) return SPAN_PROP_BOOL;
    if (firstByte === 110) return SPAN_PROP_NULL;
    if (firstByte === 45 || (firstByte >= 48 && firstByte <= 57)) return SPAN_PROP_NUM;
    return SPAN_PROP_DEF;
  }

  while (i < n) {
    if (i >= limit) break; // budget exhausted at a clean token boundary
    const c = text.charCodeAt(i);

    // ── Whitespace — emit verbatim (no escaping needed) ──
    if (c === 32 /* space */ || c === 9 /* tab */ || c === 10 /* \n */ || c === 13 /* \r */) {
      const start = i;
      i++;
      while (i < n) {
        const cc = text.charCodeAt(i);
        if (cc !== 32 && cc !== 9 && cc !== 10 && cc !== 13) break;
        i++;
      }
      out.push(text.slice(start, i));
      continue;
    }

    // ── String (also property key when followed by `:`) ──
    if (c === 34 /* " */) {
      const start = i;
      i++;
      while (i < n) {
        const cc = text.charCodeAt(i);
        if (cc === 92 /* \ */) { i += 2; continue; } // skip escape sequence
        if (cc === 34) { i++; break; }
        i++;
      }
      const tok = text.slice(start, i);
      // Look ahead skipping whitespace to detect property key.
      let j = i;
      while (j < n) {
        const cc = text.charCodeAt(j);
        if (cc !== 32 && cc !== 9 && cc !== 10 && cc !== 13) break;
        j++;
      }
      const isKey = j < n && text.charCodeAt(j) === 58 /* : */;
      if (isKey) {
        // Peek past the `:` and any whitespace to determine the value's
        // kind — used to colour the property key by its value's hue,
        // mirroring the tree view (orange for object/array keys, green
        // for string keys, purple for number/bool, muted for null).
        let k = j + 1;
        while (k < n) {
          const cc = text.charCodeAt(k);
          if (cc !== 32 && cc !== 9 && cc !== 10 && cc !== 13) break;
          k++;
        }
        const openSpan = k < n ? propSpanFor(text.charCodeAt(k)) : SPAN_PROP_DEF;
        out.push(openSpan, escapeHtml(tok), SPAN_CLOSE);
      } else {
        out.push(SPAN_STR_OPEN, escapeHtml(tok), SPAN_CLOSE);
      }
      continue;
    }

    // ── Number (`-`? digit+ (`.` digit+)? (`e` [+-]? digit+)?) ──
    if (c === 45 /* - */ || (c >= 48 && c <= 57)) {
      const start = i;
      if (c === 45) i++;
      while (i < n && text.charCodeAt(i) >= 48 && text.charCodeAt(i) <= 57) i++;
      if (i < n && text.charCodeAt(i) === 46 /* . */) {
        i++;
        while (i < n && text.charCodeAt(i) >= 48 && text.charCodeAt(i) <= 57) i++;
      }
      if (i < n && (text.charCodeAt(i) === 101 /* e */ || text.charCodeAt(i) === 69 /* E */)) {
        i++;
        const s = text.charCodeAt(i);
        if (s === 43 /* + */ || s === 45 /* - */) i++;
        while (i < n && text.charCodeAt(i) >= 48 && text.charCodeAt(i) <= 57) i++;
      }
      // Numbers are ASCII — slice is safe, no escaping needed.
      out.push(SPAN_NUM_OPEN, text.slice(start, i), SPAN_CLOSE);
      continue;
    }

    // ── Keywords ──
    if (c === 116 /* t */ && i + 4 <= n && text.charCodeAt(i+1) === 114 && text.charCodeAt(i+2) === 117 && text.charCodeAt(i+3) === 101) {
      out.push(SPAN_BOOL_OPEN, 'true', SPAN_CLOSE);
      i += 4;
      continue;
    }
    if (c === 102 /* f */ && i + 5 <= n && text.charCodeAt(i+1) === 97 && text.charCodeAt(i+2) === 108 && text.charCodeAt(i+3) === 115 && text.charCodeAt(i+4) === 101) {
      out.push(SPAN_BOOL_OPEN, 'false', SPAN_CLOSE);
      i += 5;
      continue;
    }
    if (c === 110 /* n */ && i + 4 <= n && text.charCodeAt(i+1) === 117 && text.charCodeAt(i+2) === 108 && text.charCodeAt(i+3) === 108) {
      out.push(SPAN_NULL_OPEN, 'null', SPAN_CLOSE);
      i += 4;
      continue;
    }

    // ── Punctuation ──
    if (c === 123 /* { */ || c === 125 /* } */ || c === 91 /* [ */ || c === 93 /* ] */ || c === 58 /* : */ || c === 44 /* , */) {
      out.push(SPAN_PUNC_OPEN, text[i], SPAN_CLOSE);
      i++;
      continue;
    }

    // ── Anything else (malformed input) — emit escaped + skip ──
    out.push(escapeHtml(text[i]));
    i++;
  }

  return { html: out.join(''), nextPos: i };
}
