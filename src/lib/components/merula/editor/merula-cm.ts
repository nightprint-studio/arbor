/**
 * merula ↔ CodeMirror 6 glue, on top of the shared code editor (`shared/ui/code-editor`):
 *
 *   - **Active-hap highlight** — a StateField fed live from `activeHapsStore`; each sounding source
 *     range is tinted with its track colour.
 *   - **Advisory underlines** — out-of-scale notes and clip risks, with hover messages.
 *   - **Occurrence marks** — every use of the `let` / `fn` under the caret, reported to the arrangement.
 *   - **Diagnostics** — a helper that maps the backend's UTF-8 byte spans onto UTF-16 offsets.
 *   - **Feature bundle** — `createMerulaFeatureExtensions`, what the language descriptor's `editing` slot carries.
 *
 * Parsing, highlighting, gutters, search, lint, completion and go-to-declaration are the shared
 * editor's; this file adds only what is merula's. Every colour is an Arbor CSS variable.
 */

import { EditorView, ViewPlugin, Decoration, hoverTooltip } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import {
  StateField, StateEffect, RangeSetBuilder, Prec, type Extension,
} from '@codemirror/state';
import type { Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { parserReady, highlightTreeOf } from '$lib/components/shared/ui/code-editor/highlight';

import { laneColor } from '../palette';
import type { MerulaDiagnostic } from '$lib/ipc/merula/merula';
import {
  makeByteToU16, identifierAt, identifierUsages, instrumentNameAt, extractSymbols, type Tree,
} from './merula-lang';

// ── Syntax-highlight ViewPlugin ────────────────────────────────────────────────

/** Fired once the (async) grammar load completes — the shared editor's own event, re-exported under the
 *  name the merula plugins already use. */
export const merulaParserReady = parserReady;

/** Read the live syntax tree of an editor (null until the grammar has loaded and the first parse has
 *  run) — the tree the shared editor's highlighter already maintains. Used by go-to-decl, the
 *  occurrence plugin and the outline derivation. */
export function getMerulaTree(view: EditorView): Tree | null {
  return highlightTreeOf(view);
}

// ── Active-hap decorations (live, per-track colour) ────────────────────────────

/** One sounding source range in CodeMirror coordinates (UTF-16, doc-clamped). */
export interface ActiveHapMark { from: number; to: number; track: number; }

/** Push the current active-hap set (already converted + clamped) to the editor. */
export const setActiveHaps = StateEffect.define<ActiveHapMark[]>();

const activeHapsField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setActiveHaps)) continue;
      const builder = new RangeSetBuilder<Decoration>();
      const sorted = [...e.value].sort((a, b) => a.from - b.from || a.to - b.to);
      for (const h of sorted) {
        if (h.to <= h.from) continue;
        builder.add(h.from, h.to, Decoration.mark({
          class: 'cm-grv-hap',
          attributes: { style: `--grv-hap: ${laneColor(h.track)}` },
        }));
      }
      deco = builder.finish();
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Convert backend active-haps (UTF-8 byte spans) into doc-clamped CM marks for
 *  `src`. Spans past the current document (a different / edited file is showing)
 *  are dropped — the highlight only paints ranges that exist in this buffer. */
export function toActiveHapMarks(
  haps: { start: number; end: number; track: number }[],
  src: string,
): ActiveHapMark[] {
  const b2u = makeByteToU16(src);
  const len = src.length;
  const out: ActiveHapMark[] = [];
  for (const h of haps) {
    const from = b2u(h.start);
    const to = b2u(h.end);
    if (to > from && to <= len) out.push({ from, to, track: h.track });
  }
  return out;
}

