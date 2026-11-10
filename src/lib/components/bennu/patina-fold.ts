/**
 * Collasso di un `.ptn` oltre le graffe: il blocco dei `use` e le righe di commento `///` consecutive.
 *
 * Funzioni pure su `EditorState`, come `braceFold` di `patina-lang.ts`: nessuna vista, nessun albero. Il
 * linguaggio e' a stream, quindi "che cos'e' un `use`" lo dice la riga, non una grammatica.
 */

import type { EditorState } from '@codemirror/state';

export interface FoldRange { from: number; to: number }

/** `use a::b;` — l'inizio di un'istruzione di importazione, a inizio riga (indentata o no). */
const USE_LINE = /^\s*(?:pub\s+)?use\b/;
/** Un commento di documentazione: `///` o `//!` (non `////`, che e' un commento qualunque). */
const DOC_LINE = /^\s*\/\/[\/!](?!\/)/;

/**
 * Il blocco di `use` in testa al file: dal primo `use` (dopo la parola chiave, cosi' chiuso legge `use …`) alla
 * fine dell'ultima istruzione consecutiva. Righe vuote e commenti fra due `use` non interrompono il blocco; la
 * prima altra cosa si'. Un'istruzione `use` puo' occupare piu' righe (`use a::{\n b,\n c\n};`): finisce al `;`.
 * `null` se non c'e' o sta su una riga sola.
 */
export function useBlockRange(state: EditorState): FoldRange | null {
  const doc = state.doc;
  let first = -1;
  let lastEnd = -1;
  let n = 1;
  while (n <= doc.lines) {
    const line = doc.line(n);
    const text = line.text;
    if (text.trim() === '' || /^\s*\/\//.test(text)) { n++; continue; }
    if (!USE_LINE.test(text)) break;
    // finisce al primo `;` fuori da commenti di riga: per i `use` basta cercare il `;`
    let end = n;
    while (end <= doc.lines && !doc.line(end).text.replace(/\/\/.*$/, '').includes(';')) end++;
    if (end > doc.lines) break;
    if (first < 0) first = n;
    lastEnd = doc.line(end).to;
    n = end + 1;
  }
  if (first < 0) return null;
  const head = doc.line(first);
  const kw = /^\s*(?:pub\s+)?use\b/.exec(head.text)!;
  const from = head.from + kw[0].length;
  if (doc.lineAt(lastEnd).number <= first) return null;
  return { from, to: lastEnd };
}

/**
 * Una serie di righe `///` (o `//!`): si piega sulla prima, dalla fine di quella alla fine dell'ultima.
 * `lineStart` deve essere la prima riga della serie, altrimenti `null` (la freccia sta solo sulla testa).
 */
export function docCommentRange(state: EditorState, lineStart: number): FoldRange | null {
  const doc = state.doc;
  const line = doc.lineAt(lineStart);
  if (!DOC_LINE.test(line.text)) return null;
  const kind = line.text.trimStart().slice(0, 3);
  if (line.number > 1) {
    const prev = doc.line(line.number - 1).text;
    if (DOC_LINE.test(prev) && prev.trimStart().slice(0, 3) === kind) return null;
  }
  let last = line.number;
  while (last < doc.lines) {
    const next = doc.line(last + 1).text;
    if (DOC_LINE.test(next) && next.trimStart().slice(0, 3) === kind) last++;
    else break;
  }
  if (last === line.number) return null;
  return { from: line.to, to: doc.line(last).to };
}

/** Il servizio di collasso per una riga: `use`, poi commenti, (le graffe restano a `braceFold`). */
export function patinaExtraFold(state: EditorState, lineStart: number, lineEnd: number): FoldRange | null {
  const use = useBlockRange(state);
  if (use && use.from >= lineStart && use.from <= lineEnd) return use;
  return docCommentRange(state, lineStart);
}
