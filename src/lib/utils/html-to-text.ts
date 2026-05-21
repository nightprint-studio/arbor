/**
 * Convert a rendered HTML fragment into plain text for clipboard use.
 *
 * Behaviour:
 *   · `<ul>` / `<li>`           → `- item` per line
 *   · `<ol>` / `<li>`           → `1. item`, `2. item`, …
 *   · `<p>` / `<div>` / `<br>`  → newline boundaries
 *   · `<pre>` / `<code>`        → inner text preserved verbatim
 *   · Everything else           → falls back to the node's textContent.
 *
 * The walker keeps a per-`<ol>` counter so nested ordered lists restart
 * from 1. Bullet prefixes are indented by 2 spaces per nesting level so
 * the structure stays readable in the clipboard.
 */
export function htmlToText(html: string): string {
  if (!html) return '';
  const tpl = document.createElement('template');
  tpl.innerHTML = html;

  const out: string[] = [];
  walk(tpl.content, out, 0);

  return out.join('')
    // Map non-ASCII whitespace to plain spaces so editors like Sublime
    // don't show them as <0xa0> markers: U+00A0 no-break space (from
    // &nbsp;), U+2007 figure space, U+202F narrow no-break space.
    .replace(/[   ]/g, ' ')
    // Strip zero-width characters Jira inserts around inline markup:
    // U+200B…U+200D and U+FEFF (BOM).
    .replace(/[​-‍﻿]/g, '')
    // Collapse runs of 3+ newlines into 2 (paragraph breaks).
    .replace(/\n{3,}/g, '\n\n')
    // Trim trailing whitespace on each line (Jira pads list items).
    .replace(/[ \t]+\n/g, '\n')
    .trim();
}

function walk(node: Node, out: string[], depth: number): void {
  if (node.nodeType === Node.TEXT_NODE) {
    out.push(node.textContent ?? '');
    return;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    // DocumentFragment / Document — descend into children only. Without
    // this branch the initial walk(tpl.content, …) call would no-op and
    // htmlToText would always return ''.
    for (const child of Array.from(node.childNodes)) walk(child, out, depth);
    return;
  }

  const el  = node as Element;
  const tag = el.tagName.toLowerCase();

  switch (tag) {
    case 'br':
      out.push('\n');
      return;

    case 'hr':
      ensureBlankLine(out);
      out.push('---\n');
      return;

    case 'ul':
    case 'ol': {
      ensureNewline(out);
      let n = 1;
      for (const child of Array.from(el.children)) {
        if (child.tagName.toLowerCase() !== 'li') {
          walk(child, out, depth);
          continue;
        }
        out.push('  '.repeat(depth));
        out.push(tag === 'ol' ? `${n}. ` : '- ');
        const inner: string[] = [];
        for (const liChild of Array.from(child.childNodes)) {
          walk(liChild, inner, depth + 1);
        }
        // Nested lists inside <li> already emitted their own newlines —
        // strip a trailing newline so the bullet sits on one line.
        out.push(inner.join('').replace(/\n+$/, ''));
        out.push('\n');
        n++;
      }
      return;
    }

    case 'li': {
      // Bare <li> — typical of Range.cloneContents() when the selection
      // spans multiple list items: the wrapper <ul>/<ol> is dropped and
      // the fragment exposes the <li> children directly. Without this
      // branch they'd fall through to the default and lose the bullet.
      ensureNewline(out);
      out.push('  '.repeat(depth));
      out.push('- ');
      const inner: string[] = [];
      for (const child of Array.from(el.childNodes)) walk(child, inner, depth + 1);
      out.push(inner.join('').replace(/\n+$/, ''));
      out.push('\n');
      return;
    }

    case 'p':
    case 'div':
    case 'blockquote':
    case 'h1': case 'h2': case 'h3':
    case 'h4': case 'h5': case 'h6':
      ensureNewline(out);
      for (const child of Array.from(el.childNodes)) walk(child, out, depth);
      ensureNewline(out);
      return;

    case 'pre':
      ensureBlankLine(out);
      out.push(el.textContent ?? '');
      ensureNewline(out);
      return;

    default:
      for (const child of Array.from(el.childNodes)) walk(child, out, depth);
  }
}

function ensureNewline(out: string[]): void {
  const last = out[out.length - 1];
  if (last !== undefined && !last.endsWith('\n')) out.push('\n');
}

function ensureBlankLine(out: string[]): void {
  ensureNewline(out);
  const joined = out.join('');
  if (!joined.endsWith('\n\n')) out.push('\n');
}

/**
 * Install a `copy` listener that reformats the clipboard text when the
 * current selection lies inside `container` AND contains `<ul>`/`<ol>`.
 * Other selections (plain prose, code snippets, selections outside the
 * container) pass through with the browser default, so we never disturb
 * cases that already work.
 *
 * The listener is attached on `document` rather than `container` because
 * some browsers / WebView builds fire `copy` on the active element only
 * (which may be `body` when no descendant has focus), and that event
 * doesn't bubble through a nested container.
 *
 * Returns a teardown function — call it from the effect cleanup.
 */
export function installListAwareCopy(container: HTMLElement): () => void {
  const handler = (e: ClipboardEvent) => {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
    const range = sel.getRangeAt(0);
    if (!container.contains(range.commonAncestorContainer)) return;
    const frag = range.cloneContents();
    if (!frag.querySelector('ul, ol, li')) return; // nothing list-y → default copy
    const div = document.createElement('div');
    div.appendChild(frag);
    const text = htmlToText(div.innerHTML);
    if (!text) return;
    e.clipboardData?.setData('text/plain', text);
    e.preventDefault();
  };
  document.addEventListener('copy', handler, true);
  return () => document.removeEventListener('copy', handler, true);
}
