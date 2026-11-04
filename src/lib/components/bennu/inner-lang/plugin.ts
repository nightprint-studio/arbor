/**
 * The CodeMirror side of injected languages: a view plugin that colours each site's value inside
 * the host's string literal, and the descriptor wrapper that installs it.
 *
 * It paints *over* whatever the host highlighter did to the literal (a Java string is one
 * `cm-tok-string` span), as nested marks, so the string colour remains wherever the inner language
 * has nothing to say — the characters of a regex that are just characters.
 *
 * Only sites near the viewport are coloured; finding them still reads the whole file (a literal
 * scan is cheap, and a hint above the statement can sit far from the visible part of it).
 */

import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { Prec, type Extension, type Range } from '@codemirror/state';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { findSites, type DetectOptions, type InnerSite, type RegexDetection } from './detect';
import { highlightInner } from './highlight';
import { hostForPath, type Host } from './literals';
import { sourceRanges } from './units';

/** How far past the viewport (in characters) a site is still coloured, so a short scroll shows no bare patch. */
const MARGIN = 2000;

const marks = new Map<string, Decoration>();
function markFor(cls: string, message?: string): Decoration {
  const key = message ? `${cls}\u0000${message}` : cls;
  let m = marks.get(key);
  if (!m) {
    m = Decoration.mark({ class: `cm-tok-${cls}`, attributes: message ? { title: message } : undefined });
    marks.set(key, m);
  }
  return m;
}

/** The decorations for the sites of `src` that intersect `[lo, hi]`. */
export function innerDecorations(
  src: string, host: Host, opts: DetectOptions, lo: number, hi: number,
): DecorationSet {
  const out: Range<Decoration>[] = [];
  for (const site of findSites(src, host, opts)) {
    const first = site.literals[0];
    const last = site.literals[site.literals.length - 1];
    if (last.close < lo || first.open > hi) continue;
    paintSite(site, out);
  }
  return Decoration.set(out, true);
}

function paintSite(site: InnerSite, out: Range<Decoration>[]): void {
  const text = site.units.map((u) => u.ch).join('');
  // A fragment of a pattern built with `+` is not a pattern, so its balance proves nothing.
  const spans = highlightInner(site.lang, text, { flavour: site.flavour, flags: site.flags })
    .filter((s) => !(site.partial && s.cls === 'invalid'));
  for (const span of spans) {
    for (const r of sourceRanges(site.units, span.from, span.to)) {
      if (r.to > r.from) out.push(markFor(span.cls, span.message).range(r.from, r.to));
    }
  }
}

/** The view plugin for one host language. */
export function innerLanguageHighlight(host: Host, opts: DetectOptions): Extension {
  // High precedence puts these marks INSIDE the host's string mark: where both set a colour the inner
  // span wins, and the string's green must not paint over the language's own tokens.
  return Prec.high(ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) { this.decorations = this.build(view); }
      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view);
      }
      build(view: EditorView): DecorationSet {
        const { from, to } = view.viewport;
        return innerDecorations(view.state.doc.toString(), host, opts, from - MARGIN, to + MARGIN);
      }
    },
    { decorations: (v) => v.decorations },
  ));
}

// ── Descriptor wrapper ────────────────────────────────────────────────────────────

const wrapped = new WeakMap<LanguageDescriptor, Map<string, LanguageDescriptor>>();

/**
 * `desc` with injected-language colouring for the host language `path` implies, or `desc` itself
 * when the file's language has no injections. Cached: a descriptor's identity is what the editor
 * keys itself on, so a fresh object per call would remount it on every read.
 */
export function withInnerLanguages(
  desc: LanguageDescriptor, path: string | null, detection: RegexDetection,
): LanguageDescriptor {
  const host = hostForPath(path);
  if (!host) return desc;
  const key = `${host}:${detection}`;
  let byKey = wrapped.get(desc);
  if (!byKey) { byKey = new Map(); wrapped.set(desc, byKey); }
  let out = byKey.get(key);
  if (!out) {
    const ext = innerLanguageHighlight(host, { regexDetection: detection });
    out = { ...desc, extraHighlight: desc.extraHighlight ? [desc.extraHighlight, ext] : ext };
    byKey.set(key, out);
  }
  return out;
}