// ── Editor warning underlines (out-of-scale notes · clip-risk events) ──────────
//
// A small family of advisory underline layers, separate from the active-hap
// highlight and the eval lint: the FE detectors mark source ranges, painted as a
// wavy underline (amber for out-of-key, red for clip-risk) with a message shown
// on hover. Each "kind" is its own layer (own colour + StateEffect) but they
// share the shape, so a factory builds them. Marks are kept as plain data (with
// their messages) so the hover can look one up at a position, and positions
// re-map through edits so the underline + hover track typing until the next pass.

/** One advisory mark: a CM offset range plus the message shown on hover. */
export interface WarnMark { from: number; to: number; message: string; }

/** Build one underline layer (a class-tinted mark) fed by its own StateEffect. */
function makeWarnLayer(cls: string) {
  const mark = Decoration.mark({ class: cls });
  const set = StateEffect.define<WarnMark[]>();
  const field = StateField.define<WarnMark[]>({
    create: () => [],
    update(marks, tr) {
      let next = marks;
      if (tr.docChanged) {
        next = marks
          .map((m) => ({ from: tr.changes.mapPos(m.from), to: tr.changes.mapPos(m.to, 1), message: m.message }))
          .filter((m) => m.to > m.from);
      }
      for (const e of tr.effects) if (e.is(set)) next = e.value;
      return next;
    },
    provide: (f) =>
      EditorView.decorations.from(f, (marks) => {
        const builder = new RangeSetBuilder<Decoration>();
        for (const m of [...marks].sort((a, b) => a.from - b.from || a.to - b.to)) {
          if (m.to > m.from) builder.add(m.from, m.to, mark);
        }
        return builder.finish();
      }),
  });
  return { set, field };
}

const offScaleLayer = makeWarnLayer('cm-grv-offscale');
const clipRiskLayer = makeWarnLayer('cm-grv-clip');

/** Push the out-of-scale note marks (amber) — notes outside the detected key. */
export const setOutOfScale = offScaleLayer.set;
/** Push the clip-risk event marks (red) — events whose level is likely to clip. */
export const setClipRisk = clipRiskLayer.set;

/** Hover any advisory mark → its message in a styled tooltip. */
const warnHover = hoverTooltip((view, pos) => {
  for (const layer of [offScaleLayer, clipRiskLayer]) {
    const marks = view.state.field(layer.field, false);
    const m = marks?.find((x) => pos >= x.from && pos <= x.to);
    if (!m) continue;
    return {
      pos: m.from,
      end: m.to,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'cm-grv-warn-tip';
        dom.textContent = m.message;
        return { dom };
      },
    };
  }
  return null;
});

/** Convert advisory marks (UTF-8 byte ranges + message) into doc-clamped CM marks
 *  for `src`. Ranges outside the current buffer are dropped (stale / other file). */
export function toWarnMarks(
  notes: { from: number; to: number; message: string }[],
  src: string,
): WarnMark[] {
  const b2u = makeByteToU16(src);
  const len = src.length;
  const out: WarnMark[] = [];
  for (const n of notes) {
    const from = b2u(n.from);
    const to = b2u(n.to);
    if (to > from && to <= len) out.push({ from, to, message: n.message });
  }
  return out;
}

// ── Occurrence highlight (identifier under caret → all its uses) ───────────────
//
// IntelliJ-style "highlight usages of the symbol at the caret": resting the caret
// on a `let`/`fn`/`import`/`$splice` name softly boxes every occurrence in the
// buffer. Pure: a self-contained ViewPlugin that reads the live tree (so it never
// dispatches mid-update) and *provides* its decorations. It also reports the
// current symbol via `onSymbol` so the host can light up the arrangement lanes
// that reference it — the editor↔DAW link. Only highlights when ≥2 occurrences
// exist (a lone declaration needn't glow).

const OCCUR_MARK = Decoration.mark({ class: 'cm-grv-occur' });

export interface OccurrenceOptions {
  /** Called when the symbol under the caret changes (null when off a name). */
  onSymbol?: (name: string | null, view: EditorView) => void;
  /** Called when the editor's main selection range changes (UTF-16 doc offsets);
   *  `null` when the selection is empty (a bare caret). Drives the editor→DAW
   *  region highlight: the arrangement boxes the haps whose source span overlaps. */
  onSelection?: (range: { from: number; to: number } | null) => void;
}

