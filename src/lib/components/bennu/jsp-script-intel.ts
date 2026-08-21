/**
 * Completion and hover for the JavaScript inside a JSP's `<script>` body.
 *
 * ## Why this is not the language server
 *
 * `typescript-language-server` serves `.ts` and `.js` files. A JSP is neither: it is a template
 * that *prints* JavaScript, and half of what is in a `<script>` body is `<%= %>` holes that no
 * JavaScript parser accepts. There is no server to route to and there never will be, so the
 * choice is between answering locally and answering not at all — and a legacy webapp keeps a
 * great deal of its behaviour in exactly these blocks.
 *
 * ## What "real" means here
 *
 * Two sources, both facts rather than guesses:
 *
 * 1. **The buffer's own declarations.** The `var` / `let` / `const` / `function` / `class` names
 *    and the parameters of the enclosing functions, read out of the script region. This is the
 *    half a completion list is actually used for — finishing a name you wrote forty lines up.
 * 2. **The browser's own API, by reflection.** After `document.` the members offered are the
 *    ones `document` really has, read off the object and its prototype chain in the very engine
 *    the page will run in. Not a vocabulary someone typed into a table here, so it cannot go
 *    stale and it cannot be incomplete in the way a hand-written list always is.
 *
 * Reflection starts only from an **allowlist** of roots, and that is the load-bearing part: the
 * WebView's `globalThis` is Arbor's own, and completing bare globals from it would offer
 * `__TAURI_INTERNALS__` to somebody editing a Struts page. The allowlist is the browser API a
 * page can count on, and nothing else Arbor happens to have.
 *
 * ## What it does not know
 *
 * jQuery and the page's other libraries. `$` is offered as a name because a legacy page has one,
 * but `$(…).` cannot be reflected — the library is not loaded in this window, and a curated list
 * of its methods would be exactly the hand-written vocabulary this file exists to avoid. Same
 * for a global a `<script src>` brings in.
 */

import type {
  Completion, CompletionContext, CompletionResult, CompletionSource,
} from '@codemirror/autocomplete';
import type { EditorState } from '@codemirror/state';
import type { EditorView, Tooltip } from '@codemirror/view';
import { hoverCardDom } from '$lib/components/shared/ui/code-editor';

/** How far back to look for the opening `<script`. A script block longer than this loses
 *  nothing but the two features here; the cap is what keeps a keystroke from scanning a
 *  400 KB page. */
const LOOKBACK = 200_000;

/**
 * The `<script>` body containing `pos`, or `null`.
 *
 * Lexical, not from the tree: a `CompletionSource` is handed the `EditorState`, not the live
 * syntax tree, which belongs to the highlight plugin. The scan is anchored on the *last*
 * `<script` before the caret and refuses if a `</script>` closed it — so the ordinary case is
 * two `indexOf`s.
 *
 * A `<script>` whose `type` is not JavaScript (`text/template`, `text/x-handlebars`, and the
 * other ways a page smuggles markup past the browser) is deliberately not one: offering DOM
 * members inside a Handlebars template would be confidently wrong.
 */
export function scriptRegionAt(state: EditorState, pos: number): { from: number; to: number } | null {
  const start = Math.max(0, pos - LOOKBACK);
  const before = state.doc.sliceString(start, pos);

  const open = before.toLowerCase().lastIndexOf('<script');
  if (open < 0) return null;
  const gt = before.indexOf('>', open);
  if (gt < 0) return null; // still inside the opening tag itself
  if (before.toLowerCase().indexOf('</script', gt) >= 0) return null; // that block already closed

  const tag = before.slice(open, gt);
  const type = /\stype\s*=\s*["']([^"']*)["']/i.exec(tag)?.[1]?.toLowerCase();
  if (type && !/^(text\/javascript|application\/javascript|module|text\/ecmascript)$/.test(type)) {
    return null;
  }

  const from = start + gt + 1;
  const rest = state.doc.sliceString(pos, Math.min(state.doc.length, pos + LOOKBACK));
  const close = rest.toLowerCase().indexOf('</script');
  return { from, to: close < 0 ? state.doc.length : pos + close };
}

