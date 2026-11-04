/**
 * The editor → Regex Tester hand-off: decide what to open (the pattern under the caret, the
 * selection, or nothing) and how to write an edit back.
 *
 * Kept out of the editor component because it is all pure text work — the component only supplies
 * the buffer, the caret and a way to replace a range.
 */

import { regexSiteAt, type RegexDetection } from '../inner-lang/detect';
import { hostForPath } from '../inner-lang/literals';
import { unitsText } from '../inner-lang/units';
import { rewriteSite } from '../inner-lang/write-back';
import type { RegexOpenRequest } from '$lib/stores/bennu/regex.svelte';
import type { Flavour } from './types';

export interface EditorAccess {
  path: string | null;
  /** The buffer right now. */
  text: () => string;
  /** The primary selection, UTF-16 offsets. */
  selection: () => { from: number; to: number };
  /** Replace a UTF-16 range of the buffer. */
  replace: (from: number, to: number, text: string) => void;
  detection: RegexDetection;
}

const flavourOf = (path: string | null): Flavour => {
  const host = hostForPath(path);
  return host === 'js' ? 'js' : host === 'rust' ? 'rust' : 'java';
};

/** Whether the caret is in a string literal the tester could open. */
export function literalUnderCaret(a: EditorAccess): boolean {
  const host = hostForPath(a.path);
  if (!host) return false;
  const { from, to } = a.selection();
  return regexSiteAt(a.text(), host, from, to, a.detection) !== null;
}

/** What to open the tester with. Never null: with nothing to read it opens empty. */
export function regexRequestFor(a: EditorAccess): RegexOpenRequest {
  const flavour = flavourOf(a.path);
  const host = hostForPath(a.path);
  const src = a.text();
  const { from, to } = a.selection();
  const site = host ? regexSiteAt(src, host, from, to, a.detection) : null;

  if (site) {
    const original = src.slice(site.literals[0].start, site.literals[site.literals.length - 1].end);
    const startAt = site.literals[0].start;
    return {
      pattern: unitsText(site.units),
      flavour: site.flavour,
      flags: site.flags,
      partial: site.partial,
      target: {
        file: a.path ?? '',
        apply: (pattern) => {
          const current = a.text();
          // The literal must still be exactly what the tester read; otherwise the edit would land elsewhere.
          if (current.slice(startAt, startAt + original.length) !== original) return false;
          const edit = rewriteSite(current, site, pattern);
          if (!edit) return false;
          a.replace(edit.from, edit.to, edit.text);
          return true;
        },
      },
    };
  }
  // Not in a literal: a selection is a pattern worth trying, but there is nowhere to write it back.
  const picked = from === to ? '' : src.slice(from, to);
  return { pattern: picked.includes('\n') ? '' : picked, flavour, flags: flavour === 'js' ? 'g' : '', target: null };
}