function occurrenceHighlight(opts: OccurrenceOptions): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      lastName: string | null = null;
      lastSel: string | null = null;

      constructor(view: EditorView) { this.recompute(view); }

      update(u: ViewUpdate) {
        const ready = u.transactions.some((tr) =>
          tr.effects.some((e) => e.is(merulaParserReady)));
        if (u.docChanged || u.selectionSet || ready) this.recompute(u.view);
      }

      private recompute(view: EditorView) {
        const tree = getMerulaTree(view);
        const head = view.state.selection.main.head;

        // Editor→DAW selection link: report the main selection range (null when
        // it's an empty caret), de-duped so it only fires on a real change.
        if (opts.onSelection) {
          const sel = view.state.selection.main;
          const range = sel.empty ? null : { from: sel.from, to: sel.to };
          const key = range ? `${range.from}:${range.to}` : null;
          if (key !== this.lastSel) {
            this.lastSel = key;
            opts.onSelection(range);
          }
        }
        // Boundary-tolerant (caret at a word's right edge resolves to the word).
        let name = tree
          ? (identifierAt(tree, head) ?? (head > 0 ? identifierAt(tree, head - 1) : null))
          : null;

        // Only a user-declared symbol (let / fn / import) is worth highlighting —
        // gating out builtins (`fast`, `par`, …) keeps the editor + the arrangement
        // link from lighting up on every combinator.
        if (tree && name) {
          const syms = extractSymbols(tree);
          if (!syms.defs.has(name) && !syms.imports.has(name)) name = null;
        }

        let deco: DecorationSet = Decoration.none;
        if (tree && name) {
          const uses = identifierUsages(tree, name);
          if (uses.length >= 2) {
            const builder = new RangeSetBuilder<Decoration>();
            for (const r of uses) if (r.to > r.from) builder.add(r.from, r.to, OCCUR_MARK);
            deco = builder.finish();
          }
        }
        this.decorations = deco;
        if (name !== this.lastName) {
          this.lastName = name;
          opts.onSymbol?.(name, view);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}

// ── Diagnostics (byte spans → CM lint) ─────────────────────────────────────────

/** Map the diagnostics store (UTF-8 byte offsets) onto CodeMirror `Diagnostic`s
 *  for `src`. A null span (whole-file error) lands at the document start. */
export function toCmDiagnostics(errors: MerulaDiagnostic[], src: string): CmDiagnostic[] {
  const b2u = makeByteToU16(src);
  const len = src.length;
  const out: CmDiagnostic[] = [];
  for (const e of errors) {
    let from = e.start != null ? b2u(e.start) : 0;
    let to = e.end != null ? b2u(e.end) : from;
    from = Math.max(0, Math.min(from, len));
    to = Math.max(from, Math.min(to, len));
    if (to === from) to = Math.min(len, from + 1); // give the marker some width
    out.push({ from, to, severity: e.severity, message: e.message, source: 'merula' });
  }
  return out;
}

// ── Theme ──────────────────────────────────────────────────────────────────────

/** Merula's additions to the shared editor theme: the DSL's own token colours, the autocomplete/hover docs
 *  block, and the live layers (active haps, occurrence marks, advisory underlines). The editor chrome
 *  (gutters, caret, selection, search) is the shared theme's. */
export const merulaTheme = EditorView.theme(
  {
    // ── Token palette ──
    //
    // Deliberate, hierarchical colouring rather than one-accent-per-token:
    //   • Music content (notes / chords / sounds) is the bright, eye-catching
    //     layer — it's what you read while composing.
    //   • Structure (island heads, combinators-via-fn, splice) uses the accent /
    //     teal family so the "shape" of a phrase stands out from its content.
    //   • Host scaffolding (keywords, defs, numbers, strings, comments) uses the
    //     standard Arbor syntax vars, quieter than the music.
    //   • Punctuation / mini-notation operators stay muted so the rhythm reads
    //     without visual noise.

    // Host scaffolding (quiet, conventional — same `--syntax-*` vars + fallbacks
    // the rest of Arbor's highlighting uses, so a theme overlay re-skins it too).
    '.cm-tok-comment': { color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
    '.cm-tok-string': { color: 'var(--syntax-string, #6a9956)' },
    '.cm-tok-number': { color: 'var(--syntax-number, #9876aa)' },
    '.cm-tok-keyword': { color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
    '.cm-tok-def': { color: 'var(--syntax-type, #4d78cc)' },
    '.cm-tok-ident': { color: 'var(--text-primary)' },

    // Structure: function/method calls (combinators, transforms) + island heads.
    '.cm-tok-fn': { color: 'var(--syntax-function, #ffc66d)' },
    '.cm-tok-island': { color: 'var(--accent, #56b6c2)', fontWeight: '700' },
    '.cm-tok-splice': { color: 'var(--accent, #56b6c2)', fontWeight: '600' },

    // Music content (the bright layer): notes warm, chords italic, sounds cool
    // teal. Merula-local vars (default to the music palette) keep them distinct
    // from host scaffolding while still theme-overridable.
    '.cm-tok-note': { color: 'var(--grv-syntax-note, #e5c07b)' },
    '.cm-tok-chord': { color: 'var(--grv-syntax-note, #e5c07b)', fontStyle: 'italic', fontWeight: '600' },
    '.cm-tok-sound': { color: 'var(--grv-syntax-sound, #56b6c2)' },

    // Mini-notation operators (~ _ * / ! @ …) — muted, so rhythm reads clean.
    '.cm-tok-mininote': { color: 'var(--text-muted)' },
    '.cm-tok-op': { color: 'var(--text-secondary)' },

    // ── Autocomplete + hover docs ──
    '.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip-autocomplete > ul > li': {
      fontFamily: 'var(--font-code)', fontSize: '12px',
      padding: '2px 6px', color: 'var(--text-primary)',
    },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--accent-subtle)', color: 'var(--text-primary)',
    },
    '.cm-completionLabel': { color: 'var(--text-primary)' },
    '.cm-completionDetail': { color: 'var(--text-muted)', fontStyle: 'normal', marginLeft: '0.6em' },
    '.cm-completionMatchedText': { color: 'var(--accent)', textDecoration: 'none', fontWeight: '700' },

    // Shared docs block (autocomplete info side-panel + hover tooltip).
    '.cm-grv-hover': { maxWidth: '420px' },
    '.cm-grv-doc': { padding: '4px 2px', maxWidth: '420px' },
    '.cm-grv-doc-sig': {
      fontFamily: 'var(--font-code)', fontSize: '12px', fontWeight: '600',
      color: 'var(--accent)', marginBottom: '4px', whiteSpace: 'pre-wrap',
    },
    '.cm-grv-doc-summary': {
      fontSize: '11.5px', lineHeight: '1.5', color: 'var(--text-secondary)',
    },
    '.cm-grv-doc-params': { margin: '6px 0 0', display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '1px 8px' },
    '.cm-grv-doc-params dt': {
      fontFamily: 'var(--font-code)', fontSize: '11px', fontWeight: '600',
      color: 'var(--grv-syntax-note, #e5c07b)', margin: '0',
    },
    '.cm-grv-doc-params dd': { margin: '0', fontSize: '11px', color: 'var(--text-secondary)', lineHeight: '1.45' },
    '.cm-grv-doc-example': {
      margin: '6px 0 0', padding: '5px 7px',
      background: 'var(--bg-input)', border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-sm, 4px)',
      fontFamily: 'var(--font-code)', fontSize: '11px', color: 'var(--text-primary)',
      whiteSpace: 'pre-wrap',
    },

    // Occurrence highlight — the symbol under the caret + all its uses. A soft
    // accent box, quieter than the selection-match colour so it reads as ambient.
    '.cm-grv-occur': {
      backgroundColor: 'color-mix(in srgb, var(--accent) 12%, transparent)',
      borderRadius: '2px',
      boxShadow: 'inset 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent)',
    },

    // Active-hap underline — tinted with the per-track colour via --grv-hap.
    '.cm-grv-hap': {
      backgroundColor: 'color-mix(in srgb, var(--grv-hap) 22%, transparent)',
      boxShadow: 'inset 0 -2px 0 0 var(--grv-hap)',
      borderRadius: '2px',
    },
    // Out-of-scale note — a soft amber wavy underline (a warning, not an error).
    '.cm-grv-offscale': {
      textDecoration: 'underline wavy var(--warning)',
      textDecorationSkipInk: 'none',
      textUnderlineOffset: '3px',
    },
    // Clip-risk event — a red wavy underline (a likely level overload).
    '.cm-grv-clip': {
      textDecoration: 'underline wavy var(--error)',
      textDecorationSkipInk: 'none',
      textUnderlineOffset: '3px',
    },
    // Hover bubble for an advisory mark (out-of-scale / clip-risk).
    '.cm-grv-warn-tip': {
      padding: '4px 8px',
      fontFamily: 'var(--font-ui-sans)',
      fontSize: '11.5px',
      lineHeight: '1.4',
      color: 'var(--text-primary)',
      maxWidth: '260px',
    },
    '.cm-tooltip:has(.cm-grv-warn-tip)': {
      backgroundColor: 'var(--bg-overlay)',
      border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-sm)',
      boxShadow: 'var(--shadow-popup)',
    },
  },
  { dark: true },
);