// ── The buffer's own names ─────────────────────────────────────────────────────

/** A declaration found in the script region: what it is, and the line it was written on. */
export interface LocalName {
  name: string;
  kind: 'var' | 'function' | 'class' | 'param';
  /** The source line, trimmed — what the hover shows, because it is what was written. */
  line: string;
}

const DECL = /\b(var|let|const)\s+([A-Za-z_$][\w$]*)/g;
const FUNC = /\bfunction\s*\*?\s*([A-Za-z_$][\w$]*)\s*\(([^)]{0,400})\)/g;
const CLASS = /\bclass\s+([A-Za-z_$][\w$]*)/g;
/** `foo: function (a, b)` and `foo = function (a, b)` — how a jQuery-era page writes most of
 *  its functions, and the form a `function name(` scan misses entirely. */
const ANON = /([A-Za-z_$][\w$]*)\s*[:=]\s*function\s*\(([^)]{0,400})\)/g;

/**
 * Every name declared in `text`.
 *
 * A lexical scan, with two consequences worth stating rather than hiding: a name written inside
 * a string or a comment can slip in, and *every* declaration in the block is offered rather than
 * only those in scope. Both make the list a **superset** — nothing offered fails to exist — which
 * is the right way for this to be wrong.
 */
export function localNames(text: string): LocalName[] {
  const out = new Map<string, LocalName>();
  const lineOf = (index: number) => {
    const from = text.lastIndexOf('\n', index) + 1;
    const to = text.indexOf('\n', index);
    return text.slice(from, to < 0 ? undefined : to).trim().slice(0, 160);
  };
  const add = (name: string, kind: LocalName['kind'], at: number) => {
    if (!out.has(name)) out.set(name, { name, kind, line: lineOf(at) });
  };
  const params = (list: string, at: number) => {
    for (const raw of list.split(',')) {
      const p = raw.trim().replace(/=.*$/, '').trim();
      if (/^[A-Za-z_$][\w$]*$/.test(p)) add(p, 'param', at);
    }
  };

  for (const m of text.matchAll(DECL)) add(m[2], 'var', m.index!);
  for (const m of text.matchAll(CLASS)) add(m[1], 'class', m.index!);
  for (const m of text.matchAll(FUNC)) { add(m[1], 'function', m.index!); params(m[2], m.index!); }
  for (const m of text.matchAll(ANON)) { add(m[1], 'function', m.index!); params(m[2], m.index!); }
  return [...out.values()];
}

// ── The browser's own API ──────────────────────────────────────────────────────

/**
 * The objects a member completion may start from.
 *
 * An allowlist and not `globalThis`, for the reason in the module doc. Two groups: what a page
 * is handed (`document`, `location`, `localStorage`, …) and the language's own namespaces
 * (`Math`, `JSON`, `Object`, …). `window` is here because `window.` is how half of a legacy page
 * addresses everything, and its members really are the page's.
 */
const ROOTS = [
  'window', 'document', 'console', 'navigator', 'location', 'history', 'screen',
  'localStorage', 'sessionStorage', 'performance',
  'Math', 'JSON', 'Object', 'Array', 'String', 'Number', 'Boolean', 'Date', 'RegExp',
  'Promise', 'Map', 'Set', 'WeakMap', 'WeakSet', 'Symbol', 'Error', 'Intl',
];

/** The globals offered as bare names — the roots, plus the ones a page uses as values. */
const GLOBAL_NAMES = [
  ...ROOTS,
  'alert', 'confirm', 'prompt', 'setTimeout', 'setInterval', 'clearTimeout', 'clearInterval',
  'parseInt', 'parseFloat', 'isNaN', 'encodeURIComponent', 'decodeURIComponent', 'fetch',
  '$', 'jQuery',
];

