/**
 * Shared utilities for in-page text search: query compilation, label
 * highlighting, and DOM `<mark>` injection / cleanup.
 *
 * Used by `DocsPanel` and `SettingsPanel` (and any future tree-shaped
 * documentation surface) so the search behavior stays consistent.
 */

const ESCAPE_RE = /[.*+?^${}()|[\]\\]/g;

export interface CompileOptions {
  regex?: boolean;
  /** Match case. Default false (case-insensitive). */
  caseSensitive?: boolean;
}

/**
 * Build a global RegExp for a query. Returns `null` when the query is
 * empty or — in regex mode — invalid. Always uses the global flag so the
 * same instance can be reused with `matchAll` / `exec` loops.
 */
export function compileQuery(query: string, opts: CompileOptions = {}): RegExp | null {
  const q = query;
  if (!q) return null;
  const flags = opts.caseSensitive ? 'g' : 'gi';
  const pattern = opts.regex ? q : q.replace(ESCAPE_RE, '\\$&');
  try {
    return new RegExp(pattern, flags);
  } catch {
    return null;
  }
}

/** Tags whose text content is excluded from the highlight pass.
 *
 *  · `PRE` / `CODE` — preserve code samples verbatim; we never want
 *    `<mark>` shoved into a syntax-highlighted block.
 *  · `MARK` — guard against re-wrapping an existing highlight when an
 *    incremental injection runs over a partially-highlighted tree.
 *  · `SCRIPT` / `STYLE` — never user-visible text.
 *  · `BUTTON` — action labels are not searchable content. Highlighting a
 *    fragment inside a button (e.g. the "4" in "Reset to 4") makes the
 *    fragment look like a chip / tiny embedded image, fragmenting the
 *    label visually for no real navigational gain. Matches the IDE
 *    convention of "search highlights body content, not toolbar /
 *    button affordances". */
const DEFAULT_SKIP_TAGS = new Set([
  'PRE', 'CODE', 'MARK', 'SCRIPT', 'STYLE', 'BUTTON',
]);

function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Render a label with `<mark>` wrappers around regex matches.
 * Returned string is HTML-safe (escapes the rest of the label).
 *
 * If `re` is null, returns the escaped label unchanged.
 */
export function highlightLabel(label: string, re: RegExp | null): string {
  if (!re) return escHtml(label);
  // RegExp.global state — clone to avoid sharing lastIndex across calls.
  const cloned = new RegExp(re.source, re.flags);
  let out = '';
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = cloned.exec(label))) {
    if (m.index > last) out += escHtml(label.slice(last, m.index));
    if (m[0].length === 0) {
      // zero-width match — avoid infinite loop
      cloned.lastIndex++;
      continue;
    }
    out += '<mark>' + escHtml(m[0]) + '</mark>';
    last = m.index + m[0].length;
  }
  if (last < label.length) out += escHtml(label.slice(last));
  return out;
}

/** Test whether `text` contains at least one match of `re`. */
export function textMatches(text: string, re: RegExp | null): boolean {
  if (!re) return false;
  const cloned = new RegExp(re.source, re.flags);
  return cloned.test(text);
}

export interface InjectOptions {
  /** Tags whose text is excluded from highlight. Default: pre, code, mark, script, style. */
  skipTags?: Iterable<string>;
  /** CSS class applied to inserted `<mark>` elements. Default: `text-search-mark`. */
  className?: string;
}

/**
 * Remove every `<mark>` previously injected by `injectHighlights`,
 * splicing the original text back into place.
 */
export function clearHighlights(root: HTMLElement, className = 'text-search-mark') {
  const marks = root.querySelectorAll<HTMLElement>(`mark.${CSS.escape(className)}`);
  if (marks.length === 0) return;
  marks.forEach((m) => {
    const text = document.createTextNode(m.textContent ?? '');
    m.parentNode?.replaceChild(text, m);
  });
  root.normalize();
}

/**
 * Walk text nodes under `root`, wrapping each match of `re` in a
 * `<mark class="text-search-mark">`. Skips descendants of `skipTags`
 * (default: pre/code/mark/script/style) so code blocks stay untouched.
 *
 * Returns the inserted `<mark>` elements in document order — handy for
 * scroll-into-view + previous/next navigation.
 *
 * Caller is responsible for calling `clearHighlights` first if rerunning.
 */
export function injectHighlights(
  root: HTMLElement,
  re: RegExp | null,
  opts: InjectOptions = {},
): HTMLElement[] {
  const className = opts.className ?? 'text-search-mark';
  if (!re) return [];

  const skip = new Set<string>(DEFAULT_SKIP_TAGS);
  if (opts.skipTags) for (const t of opts.skipTags) skip.add(t.toUpperCase());

  // Phase 1 — collect candidate text nodes (no mutation during walk).
  const targets: Text[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      let p = node.parentElement;
      while (p && p !== root) {
        if (skip.has(p.tagName)) return NodeFilter.FILTER_REJECT;
        p = p.parentElement;
      }
      const text = node.nodeValue ?? '';
      if (!text || !text.trim()) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let n: Node | null;
  while ((n = walker.nextNode())) targets.push(n as Text);

  // Phase 2 — split each text node and inject <mark> elements.
  const marks: HTMLElement[] = [];
  for (const textNode of targets) {
    const text = textNode.nodeValue ?? '';
    const cloned = new RegExp(re.source, re.flags);
    let last = 0;
    let m: RegExpExecArray | null;
    let any = false;
    const frag = document.createDocumentFragment();
    while ((m = cloned.exec(text))) {
      if (m[0].length === 0) {
        cloned.lastIndex++;
        continue;
      }
      any = true;
      if (m.index > last) frag.appendChild(document.createTextNode(text.slice(last, m.index)));
      const mark = document.createElement('mark');
      mark.className = className;
      mark.textContent = m[0];
      frag.appendChild(mark);
      marks.push(mark);
      last = m.index + m[0].length;
    }
    if (!any) continue;
    if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)));
    textNode.parentNode?.replaceChild(frag, textNode);
  }

  return marks;
}
