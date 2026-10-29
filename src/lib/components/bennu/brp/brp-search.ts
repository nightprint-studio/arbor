/**
 * The inspector's one search box: free text plus `has:` / `no:` component filters.
 *
 * `goblin has:Health no:Dead` — entities whose name contains "goblin", that have a `Health`
 * component and no `Dead` one. One field instead of a text box plus two pickers, because every part
 * of it is typed and the whole thing is reachable without leaving the keyboard.
 *
 * Component names are the short name (`Health`) or a full path (`my_game::Health`); resolving them
 * to registered type paths needs the game's registry, so that is the store's job — this only splits.
 */

export interface ParsedSearch {
  /** What is left once the filters are taken out: matched against names and ids. */
  text: string;
  has: string[];
  no: string[];
}

const HAS = /^(?:has|with):(.+)$/i;
const NO = /^(?:no|not|without):(.+)$/i;

export function parseSearch(raw: string): ParsedSearch {
  const out: ParsedSearch = { text: '', has: [], no: [] };
  const words: string[] = [];
  for (const token of raw.trim().split(/\s+/).filter(Boolean)) {
    const has = HAS.exec(token);
    const no = NO.exec(token);
    if (has) out.has.push(has[1]);
    else if (no) out.no.push(no[1]);
    else words.push(token);
  }
  out.text = words.join(' ');
  return out;
}

/** Nothing to filter by — the tree, not a flat result list, is what to show. */
export function isEmptySearch(p: ParsedSearch): boolean {
  return p.text === '' && p.has.length === 0 && p.no.length === 0;
}
