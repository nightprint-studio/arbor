/**
 * Fenced code blocks inside a message — found, split out, and removed.
 *
 * Pure text work, shared by the composer (the chips that list the blocks being written, the "am I
 * pasting inside a fence" check) and by the thread (a message of yours renders its blocks highlighted).
 * A fence opens on a line of three or more backticks with an optional language and closes on a line of
 * at least as many; one that is never closed runs to the end of the text, which is what a half-written
 * message looks like.
 */

export interface FencedBlock {
  /** The language word after the opening fence; empty when there is none. */
  language: string;
  /** The text between the fences, without the fence lines or the last newline. */
  code: string;
  /** Offset of the opening fence line. */
  start: number;
  /** Offset just after the closing fence line (and its newline), or the text's end when unclosed. */
  end: number;
  closed: boolean;
  lines: number;
}

export type MessageSegment =
  | { kind: 'text'; text: string }
  | { kind: 'code'; language: string; code: string };

const OPEN = /^ {0,3}(`{3,})[ \t]*([\w+#.-]*)[ \t]*$/;

/** Every fenced block of `text`, in order. */
export function findFences(text: string): FencedBlock[] {
  const blocks: FencedBlock[] = [];
  let offset = 0;
  let open: { start: number; ticks: number; language: string; bodyStart: number } | null = null;

  for (const raw of text.split('\n')) {
    const lineEnd = offset + raw.length;
    const next = Math.min(lineEnd + 1, text.length);
    const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw;

    if (!open) {
      const m = OPEN.exec(line);
      if (m) open = { start: offset, ticks: m[1].length, language: m[2], bodyStart: next };
    } else if (new RegExp(`^ {0,3}\`{${open.ticks},}[ \\t]*$`).test(line)) {
      blocks.push(build(text, open, offset, next, true));
      open = null;
    }
    offset = lineEnd + 1;
  }
  if (open) blocks.push(build(text, open, text.length, text.length, false));
  return blocks;
}

function build(
  text: string,
  open: { start: number; language: string; bodyStart: number },
  bodyEnd: number,
  end: number,
  closed: boolean,
): FencedBlock {
  const body = text.slice(open.bodyStart, Math.max(open.bodyStart, bodyEnd)).replace(/\r?\n$/, '');
  return { language: open.language, code: body, start: open.start, end, closed, lines: body === '' ? 0 : body.split('\n').length };
}

/** The message as alternating prose and code, so each can be drawn in its own way. */
export function splitFences(text: string): MessageSegment[] {
  const out: MessageSegment[] = [];
  let at = 0;
  for (const b of findFences(text)) {
    const before = text.slice(at, b.start).replace(/\n+$/, '');
    if (before.trim()) out.push({ kind: 'text', text: before });
    out.push({ kind: 'code', language: b.language, code: b.code });
    at = b.end;
  }
  const rest = text.slice(at).replace(/^\n+/, '');
  if (rest.trim()) out.push({ kind: 'text', text: rest });
  return out;
}

export const hasFence = (text: string): boolean => text.includes('```') && findFences(text).length > 0;

/** Is `pos` on a fence or between two — where a paste must stay as it is and not open a second fence? */
export function isInsideFence(text: string, pos: number): boolean {
  if (!text.includes('```')) return false;
  return findFences(text).some((b) => pos > b.start && (b.closed ? pos < b.end : pos <= text.length));
}

/** The text without `block`, and the blank line it left collapsed. */
export function removeFence(text: string, block: Pick<FencedBlock, 'start' | 'end'>): string {
  const head = text.slice(0, block.start).replace(/\n+$/, '');
  const tail = text.slice(block.end).replace(/^\n+/, '');
  return head && tail ? `${head}\n\n${tail}` : head || tail;
}

/** `code` spans become `<code>` and **bold** becomes `<strong>`; everything else stays text. No HTML is
 *  ever injected. */
export function inlineSpans(text: string): { code: boolean; bold: boolean; text: string }[] {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean).map((part) => {
    if (part.length > 1 && part.startsWith('`') && part.endsWith('`')) return { code: true, bold: false, text: part.slice(1, -1) };
    if (part.length > 4 && part.startsWith('**') && part.endsWith('**')) return { code: false, bold: true, text: part.slice(2, -2) };
    return { code: false, bold: false, text: part };
  });
}
