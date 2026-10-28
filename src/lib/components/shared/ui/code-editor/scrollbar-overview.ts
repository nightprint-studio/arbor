/**
 * Scrollbar overview — the IntelliJ "error stripe" that replaces the minimap.
 *
 * A thin strip pinned to the right edge that (a) marks every lint diagnostic as a small
 * coloured bar at its proportional position, so you see at a glance WHERE the errors and
 * warnings are, and (b) on hover pops a small preview of the document around that position
 * (the IntelliJ scrollbar lens). Click / drag the strip to jump / scroll — it takes over the
 * vertical scroll affordance from the (hidden) native scrollbar, which is why it also draws
 * a **thumb**: a scroll affordance that never says where you are is half a scrollbar.
 *
 * Clicking lands ON a mark when you aimed at one. Proportional scrolling would put the line
 * near the click rather than at it, and "near" is exactly the difference between jumping to
 * an error and going looking for it — which is the trip this strip exists to save.
 *
 * App-agnostic and opt-in (a host enables it INSTEAD of the minimap). Reads diagnostics from
 * the `@codemirror/lint` state, so it stays in sync with whatever the host pushed.
 */

import { EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { forEachDiagnostic } from '@codemirror/lint';
import { ensureSyntaxTree } from '@codemirror/language';
import { highlightTree } from '@lezer/highlight';
import { esc, highlightToHtml } from './mini-highlight';
import { lezerHighlightStyle } from './theme';

/**
 * One line highlighted the way the buffer is, when the buffer's language is a Lezer / stream one (XML,
 * pom, mappers, YAML, JSON, SQL, …): the SAME parse tree and the SAME style the editor paints with, so
 * the lens can never colour a line differently from where it came from. `null` for a language with no
 * such tree (the Tree-sitter ones), which then use the approximate {@link highlightToHtml}.
 */
function lezerLineHtml(view: EditorView, lineNumber: number): string | null {
  const state = view.state;
  const line = state.doc.line(lineNumber);
  // Parsed on demand up to this line: a stream language only parses what was scrolled to, and the
  // lens shows lines that never were. Bounded, so a huge file cannot stall a pointer move.
  const tree = ensureSyntaxTree(state, line.to, 40);
  if (!tree || tree.length === 0) return null;

  // The spans the language paints, and the ones the HOST adds on top (a pom's coordinates, a `${…}`, a
  // query's tint): both are read off the editor, so the lens shows the line as the buffer does. Marks
  // are class-only decorations, which the lens rows carry under a `.cm-content` so the host's own
  // stylesheet (written as `.cm-content .cm-fw-…`) reaches them.
  const spans: { from: number; to: number; cls: string }[] = [];
  highlightTree(tree, lezerHighlightStyle, (from, to, cls) => spans.push({ from, to, cls }), line.from, line.to);
  for (const source of state.facet(EditorView.decorations)) {
    const set = typeof source === 'function' ? source(view) : source;
    set.between(line.from, line.to, (from, to, deco) => {
      const cls = (deco.spec as { class?: string }).class;
      if (cls && to > from) spans.push({ from: Math.max(from, line.from), to: Math.min(to, line.to), cls });
    });
  }

  // Cut the line at every span edge; each piece takes the classes of all spans covering it.
  const cuts = new Set<number>([line.from, line.to]);
  for (const s of spans) { cuts.add(s.from); cuts.add(s.to); }
  const points = [...cuts].sort((a, b) => a - b);
  let out = '';
  for (let i = 0; i + 1 < points.length; i++) {
    const [a, b] = [points[i], points[i + 1]];
    const cls = spans.filter((s) => s.from <= a && s.to >= b).map((s) => s.cls).join(' ');
    const text = esc(state.sliceDoc(a, b));
    out += cls ? `<span class="${cls}">${text}</span>` : text;
  }
  return out;
}

/** Width of the overview strip in px (a hair wider than a typical scrollbar so marks read). */
const STRIP_W = 14;
/** Lines of context shown above/below the hovered line in the preview lens. */
const PREVIEW_RADIUS = 6;
/** How far (px) a click may miss a mark and still count as aimed at it. Generous, because
 *  a 3px bar on a 14px strip is a hard target and the cost of over-reaching is landing one
 *  problem away — while the cost of under-reaching is the scroll-and-squint this replaces. */
const MARK_HIT_SLOP = 5;

/** A cheap signature of the current diagnostic set (count + folded positions/severities), so the
 *  strip only re-renders its marks when the diagnostics actually change — not on every keystroke. */
function diagnosticSignature(view: EditorView): string {
  let sig = '';
  forEachDiagnostic(view.state, (d, from) => {
    sig += `${from}:${d.severity[0]};`;
  });
  return sig;
}

export function scrollbarOverview(): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      readonly strip: HTMLElement;
      readonly marks: HTMLElement;
      readonly thumb: HTMLElement;
      readonly preview: HTMLElement;
      lastSig = '';
      dragging = false;
      /** Every rendered mark's document line, keyed by its position down the strip as a
       *  [0,1] fraction — what a click is matched against. */
      markLines: { frac: number; line: number }[] = [];

      constructor(readonly view: EditorView) {
        this.strip = document.createElement('div');
        this.strip.className = 'cm-overview';
        this.thumb = document.createElement('div');
        this.thumb.className = 'cm-overview-thumb';
        this.strip.appendChild(this.thumb);
        this.marks = document.createElement('div');
        this.marks.className = 'cm-overview-marks';
        this.strip.appendChild(this.marks);
        this.preview = document.createElement('div');
        this.preview.className = 'cm-overview-preview';
        this.preview.style.display = 'none';

        view.dom.appendChild(this.strip);
        view.dom.appendChild(this.preview);
        view.dom.classList.add('cm-has-overview');

        this.strip.addEventListener('pointerdown', this.onPointerDown);
        this.strip.addEventListener('pointermove', this.onPointerMove);
        this.strip.addEventListener('pointerleave', this.onPointerLeave);
        // A drag can end anywhere — a window-level pointerup clears the drag even off the strip.
        window.addEventListener('pointerup', this.onPointerUp);
        // The thumb follows the scroll, and a scroll is not a state change: CodeMirror does
        // not dispatch an update for it, so the element that owns the scroll is asked directly.
        view.scrollDOM.addEventListener('scroll', this.onScroll, { passive: true });

        this.renderMarks();
        this.renderThumb();
      }

      update(u: ViewUpdate) {
        // Marks: only when the doc or the diagnostic set changed (positions shift on edits).
        if (u.docChanged || u.geometryChanged) {
          this.renderMarks();
        } else {
          const sig = diagnosticSignature(u.view);
          if (sig !== this.lastSig) this.renderMarks();
        }
        if (u.docChanged || u.geometryChanged) this.renderThumb();
      }

      onScroll = () => this.renderThumb();

      /** Size and place the thumb from the scroller's own geometry. Hidden when the whole
       *  document fits — an unscrollable buffer has no position to report. */
      renderThumb() {
        const sc = this.view.scrollDOM;
        const range = sc.scrollHeight - sc.clientHeight;
        if (range <= 0 || sc.scrollHeight <= 0) {
          this.thumb.style.display = 'none';
          return;
        }
        this.thumb.style.display = 'block';
        // A floor in percent of the strip, so a 30k-line file still leaves something to grab.
        const height = Math.max((sc.clientHeight / sc.scrollHeight) * 100, 3);
        this.thumb.style.height = `${height}%`;
        this.thumb.style.top = `${(sc.scrollTop / range) * (100 - height)}%`;
      }

      /** The document line of the mark a click at `clientY` was aimed at, if any. */
      markAtY(clientY: number): number | null {
        const rect = this.strip.getBoundingClientRect();
        if (rect.height <= 0) return null;
        let best: { line: number; dist: number } | null = null;
        for (const m of this.markLines) {
          const dist = Math.abs(rect.top + m.frac * rect.height - clientY);
          if (dist <= MARK_HIT_SLOP && (!best || dist < best.dist)) best = { line: m.line, dist };
        }
        return best?.line ?? null;
      }

      /** Put the caret at the start of `line` and centre it — a jump you can then act on
       *  (F2, quick-fix, copy) rather than one you have to click into first. */
      jumpToLine(line: number) {
        const pos = this.view.state.doc.line(line).from;
        this.view.dispatch({
          selection: { anchor: pos },
          effects: EditorView.scrollIntoView(pos, { y: 'center' }),
        });
        this.view.focus();
      }

      /** The [0,1] vertical fraction of the strip a Y coordinate maps to. */
      fractionAtY(clientY: number): number {
        const rect = this.strip.getBoundingClientRect();
        const frac = rect.height > 0 ? (clientY - rect.top) / rect.height : 0;
        return Math.max(0, Math.min(1, frac));
      }

      /** The document line (1-based) at a strip fraction — for the hover preview. */
      lineAtFraction(frac: number): number {
        return Math.max(1, Math.min(this.view.state.doc.lines, Math.round(frac * this.view.state.doc.lines)));
      }

      /** Scroll the buffer so the strip fraction maps to the same fraction of the scroll range — a
       *  true proportional scrollbar drag. */
      scrollToFraction(frac: number) {
        const sc = this.view.scrollDOM;
        sc.scrollTop = frac * Math.max(0, sc.scrollHeight - sc.clientHeight);
      }

      onPointerDown = (e: PointerEvent) => {
        e.preventDefault();
        // Aimed at a mark → land on it exactly. Anywhere else → the strip is a scrollbar.
        const line = this.markAtY(e.clientY);
        if (line != null) {
          this.preview.style.display = 'none';
          this.jumpToLine(line);
          return;
        }
        this.dragging = true;
        this.strip.setPointerCapture(e.pointerId);
        this.scrollToFraction(this.fractionAtY(e.clientY));
      };

      onPointerMove = (e: PointerEvent) => {
        const frac = this.fractionAtY(e.clientY);
        if (this.dragging) {
          this.scrollToFraction(frac);
          return;
        }
        this.showPreview(this.lineAtFraction(frac), e.clientY);
      };

      onPointerLeave = () => {
        this.preview.style.display = 'none';
      };

      onPointerUp = (e: PointerEvent) => {
        if (this.dragging) {
          this.dragging = false;
          try { this.strip.releasePointerCapture(e.pointerId); } catch { /* already released */ }
        }
      };

      /** Render (or refresh) the diagnostic marks: one bar per diagnostic at its proportional Y. */
      renderMarks() {
        this.lastSig = diagnosticSignature(this.view);
        this.marks.textContent = '';
        this.markLines = [];
        const doc = this.view.state.doc;
        const total = Math.max(1, doc.lines);
        forEachDiagnostic(this.view.state, (d, from) => {
          const line = doc.lineAt(Math.max(0, Math.min(from, doc.length))).number;
          const frac = (line - 0.5) / total;
          const mark = document.createElement('div');
          mark.className = `cm-overview-mark cm-overview-${d.severity}`;
          mark.style.top = `${frac * 100}%`;
          this.marks.appendChild(mark);
          this.markLines.push({ frac, line });
        });
      }

      /** Show the preview lens for `line`, vertically anchored near the pointer. */
      showPreview(line: number, clientY: number) {
        const doc = this.view.state.doc;
        const from = Math.max(1, line - PREVIEW_RADIUS);
        const to = Math.min(doc.lines, line + PREVIEW_RADIUS);
        const rows: string[] = [];
        for (let n = from; n <= to; n++) {
          const cls = n === line ? 'cm-overview-preview-row cm-overview-preview-cur' : 'cm-overview-preview-row';
          // Highlighted like the buffer (the token classes are HTML-escaped inside `highlightToHtml`).
          const html = (lezerLineHtml(this.view, n) ?? highlightToHtml(doc.line(n).text)) || '&nbsp;';
          rows.push(`<div class="${cls}">${html}</div>`);
        }
        // Under a `.cm-content`, so the host's marks (styled as `.cm-content .cm-fw-…`) apply to the rows.
        this.preview.innerHTML = `<div class="cm-content cm-overview-rows">${rows.join('')}</div>`;
        this.preview.style.display = 'block';
        // Anchor the lens to the LEFT of the strip, vertically centred on the pointer but clamped
        // inside the editor box.
        const host = this.view.dom.getBoundingClientRect();
        const ph = this.preview.offsetHeight;
        let top = clientY - host.top - ph / 2;
        top = Math.max(4, Math.min(host.height - ph - 4, top));
        this.preview.style.top = `${top}px`;
      }

      destroy() {
        this.strip.removeEventListener('pointerdown', this.onPointerDown);
        this.strip.removeEventListener('pointermove', this.onPointerMove);
        this.strip.removeEventListener('pointerleave', this.onPointerLeave);
        window.removeEventListener('pointerup', this.onPointerUp);
        this.view.scrollDOM.removeEventListener('scroll', this.onScroll);
        this.strip.remove();
        this.preview.remove();
        this.view.dom.classList.remove('cm-has-overview');
      }
    },
  );

  return [plugin, overviewTheme];
}