const KEYWORDS = [
  'var', 'let', 'const', 'function', 'return', 'if', 'else', 'for', 'while', 'do', 'switch',
  'case', 'default', 'break', 'continue', 'new', 'this', 'typeof', 'instanceof', 'delete',
  'try', 'catch', 'finally', 'throw', 'class', 'extends', 'null', 'true', 'false', 'undefined',
];

/** Resolve a dotted path against the allowlisted roots. `null` for anything else — including a
 *  root that this engine does not have, which is how a headless test stays safe. */
function resolvePath(path: string[]): unknown {
  if (!path.length || !ROOTS.includes(path[0])) return null;
  let value: unknown = (globalThis as Record<string, unknown>)[path[0]];
  for (const step of path.slice(1)) {
    if (value == null || typeof value !== 'object' && typeof value !== 'function') return null;
    try {
      value = (value as Record<string, unknown>)[step];
    } catch {
      return null; // a getter that throws on this engine — not our business to make it
    }
  }
  return value ?? null;
}

/** What kind of member `key` is, WITHOUT reading it.
 *
 *  Reading would invoke the getter, and a DOM getter can be expensive (`offsetTop` forces
 *  layout) or can throw. A descriptor says everything the icon needs. */
function memberKind(owner: object, key: string): string {
  for (let o: object | null = owner; o; o = Object.getPrototypeOf(o)) {
    const d = Object.getOwnPropertyDescriptor(o, key);
    if (!d) continue;
    if (d.get) return 'property';
    return typeof d.value === 'function' ? 'method' : 'property';
  }
  return 'property';
}

/** Every member name on `owner` and its prototype chain, identifiers only. */
function memberNames(owner: object): string[] {
  const out = new Set<string>();
  for (let o: object | null = owner; o && o !== Object.prototype; o = Object.getPrototypeOf(o)) {
    for (const k of Object.getOwnPropertyNames(o)) {
      if (/^[A-Za-z_$][\w$]*$/.test(k)) out.add(k);
    }
  }
  return [...out];
}

// ── Completion ────────────────────────────────────────────────────────────────

/** The dotted path immediately before `pos`, e.g. `document.body.` → `['document','body']`. */
function pathBefore(text: string): string[] | null {
  const m = /((?:[A-Za-z_$][\w$]*\s*\.\s*)+)[A-Za-z_$][\w$]*$|((?:[A-Za-z_$][\w$]*\s*\.\s*)+)$/.exec(text);
  const chain = m?.[1] ?? m?.[2];
  if (!chain) return null;
  return chain.split('.').map((s) => s.trim()).filter(Boolean);
}

/**
 * Completion inside a JSP's `<script>`.
 *
 * Returns `null` outside one, which is what lets it sit in front of the markup source: the two
 * answer disjoint positions, and the script check has to go first because a `<` in JavaScript
 * (`if (a < b)`) reads to the markup tokenizer as an unclosed tag — which is how a taglib list
 * used to appear in the middle of a comparison.
 */
export const jspScriptCompletion: CompletionSource = (
  ctx: CompletionContext,
): CompletionResult | null => {
  const region = scriptRegionAt(ctx.state, ctx.pos);
  if (!region) return null;

  const word = ctx.matchBefore(/[\w$]*$/);
  const line = ctx.state.doc.sliceString(Math.max(region.from, ctx.pos - 300), ctx.pos);
  const path = pathBefore(line);

  // After a dot: the real members of the real object, when it is one we may reflect on.
  if (path) {
    const owner = resolvePath(path);
    if (owner == null || (typeof owner !== 'object' && typeof owner !== 'function')) return null;
    const options: Completion[] = memberNames(owner as object).map((name) => ({
      label: name,
      type: memberKind(owner as object, name),
      detail: path.join('.'),
    }));
    if (!options.length) return null;
    return { from: word ? word.from : ctx.pos, options, validFor: /^[\w$]*$/ };
  }

  if (!ctx.explicit && (!word || word.from === word.to)) return null;

  const text = ctx.state.doc.sliceString(region.from, region.to);
  const options: Completion[] = [
    ...localNames(text).map((l) => ({
      label: l.name,
      type: l.kind === 'function' ? 'function' : l.kind === 'class' ? 'class' : 'variable',
      detail: l.kind,
      // The buffer's own names first: they are the ones being reached for, and a browser API
      // list is long enough to bury them otherwise.
      boost: 20,
    })),
    ...GLOBAL_NAMES.map((g) => ({ label: g, type: 'variable', detail: 'global' })),
    ...KEYWORDS.map((k) => ({ label: k, type: 'keyword' })),
  ];
  return { from: word ? word.from : ctx.pos, options, validFor: /^[\w$]*$/ };
};

