/**
 * The link between a code template and its preview, line by line: the output lines the caret's template line
 * wrote light up in the preview, scrolled into view when they are off screen; a selection in the preview
 * lights up, in the template, the lines that wrote it.
 *
 * The map is the backend's `source_lines` — for each output line, the template line that wrote it, `0` for
 * none. It belongs to one render, so while a template is being edited it can be a line off until the next
 * render lands.
 *
 * The preview owns the link, because the render it shows is where the map comes from; the editor on the other
 * side only hears which of its lines to light.
 */

import { untrack } from 'svelte';

/** The row class both ends of the link light a line with — styled once, in `BennuTemplatePreview`. */
export const TEMPLATE_LINK_CLASS = 'cm-template-link';

/** What the link needs of the preview's editor. */
export interface LinkedEditor {
  selectionRange: () => { from: number; to: number };
  isLineVisible: (line: number) => boolean;
  revealLine: (line: number) => void;
}

export interface TemplateLinkSource {
  /** The caret in the template — a new object for every move, a click back on the same line included. */
  caret: () => { line: number; col: number };
  /** The map of the render shown; `null` without one. */
  sourceLines: () => readonly number[] | null;
  /** The text the preview shows — the map's lines are its lines. */
  shown: () => string;
  /** The template lines to light; `[]` for none. */
  onSourceLines: (lines: number[]) => void;
}

/** Call while a component initialises: the effects live as long as it does. */
export function createTemplateLink(source: TemplateLinkSource) {
  let editor = $state<LinkedEditor | null>(null);
  /** Where the link was last asked from: the template's caret, or a selection in the preview. */
  let linkedFrom = $state<'template' | 'preview'>('template');
  /** The preview's lines that the caret's template line wrote. */
  const written = $derived.by(() => {
    const lines = source.sourceLines();
    return lines && linkedFrom === 'template' ? linesWrittenBy(lines, source.caret().line) : [];
  });
  const marks = $derived(written.map((line) => ({ line, className: TEMPLATE_LINK_CLASS })));
  /** What the preview last scrolled for. Plain: a render that changes nothing about it must not pull the
   *  preview back from where it was scrolled to, and only the effects below read or write it. */
  let revealedFor = '';

  // The caret moved in the template: the link is from there again, and what a preview selection lit goes dark.
  $effect(() => {
    void source.caret();
    untrack(() => {
      revealedFor = '';
      linkedFrom = 'template';
      source.onSourceLines([]);
    });
  });

  $effect(() => {
    const first = written[0];
    const key = first ? `${source.caret().line}:${first}` : '';
    if (!first || !editor || key === revealedFor) return;
    revealedFor = key;
    if (!editor.isLineVisible(first)) editor.revealLine(first);
  });

  return {
    /** The preview's editor — `bind:this` it here. */
    get editor() { return editor; },
    set editor(next: LinkedEditor | null) { editor = next; },
    /** The preview's lines to light, as `lineHighlights`. */
    get marks() { return marks; },
    /** After a click or a key in the preview — on the next frame, once the editor has moved its selection. */
    fromPreview() {
      requestAnimationFrame(() => {
        const lines = source.sourceLines();
        if (!editor || !lines) return;
        const { from, to } = editor.selectionRange();
        linkedFrom = 'preview';
        source.onSourceLines(linesThatWrote(source.shown(), lines, from, to));
      });
    },
  };
}

/** The output lines, 1-based, that template line `line` wrote. */
export function linesWrittenBy(sourceLines: readonly number[], line: number): number[] {
  const out: number[] = [];
  sourceLines.forEach((source, i) => {
    if (source === line) out.push(i + 1);
  });
  return out;
}

/** The template lines that wrote the output between UTF-16 offsets `from` and `to` of `text`, in order. */
export function linesThatWrote(text: string, sourceLines: readonly number[], from: number, to: number): number[] {
  const lines = new Set<number>();
  for (let line = lineAt(text, from); line <= lineAt(text, to); line++) {
    const source = sourceLines[line - 1];
    if (source) lines.add(source);
  }
  return [...lines].sort((a, b) => a - b);
}

function lineAt(text: string, offset: number): number {
  let line = 1;
  const end = Math.min(offset, text.length);
  for (let i = 0; i < end; i++) if (text.charCodeAt(i) === 10) line++;
  return line;
}