const overviewTheme = EditorView.baseTheme({
  // Hide the native vertical scrollbar for an editor that owns an overview strip (the strip is the
  // scroll affordance now). The horizontal scrollbar is left intact.
  // `&` and not a leading descendant selector: `cm-has-overview` sits on the editor root itself, so
  // `.root .cm-has-overview …` could never match it — and the native scrollbar stayed, drawing a second
  // scrollbar beside the strip's own thumb.
  '&.cm-has-overview .cm-scroller': { scrollbarWidth: 'none' },
  '&.cm-has-overview .cm-scroller::-webkit-scrollbar:vertical': { width: '0' },
  // Reserve the strip's width as a right gutter so text never runs under it (a proper scrollbar
  // gutter, like the native scrollbar used to reserve).
  '&.cm-has-overview .cm-content': { paddingRight: `${STRIP_W}px` },

  '.cm-overview': {
    position: 'absolute',
    top: '0',
    right: '0',
    bottom: '0',
    width: `${STRIP_W}px`,
    zIndex: '5',
    cursor: 'pointer',
    background: 'transparent',
  },
  '.cm-overview:hover': { background: 'color-mix(in srgb, var(--text-muted, #808080) 8%, transparent)' },
  // The scroll position. Behind the marks (they are the reason to look at the strip) and
  // quiet enough not to read as a highlight.
  '.cm-overview-thumb': {
    position: 'absolute',
    left: '3px',
    right: '3px',
    borderRadius: '4px',
    background: 'color-mix(in srgb, var(--text-muted, #808080) 30%, transparent)',
    pointerEvents: 'none',
  },
  '.cm-overview:hover .cm-overview-thumb': {
    background: 'color-mix(in srgb, var(--text-muted, #808080) 48%, transparent)',
  },
  '.cm-overview-marks': { position: 'absolute', inset: '0', pointerEvents: 'none' },
  '.cm-overview-mark': {
    position: 'absolute',
    right: '2px',
    width: '8px',
    height: '3px',
    borderRadius: '1px',
    transform: 'translateY(-50%)',
  },
  '.cm-overview-error': { background: 'var(--danger, #e0524f)' },
  '.cm-overview-warning': { background: 'var(--warning, #d6a018)' },
  '.cm-overview-info': { background: 'var(--info, #4a9eff)' },

  '.cm-overview-preview': {
    position: 'absolute',
    right: `${STRIP_W + 6}px`,
    zIndex: '10',
    maxWidth: '520px',
    padding: '6px 8px',
    borderRadius: 'var(--radius-md, 6px)',
    background: 'var(--bg-elevated, #26262b)',
    border: '1px solid var(--border, #3a3a40)',
    boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
    fontFamily: 'var(--font-code)',
    fontSize: 'var(--font-size-xs)',
    lineHeight: '1.5',
    color: 'var(--text-primary)',
    whiteSpace: 'pre',
    overflow: 'hidden',
    pointerEvents: 'none',
  },
  // The rows sit in a `.cm-content` only to be reached by host stylesheets; none of the editor's own
  // content chrome (padding, caret, wrapping) belongs to the lens.
  '.cm-overview-preview .cm-overview-rows': { padding: '0', margin: '0', display: 'block', minHeight: '0', flex: 'none', caretColor: 'transparent' },
  '.cm-overview-preview-row': { overflow: 'hidden', textOverflow: 'ellipsis' },
  '.cm-overview-preview-cur': {
    color: 'var(--text, #e8e8e8)',
    background: 'color-mix(in srgb, var(--accent, #4a9eff) 18%, transparent)',
  },
});