// ── Hover ─────────────────────────────────────────────────────────────────────

/** The identifier (and any dotted receiver) under `pos`. */
function tokenAt(state: EditorState, pos: number): { path: string[]; from: number; to: number } | null {
  const line = state.doc.lineAt(pos);
  const text = line.text;
  let i = pos - line.from;
  let j = i;
  const isWord = (c: string) => /[\w$]/.test(c);
  while (i > 0 && isWord(text[i - 1])) i--;
  while (j < text.length && isWord(text[j])) j++;
  if (i === j) return null;
  const before = text.slice(0, i);
  const chain = /((?:[A-Za-z_$][\w$]*\s*\.\s*)+)$/.exec(before)?.[1] ?? '';
  const path = [...chain.split('.').map((s) => s.trim()).filter(Boolean), text.slice(i, j)];
  return { path, from: line.from + i, to: line.from + j };
}

/** A function's signature as the engine knows it: its name and how many parameters it declares.
 *  `Function.prototype.length` is a fact; the parameter *names* of a native function are not
 *  available at all, so the card says what is knowable and does not invent the rest. */
function describe(value: unknown, name: string): { signature: string; kind: string } {
  if (typeof value === 'function') {
    const arity = (value as { length: number }).length;
    const args = arity === 0 ? '' : Array.from({ length: arity }, (_, i) => `arg${i + 1}`).join(', ');
    return { signature: `${name}(${args})`, kind: 'function' };
  }
  if (value === null) return { signature: name, kind: 'null' };
  if (typeof value === 'object') {
    const ctor = (value as object).constructor?.name;
    return { signature: name, kind: ctor ? ctor : 'object' };
  }
  return { signature: name, kind: typeof value };
}

/**
 * Hover inside a JSP's `<script>`.
 *
 * A declaration in the buffer shows **the line it was written on** — the most useful thing there
 * is about a local, and one that cannot be wrong. A browser API shows what the engine says it
 * is: `document.getElementById` is a function of one argument because that is what the function
 * object reports, not because a table here says so.
 */
export function jspScriptHover(view: EditorView, pos: number): Tooltip | null {
  const region = scriptRegionAt(view.state, pos);
  if (!region) return null;
  const token = tokenAt(view.state, pos);
  if (!token) return null;

  const name = token.path[token.path.length - 1];

  if (token.path.length === 1) {
    const local = localNames(view.state.doc.sliceString(region.from, region.to))
      .find((l) => l.name === name);
    if (local) {
      return {
        pos: token.from, end: token.to, above: true,
        create: () => ({ dom: hoverCardDom({ signature: local.line, kind: local.kind }) }),
      };
    }
  }

  const value = resolvePath(token.path);
  if (value == null && !ROOTS.includes(name)) return null;
  const resolved = token.path.length === 1 ? (globalThis as Record<string, unknown>)[name] : value;
  if (resolved === undefined) return null;
  const { signature, kind } = describe(resolved, token.path.join('.'));
  return {
    pos: token.from, end: token.to, above: true,
    create: () => ({ dom: hoverCardDom({ signature, kind, doc: 'Read from this browser engine.' }) }),
  };
}