// ── Extensions factory ─────────────────────────────────────────────────────────

export interface MerulaFeatureOptions {
  /** Ctrl/Cmd+Click on an instrument name (`inst("…")` / `s("…")`) — host opens the preview/audition
   *  modal. Takes precedence over the editor's own go-to-declaration at that spot. */
  onPreview?: (name: string) => void;
  /** Called when the identifier under the caret changes (null when off a name) —
   *  drives the editor↔arrangement symbol highlight. */
  onSymbol?: (name: string | null, view: EditorView) => void;
  /** Called when the main selection range changes (UTF-16 doc offsets; null when
   *  empty) — drives the editor→DAW region highlight (box overlapping haps). */
  onSelection?: (range: { from: number; to: number } | null) => void;
}

/**
 * What merula adds on top of the shared code editor, as one extension to hand to the language
 * descriptor's `editing` slot: its token theme, the live layers (active haps, occurrence marks,
 * advisory underlines) and the instrument-preview click. Everything generic — history, gutters,
 * search, lint, completion, go-to-declaration — is the shared editor's.
 */
export function createMerulaFeatureExtensions(opts: MerulaFeatureOptions = {}): Extension {
  const exts: Extension[] = [
    // Above the shared theme, whose generic `cm-tok-*` rules it refines.
    Prec.highest(merulaTheme),
    // Reads the live tree the shared highlighter keeps; registered after it, so it sees the fresh parse.
    occurrenceHighlight({ onSymbol: opts.onSymbol, onSelection: opts.onSelection }),
    activeHapsField,
    offScaleLayer.field,
    clipRiskLayer.field,
    warnHover,
  ];
  if (opts.onPreview) {
    const { onPreview } = opts;
    exts.push(EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey)) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        const tree = getMerulaTree(view);
        if (!tree) return false;
        const sound = instrumentNameAt(tree, pos);
        if (!sound) return false;
        event.preventDefault();
        onPreview(sound);
        return true;
      },
    }));
  }
  return exts;
}
