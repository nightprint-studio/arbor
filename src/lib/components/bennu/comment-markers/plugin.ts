/**
 * The CodeMirror side of comment markers: colours each `TODO` / `FIXME`… found in a comment, points at it on
 * the scrollbar strip and, when asked, in the gutter — and installs itself on a language descriptor.
 *
 * Colours are theme variables (the tone → variable table is the stylesheet below), never literals.
 */

import { Decoration, EditorView, GutterMarker, gutter, type DecorationSet } from '@codemirror/view';
import { Prec, StateField, type EditorState, type Extension } from '@codemirror/state';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { overviewMarkSource, type OverviewMark } from '$lib/components/shared/ui/code-editor/scrollbar-overview';
import { knowsComments } from './comments';
import { findMarkers, type MarkerHit, type MarkerOptions, type MarkerTone } from './markers';

/** Past this a document is left alone: the scan is linear and runs on every edit. */
const MAX_CHARS = 2_000_000;

export interface MarkerSettings extends MarkerOptions {
  /** A dot in the gutter on each line that holds a marker. */
  gutter: boolean;
}

const marks = new Map<MarkerTone, Decoration>();
function markFor(tone: MarkerTone): Decoration {
  let m = marks.get(tone);
  if (!m) {
    m = Decoration.mark({ class: `cm-comment-marker cm-comment-marker-${tone}` });
    marks.set(tone, m);
  }
  return m;
}

interface Found {
  hits: MarkerHit[];
  deco: DecorationSet;
  /** The tone of the first marker on each line. */
  tonesByLine: Map<number, MarkerTone>;
}

const EMPTY: Found = { hits: [], deco: Decoration.none, tonesByLine: new Map() };

function scan(state: EditorState, path: string, settings: MarkerSettings): Found {
  if (state.doc.length > MAX_CHARS) return EMPTY;
  const hits = findMarkers(state.doc.toString(), path, settings);
  if (!hits.length) return EMPTY;
  const tonesByLine = new Map<number, MarkerTone>();
  for (const h of hits) if (!tonesByLine.has(h.line)) tonesByLine.set(h.line, h.tone);
  return { hits, tonesByLine, deco: Decoration.set(hits.map((h) => markFor(h.tone).range(h.from, h.to))) };
}

class DotMarker extends GutterMarker {
  constructor(readonly tone: MarkerTone) { super(); }
  eq(other: GutterMarker) { return other instanceof DotMarker && other.tone === this.tone; }
  toDOM() {
    const dot = document.createElement('span');
    dot.className = `cm-comment-marker-dot cm-comment-marker-dot-${this.tone}`;
    return dot;
  }
}

const TONES: MarkerTone[] = ['accent', 'danger', 'warning', 'info', 'success', 'muted'];

const toneColour = (tone: MarkerTone) => (tone === 'muted' ? 'var(--text-muted)' : `var(--${tone})`);

function themeRules(): Record<string, Record<string, string>> {
  const rules: Record<string, Record<string, string>> = {
    '.cm-comment-marker': { fontWeight: '700', fontStyle: 'normal', borderRadius: '3px' },
    '.cm-comment-marker-dot': { display: 'inline-block', width: '6px', height: '6px', borderRadius: '50%', verticalAlign: 'middle' },
    '.cm-comment-marker-gutter .cm-gutterElement': { display: 'flex', alignItems: 'center', justifyContent: 'center' },
    '.cm-comment-marker-gutter': { minWidth: '12px' },
  };
  for (const tone of TONES) {
    rules[`.cm-comment-marker-${tone}`] = {
      color: toneColour(tone),
      background: `color-mix(in srgb, ${toneColour(tone)} 16%, transparent)`,
    };
    rules[`.cm-comment-marker-dot-${tone}`] = { background: toneColour(tone) };
    rules[`.cm-overview-mark.cm-overview-marker-${tone}`] = { background: toneColour(tone) };
  }
  return rules;
}

const theme = EditorView.baseTheme(themeRules());

/** The extension for one file's language: decorations, strip marks and the optional gutter. */
export function commentMarkers(path: string, settings: MarkerSettings): Extension {
  const field = StateField.define<Found>({
    create: (state) => scan(state, path, settings),
    update: (found, tr) => (tr.docChanged ? scan(tr.state, path, settings) : found),
    provide: (f) => [
      EditorView.decorations.from(f, (found) => found.deco),
      overviewMarkSource.of((state): readonly OverviewMark[] =>
        state.field(f).hits.map((h) => ({ line: h.line, cls: `cm-overview-marker-${h.tone}` }))),
    ],
  });
  const exts: Extension[] = [field, theme];
  if (settings.gutter) {
    exts.push(gutter({
      class: 'cm-comment-marker-gutter',
      lineMarker(view, line) {
        const tone = view.state.field(field).tonesByLine.get(view.state.doc.lineAt(line.from).number);
        return tone ? new DotMarker(tone) : null;
      },
      lineMarkerChange: (u) => u.docChanged,
      initialSpacer: () => new DotMarker('accent'),
    }));
  }
  // High precedence puts the mark INSIDE the comment's own, so the marker's colour wins over the comment grey.
  return Prec.high(exts);
}

// ── Descriptor wrapper ────────────────────────────────────────────────────────────

const wrapped = new WeakMap<LanguageDescriptor, Map<string, LanguageDescriptor>>();

const extensionOf = (path: string) => path.slice(path.lastIndexOf('.') + 1).toLowerCase();

/**
 * `desc` with comment markers for the file `path`, or `desc` itself when the setting is off or the file's
 * language has no comments we can read. Cached on the words, the rules and the file type: a descriptor's
 * identity is what the editor keys itself on, so a fresh object per call would remount it on every read.
 */
export function withCommentMarkers(
  desc: LanguageDescriptor, path: string | null, enabled: boolean, settings: MarkerSettings,
): LanguageDescriptor {
  if (!enabled || !path || !knowsComments(path)) return desc;
  const key = [extensionOf(path), settings.caseSensitive, settings.gutter, settings.keywords.join(',')].join('|');
  let byKey = wrapped.get(desc);
  if (!byKey) { byKey = new Map(); wrapped.set(desc, byKey); }
  let out = byKey.get(key);
  if (!out) {
    const ext = commentMarkers(path, settings);
    out = { ...desc, extraHighlight: desc.extraHighlight ? [desc.extraHighlight, ext] : ext };
    byKey.set(key, out);
  }
  return out;
}
