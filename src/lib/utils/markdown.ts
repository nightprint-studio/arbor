// Lightweight Markdown → HTML renderer used by issue trackers (Linear/Jira)
// and merge-request views (GitHub PRs / GitLab MRs).
//
// Supports the subset that issue/PR descriptions actually use:
//   • Fenced code blocks (with Prism syntax highlighting when available)
//   • Inline code, bold (**), italic (*), links [text](url)
//   • Headings (# ## ###)
//   • Blockquotes (> )
//   • Unordered lists (- / *) and ordered lists (1. )
//   • Horizontal rule (--- / *** / ___)
//   • A safelist of inline/block HTML tags (GitHub PR bodies and Dependabot
//     descriptions are HTML-heavy: <details>/<summary>, <p>, <blockquote>,
//     <code>, <em>, <a>, <ul>/<li>, etc.). Dangerous tags (<script>, <style>,
//     <iframe>) and event handlers are stripped; <a> is rewritten to a
//     non-clickable <span class="md-link"> to stay consistent with how the
//     markdown [text](url) syntax is handled.
//
// Output classes (md-pre, md-code, md-inline-code, md-h1…h3, md-p, md-bq,
// md-ul, md-ol, md-hr, md-link, md-spacer) are styled per-modal — see
// `IssueDetailModal.svelte` and `MrModal.svelte`.

import Prism from 'prismjs';
// Load every grammar Arbor supports onto `Prism.languages`. Without this the
// renderer would only highlight whichever languages other modules happened
// to have already imported (DiffViewer, highlight.ts, …) — so the same
// Dependabot description could render with rust highlighting one minute and
// plain text the next, depending on what the user had visited first.
import './prism-shared';
import { replaceEmojiShortcodes } from './emoji';

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Best-effort language detection for fenced blocks that arrive *without* a
 * language tag (Dependabot / GitLab Security Bot descriptions, raw issue
 * bodies pasted by humans, etc.). We only return a language that is actually
 * registered on `Prism.languages` (the grammars are bulk-imported via
 * `./prism-shared` at the top of this file) — otherwise highlighting would
 * silently fall through to plain text and the function would be pointless.
 *
 * The heuristic is intentionally cheap — first match wins. Add new branches
 * only when a real provider description forces our hand.
 */
