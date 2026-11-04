/**
 * `[[wikilinks]]` and `#tags` for the shared markdown editor.
 *
 * Everything that knows about a *vault* arrives through {@link VaultLinkProvider}; this module
 * knows markdown and CodeMirror and nothing else (`docs/garrulus-design.md` §7.0). A host that
 * passes no provider gets no wikilink behaviour at all — Corvus's modal and Bennu's `.md` tabs
 * are untouched.
 *
 * What it adds, as extensions the host appends to the editor's own:
 *  • decoration — a wikilink renders as its text with the brackets (and the target half of an
 *    alias) hidden, accented when the note exists and dotted-warning when it does not; a `#tag`
 *    is a chip. The line holding the caret shows the raw source, like the rest of the live
 *    preview. Code (inline, fenced, indented) is skipped using the Lezer tree the editor
 *    already keeps, so a `[[` in a fence is text.
 *  • following — Ctrl/Cmd+Click, or Enter with the caret inside the link; Shift opens it aside.
 *  • completion — `[[` offers note titles ranked by the host, `#` offers the vault's tags.
 *  • `Ctrl+Shift+K` — insert `[[` and open the picker.
 *
 * The pure scanners ({@link scanWikilinks}, {@link scanTags}) are what the tests cover.
 */

import { syntaxTree } from '@codemirror/language';
import { Prec, type Extension, type Range, type EditorState } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  keymap,
} from '@codemirror/view';
import {
  startCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from '@codemirror/autocomplete';

/** What a vault tells the editor. All synchronous: the host keeps its catalogue in memory. */
export interface VaultLinkProvider {
  /** Does a note answer to this written target? (Heading and alias already stripped.) */
  resolves(target: string): boolean;
  /** Notes to offer for what has been typed after `[[`, best first. */
  suggest(query: string): { target: string; detail?: string }[];
  /** Follow a link. `split` is the Shift modifier. */
  open(target: string, heading: string | null, split: boolean): void;
  /** Every tag in the vault, lowercase, without the `#`. */
  tags(): string[];
  /** Follow a tag — a search, in practice. */
  openTag(tag: string): void;
}

// ─── Pure scanners ───────────────────────────────────────────────────────────

export interface WikiMatch {
  /** Offsets inside the scanned text, `[from, to)`. */
  from: number;
  to: number;
  embed: boolean;
  target: string;
  heading: string | null;
  alias: string | null;
  /** Where the visible part starts/ends: the alias if there is one, else target + heading. */
  textFrom: number;
  textTo: number;
}

const WIKILINK = /(!?)\[\[([^[\]|#\n]*)(?:#([^[\]|\n]*))?(?:\|([^[\]\n]*))?\]\]/g;

/** Every `[[target#heading|alias]]` / `![[embed]]` in one line of text. */
export function scanWikilinks(line: string): WikiMatch[] {
  const out: WikiMatch[] = [];
  for (const m of line.matchAll(WIKILINK)) {
    const from = m.index ?? 0;
    const to = from + m[0].length;
    const open = from + m[1].length + 2;
    const target = m[2].trim();
    if (!target) continue;
    const aliasLen = m[4] != null ? m[4].length + 1 : 0;
    const hasAlias = m[4] != null && m[4].length > 0;
    out.push({
      from,
      to,
      embed: m[1] === '!',
      target,
      heading: m[3] != null && m[3].trim() ? m[3].trim() : null,
      alias: hasAlias ? m[4].trim() : null,
      textFrom: hasAlias ? to - 2 - m[4].length : open,
      textTo: to - 2 - (hasAlias ? 0 : aliasLen),
    });
  }
  return out;
}

export interface TagMatch {
  from: number;
  to: number;
  /** Without the `#`. */
  tag: string;
}

const TAG = /(^|[\s(])#([\p{L}\p{N}_][\p{L}\p{N}_/-]*)/gu;

/** `#tag` / `#nested/tag` in one line. A run of digits alone is not a tag; a heading marker
 *  (`# Title`) never matches because the `#` must touch the word. */
export function scanTags(line: string): TagMatch[] {
  const out: TagMatch[] = [];
  for (const m of line.matchAll(TAG)) {
    const tag = m[2];
    if (/^\d+$/.test(tag)) continue;
    const from = (m.index ?? 0) + m[1].length;
    out.push({ from, to: from + 1 + tag.length, tag });
  }
  return out;
}

/** The note a target means, for the unresolved check: same normalisation as the index's
 *  resolver (`garrulus-index::graph::link_key`) — trim, drop `.md`, lowercase. */
export function linkKey(raw: string): string {
  const t = raw.trim();
  return (t.endsWith('.md') ? t.slice(0, -3) : t).trim().toLowerCase();
}

// ─── Editor wiring ───────────────────────────────────────────────────────────

const CODE_NODES = new Set(['InlineCode', 'FencedCode', 'CodeBlock', 'CodeText', 'CodeMark']);

function inCode(state: EditorState, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>['resolveInner']> | null =
    syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (CODE_NODES.has(n.name)) return true;
  }
  return false;
}

const hide = Decoration.replace({});

function buildDecorations(view: EditorView, provider: VaultLinkProvider): DecorationSet {
  const { state } = view;
  const caret = state.selection.main;
  const out: Range<Decoration>[] = [];
  let lastLine = -1;

  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      pos = line.to + 1;
      if (line.number === lastLine) continue;
      lastLine = line.number;
      if (!line.text.includes('[[') && !line.text.includes('#')) continue;

      for (const w of scanWikilinks(line.text)) {
        const a = line.from + w.from;
        const b = line.from + w.to;
        if (inCode(state, a)) continue;
        const unresolved = !provider.resolves(w.target);
        const cls =
          'cm-wikilink' + (unresolved ? ' cm-wikilink-unresolved' : '') + (w.embed ? ' cm-wikilink-embed' : '');
        const attrs = { 'data-wikilink': w.target, 'data-heading': w.heading ?? '' };
        const editing = caret.from <= b && caret.to >= a;
        if (editing) {
          out.push(Decoration.mark({ class: cls, attributes: attrs }).range(a, b));
          continue;
        }
        const tf = line.from + w.textFrom;
        const tt = line.from + w.textTo;
        if (tf > a) out.push(hide.range(a, tf));
        out.push(Decoration.mark({ class: cls, attributes: attrs }).range(tf, tt));
        if (b > tt) out.push(hide.range(tt, b));
      }

      for (const t of scanTags(line.text)) {
        const a = line.from + t.from;
        if (inCode(state, a)) continue;
        out.push(
          Decoration.mark({ class: 'cm-md-tag', attributes: { 'data-tag': t.tag } }).range(
            a,
            line.from + t.to,
          ),
        );
      }
    }
  }
  return Decoration.set(out, true);
}

function decorations(provider: VaultLinkProvider): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildDecorations(view, provider);
      }
      update(u: ViewUpdate) {
        if (
          u.docChanged ||
          u.viewportChanged ||
          u.selectionSet ||
          syntaxTree(u.startState) !== syntaxTree(u.state)
        ) {
          this.decorations = buildDecorations(u.view, provider);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}

/** The wikilink whose source the caret is strictly inside, if any. */
function wikilinkAtCaret(state: EditorState): WikiMatch | null {
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  for (const w of scanWikilinks(line.text)) {
    if (head > line.from + w.from && head < line.from + w.to && !inCode(state, line.from + w.from)) {
      return w;
    }
  }
  return null;
}

function follow(provider: VaultLinkProvider): Extension {
  const click = EditorView.domEventHandlers({
    mousedown(e) {
      if (!(e.ctrlKey || e.metaKey) || !(e.target instanceof HTMLElement)) return false;
      const link = e.target.closest<HTMLElement>('[data-wikilink]');
      if (link) {
        provider.open(link.dataset.wikilink ?? '', link.dataset.heading || null, e.shiftKey);
        e.preventDefault();
        return true;
      }
      const tag = e.target.closest<HTMLElement>('[data-tag]');
      if (tag?.dataset.tag) {
        provider.openTag(tag.dataset.tag);
        e.preventDefault();
        return true;
      }
      return false;
    },
  });
  const enter = Prec.high(
    keymap.of([
      {
        key: 'Enter',
        run: (view) => {
          const w = wikilinkAtCaret(view.state);
          if (!w) return false;
          provider.open(w.target, w.heading, false);
          return true;
        },
      },
      {
        key: 'Mod-Shift-k',
        run: (view) => {
          const { from, to } = view.state.selection.main;
          view.dispatch({
            changes: { from, to, insert: '[[' },
            selection: { anchor: from + 2 },
          });
          startCompletion(view);
          return true;
        },
      },
    ]),
  );
  return [click, enter];
}

function wikilinkSource(provider: VaultLinkProvider): CompletionSource {
  return (ctx: CompletionContext): CompletionResult | null => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const m = /\[\[([^[\]|#\n]*)$/.exec(before);
    if (!m || inCode(ctx.state, ctx.pos)) return null;
    const from = ctx.pos - m[1].length;
    const options: Completion[] = provider.suggest(m[1]).map((c) => ({
      label: c.target,
      detail: c.detail,
      type: 'text',
      apply: (view, completion, f, t) => {
        const closed = view.state.doc.sliceString(t, t + 2) === ']]';
        const insert = completion.label + (closed ? '' : ']]');
        view.dispatch({
          changes: { from: f, to: t, insert },
          selection: { anchor: f + insert.length + (closed ? 2 : 0) },
        });
      },
    }));
    // `filter: false` and no `validFor`: the host ranks, so every keystroke asks again.
    return options.length ? { from, options, filter: false } : null;
  };
}

function tagSource(provider: VaultLinkProvider): CompletionSource {
  return (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const m = /(?:^|[\s(])#([\p{L}\p{N}_/-]*)$/u.exec(before);
    if (!m || inCode(ctx.state, ctx.pos)) return null;
    // A lone `#` at the start of a line is a heading being typed, not a tag.
    if (m[1] === '' && before.trimStart() === '#') return null;
    const typed = m[1].toLowerCase();
    const options: Completion[] = provider
      .tags()
      .filter((t) => t.startsWith(typed))
      .slice(0, 50)
      .map((t) => ({ label: t, type: 'keyword' }));
    return options.length ? { from: ctx.pos - m[1].length, options } : null;
  };
}

const vaultTheme = EditorView.baseTheme({
  '.cm-wikilink': {
    color: 'var(--accent)',
    cursor: 'pointer',
    textDecoration: 'none',
    borderRadius: '2px',
  },
  '.cm-wikilink:hover': { textDecoration: 'underline' },
  '.cm-wikilink-unresolved': {
    color: 'var(--warning, #ffc44d)',
    textDecoration: 'underline dotted',
  },
  '.cm-wikilink-embed': { fontStyle: 'italic' },
  '.cm-md-tag': {
    color: 'var(--accent)',
    background: 'var(--accent-subtle)',
    borderRadius: '8px',
    padding: '0 5px',
    cursor: 'pointer',
  },
});

/** The extensions and completion sources a host appends to the markdown editor. */
export function vaultMarkdown(provider: VaultLinkProvider): {
  extensions: Extension[];
  completionSources: CompletionSource[];
} {
  return {
    extensions: [vaultTheme, decorations(provider), follow(provider)],
    completionSources: [wikilinkSource(provider), tagSource(provider)],
  };
}
