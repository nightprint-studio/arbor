/**
 * Syntax colouring for the URL bar, as host marks for the shared code editor: scheme, host, path, query keys
 * and values, and the `{{variables}}` — in a different style when nothing gives them a value. The classes
 * are the editor theme's own `cm-tok-*` ones, so the URL is coloured like the code beside it with no CSS here.
 */

export interface UrlMark {
  from: number;
  to: number;
  className: string;
}

const VARIABLE = /\{\{\s*([\w.\-]+)\s*\}\}/g;

/** The variable names in a text, in order, once each. */
export function variablesIn(text: string): string[] {
  return [...new Set([...text.matchAll(VARIABLE)].map((m) => m[1]))];
}

export function urlMarks(text: string, known: ReadonlySet<string>): UrlMark[] {
  const marks: UrlMark[] = [];
  const mark = (from: number, to: number, className: string) => { if (to > from) marks.push({ from, to, className }); };

  // The structure first, by position: a variable can stand for any part, so it is laid over the result.
  let i = 0;
  const scheme = /^[a-z][a-z0-9+.-]*:\/\//i.exec(text);
  if (scheme) {
    mark(0, scheme[0].length - 3, 'cm-tok-keyword');
    mark(scheme[0].length - 3, scheme[0].length, 'cm-tok-punctuation');
    i = scheme[0].length;
  }
  const query = text.indexOf('?', i);
  const pathEnd = query === -1 ? text.length : query;
  const slash = text.indexOf('/', i);
  // A leading variable (`{{baseUrl}}/x`) is the host and more; only a literal host is coloured as one.
  const hostEnd = slash === -1 || slash > pathEnd ? pathEnd : slash;
  if (scheme || !text.startsWith('{{')) mark(i, hostEnd, scheme ? 'cm-tok-type' : 'cm-tok-ident');
  mark(hostEnd, pathEnd, 'cm-tok-ident');

  if (query !== -1) {
    mark(query, query + 1, 'cm-tok-punctuation');
    let at = query + 1;
    for (const pair of text.slice(query + 1).split('&')) {
      const eq = pair.indexOf('=');
      const keyEnd = eq === -1 ? at + pair.length : at + eq;
      mark(at, keyEnd, 'cm-tok-field');
      if (eq !== -1) {
        mark(keyEnd, keyEnd + 1, 'cm-tok-punctuation');
        mark(keyEnd + 1, at + pair.length, 'cm-tok-string');
      }
      at += pair.length;
      if (at < text.length) { mark(at, at + 1, 'cm-tok-punctuation'); at += 1; }
    }
  }

  for (const m of text.matchAll(VARIABLE)) {
    const from = m.index ?? 0;
    marks.push({ from, to: from + m[0].length, className: known.has(m[1]) ? 'cm-tok-constant' : 'cm-tok-constant cm-tok-invalid' });
  }
  return marks;
}