function detectLang(code: string): string | null {
  const head = code.trimStart().slice(0, 400);
  if (!head) return null;

  // TOML — `[section]` (or `[[array]]`) followed by `key = value` lines.
  if (/^\[\[?[A-Za-z][\w.-]*\]?\]\s*\n[\s\S]*?[A-Za-z_][\w-]*\s*=/.test(head)) return 'toml';

  // Rust — distinctive top-level keywords / attribute syntax / `&[u8]`.
  if (
    /^\s*(fn|use|let|pub|mut|impl|struct|enum|trait|mod|const|static|unsafe|async)\b/.test(head)
    || /#!?\[[a-z_:]+/.test(head)
    || /&\[\s*u8\s*]/.test(head)
    || /::[A-Za-z_]/.test(head)
  ) return 'rust';

  // JSON — opening brace/bracket followed by a quoted key.
  if (/^\s*[{\[][\s\S]*?"[\w-]+"\s*:/.test(head)) return 'json';

  // YAML — top-level `key: value` lines, optional leading `---`.
  if (/^---\s*\n/.test(head) || /^[A-Za-z_][\w-]*:\s+\S/.test(head)) return 'yaml';

  // Shell — shebang, common CLI prefixes.
  if (/^#!\/(?:usr\/)?bin\/(?:env\s+)?(?:bash|sh|zsh)\b/.test(head)) return 'bash';
  if (/^\s*\$\s+\S/.test(head) || /^\s*(npm|yarn|pnpm|cargo|git|docker|kubectl)\s+/.test(head)) return 'bash';

  // TypeScript / JavaScript — imports, type declarations, arrow functions.
  if (/^\s*(import|export|type|interface|const|let|var|function|class|async)\b/.test(head)) {
    return /:\s*[A-Za-z_][\w.<>,\s|&[\]]*\s*[=){,;]/.test(head) || /\binterface\b|\btype\s+\w+\s*=/.test(head)
      ? 'typescript'
      : 'javascript';
  }

  // Markup — looks like HTML/XML.
  if (/^\s*<\/?[A-Za-z][\w-]*(\s|>)/.test(head)) return 'markup';

  return null;
}

const ALLOWED_HTML_TAGS = new Set([
  'a','abbr','b','blockquote','br','code','del','details','div','em',
  'h1','h2','h3','h4','h5','h6','hr','i','ins','kbd','li','mark',
  'ol','p','pre','q','s','small','span','strong','sub','summary','sup',
  'table','tbody','td','tfoot','th','thead','tr','u','ul',
]);

const BLOCK_HTML_TAGS = new Set([
  'blockquote','details','div','h1','h2','h3','h4','h5','h6','hr',
  'li','ol','p','pre','summary','table','tbody','td','tfoot','th','thead','tr','ul',
]);

function sanitizeHtml(s: string): string {
  s = s.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '');
  s = s.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');
  s = s.replace(/<iframe\b[^>]*>[\s\S]*?<\/iframe>/gi, '');
  s = s.replace(/\son\w+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, '');
  s = s.replace(/<a\b[^>]*>/gi, '<span class="md-link">');
  s = s.replace(/<\/a\s*>/gi, '</span>');
  s = s.replace(/<img\b[^>]*\/?\s*>/gi, '');
  return s;
}

function tagName(rawTag: string): string {
  const m = rawTag.match(/^<\/?\s*([a-zA-Z][a-zA-Z0-9-]*)/);
  return m ? m[1].toLowerCase() : '';
}

function extractAllowedTags(s: string, store: string[]): string {
  return s.replace(/<\/?[a-zA-Z][a-zA-Z0-9-]*\b[^>]*?\/?>/g, (m) => {
    if (ALLOWED_HTML_TAGS.has(tagName(m))) {
      store.push(m);
      return `\x00HT${store.length - 1}\x00`;
    }
    return m;
  });
}

function inline(s: string): string {
  const inlineCodes: string[] = [];
  // Pull inline-code segments out first — they must NOT be touched by the
  // emoji-shortcode pass (`:foo:` inside backticks should stay literal).
  s = s.replace(/`([^`]+)`/g, (_m, c) => {
    inlineCodes.push(`<code class="md-inline-code">${esc(c)}</code>`);
    return `\x00IC${inlineCodes.length - 1}\x00`;
  });
  s = replaceEmojiShortcodes(s);
  s = esc(s);
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  s = s.replace(/\[([^\]]+)\]\([^)]+\)/g, '<span class="md-link">$1</span>');
  s = s.replace(/\x00IC(\d+)\x00/g, (_m, i) => inlineCodes[parseInt(i)]);
  return s;
}

export function renderMarkdown(md: string): string {
  if (!md) return '';

  const codeBlocks: string[] = [];
  let text = md.replace(/```(\w*)\n?([\s\S]*?)```/g, (_match, lang: string, code: string) => {
    const trimmed = code.replace(/\n$/, '');
    // If the fence has no language tag, try to infer one from the content so
    // Dependabot / security-bot bodies (which often skip the tag) still get
    // highlighted instead of degrading to a wall of plain monospaced text.
    const effectiveLang = lang || detectLang(trimmed) || '';
    let highlighted: string;
    if (effectiveLang && Prism.languages[effectiveLang]) {
      try { highlighted = Prism.highlight(trimmed, Prism.languages[effectiveLang], effectiveLang); }
      catch { highlighted = esc(trimmed); }
    } else {
      highlighted = esc(trimmed);
    }
    codeBlocks.push(
      `<pre class="md-pre language-${effectiveLang || 'text'}"><code class="md-code">${highlighted}</code></pre>`,
    );
    return `\x00BLK${codeBlocks.length - 1}\x00`;
  });

  // Strip HTML comments (`<!-- ... -->`) AFTER fenced-code extraction — bots
  // like GitLab Security Bot embed invisible markers (e.g.
  // `<!-- policy_violation_comment -->`) that should never surface in the
  // rendered output. Doing this after the code-block placeholder pass keeps
  // genuine HTML-comment syntax inside fenced blocks intact.
  text = text.replace(/<!--[\s\S]*?-->/g, '');

  text = sanitizeHtml(text);
  const htmlTags: string[] = [];
  text = extractAllowedTags(text, htmlTags);

  const lines = text.split('\n');
  const out: string[] = [];
  let inUl = false, inOl = false;

  function closeList() {
    if (inUl) { out.push('</ul>'); inUl = false; }
    if (inOl) { out.push('</ol>'); inOl = false; }
  }

  for (const rawLine of lines) {
    const blkM = rawLine.match(/^\x00BLK(\d+)\x00$/);
    if (blkM) { closeList(); out.push(codeBlocks[parseInt(blkM[1])]); continue; }

    const hM = rawLine.match(/^(#{1,3})\s+(.*)/);
    if (hM) { closeList(); const lvl = hM[1].length; out.push(`<h${lvl} class="md-h${lvl}">${inline(hM[2])}</h${lvl}>`); continue; }

    const bqM = rawLine.match(/^>\s?(.*)/);
    if (bqM) { closeList(); out.push(`<blockquote class="md-bq">${inline(bqM[1])}</blockquote>`); continue; }

    if (/^[-*_]{3,}$/.test(rawLine.trim())) { closeList(); out.push('<hr class="md-hr" />'); continue; }

    const ulM = rawLine.match(/^[-*]\s+(.*)/);
    if (ulM) { if (inOl) { out.push('</ol>'); inOl = false; } if (!inUl) { out.push('<ul class="md-ul">'); inUl = true; } out.push(`<li>${inline(ulM[1])}</li>`); continue; }

    const olM = rawLine.match(/^\d+\.\s+(.*)/);
    if (olM) { if (inUl) { out.push('</ul>'); inUl = false; } if (!inOl) { out.push('<ol class="md-ol">'); inOl = true; } out.push(`<li>${inline(olM[1])}</li>`); continue; }

    closeList();
    if (rawLine.trim() === '') {
      out.push('<div class="md-spacer"></div>');
      continue;
    }

    // Lines that lead with a block-level HTML tag (e.g. `<details>`, `<p>…</p>`,
    // `<blockquote>`) should not be wrapped in `<p class="md-p">` — the wrap
    // would produce invalid nesting that browsers auto-close, breaking the
    // visual rhythm. Pass the line through `inline()` so markdown inside the
    // tags (bold, inline code, emoji) still works.
    const leadM = rawLine.match(/^\s*\x00HT(\d+)\x00/);
    if (leadM && BLOCK_HTML_TAGS.has(tagName(htmlTags[parseInt(leadM[1])]))) {
      out.push(inline(rawLine));
    } else {
      out.push(`<p class="md-p">${inline(rawLine)}</p>`);
    }
  }

  closeList();
  let result = out.join('');
  result = result.replace(/\x00HT(\d+)\x00/g, (_m, i) => htmlTags[parseInt(i)]);
  return result;
}
