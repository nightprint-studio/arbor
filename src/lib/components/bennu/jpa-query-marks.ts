/**
 * Colouring for the SQL / JPQL / HQL text inside a Java string.
 *
 * The backend finds *where* a query is and *which* language it is (`jpa.query` / `jpa.query.native`
 * — an `@Query`, a text block, `createNativeQuery`, a `// language=sql` string). It no longer cuts the
 * text into tokens: that is the shared lexer's job (`code-editor/sql-lexer`), the same one the Picus
 * editor and Bennu's `.sql` files use, so a query reads the same wherever it is written.
 *
 * Pure: `(source, range, language) → marks`, in the editor's UTF-16 offsets.
 */

import { sqlStream } from '$lib/components/shared/ui/code-editor/sql-lexer';
import type { StreamParser } from '@codemirror/language';
import { streamSpans } from './inner-lang/highlight';

export interface QueryMark {
  from: number;
  to: number;
  /** `cm-fw cm-fw-jpa-query-<kind>` — the class family the editor's stylesheet colours. */
  className: string;
}

/** Lexer token class → the `<kind>` suffix of the stylesheet class. Anything absent stays plain. */
const KIND: Record<string, string> = {
  keyword: 'keyword',
  type: 'type',
  function: 'function',
  string: 'string',
  number: 'number',
  comment: 'comment',
  annotation: 'param', // a bind parameter, or an optimizer hint
  field: 'field', // a name after a dot: a column or an entity attribute
  constant: 'constant',
  operator: 'operator',
};

/**
 * Java escapes (`\"`, `\n`, `\\`) blanked to the same number of characters, so the lexer neither
 * opens a quoted identifier at an escaped quote nor reads a backslash as part of a word — and every
 * offset still lands where it was.
 */
function withoutJavaEscapes(text: string): string {
  return text.replace(/\\[\s\S]/g, '  ');
}

/** The marks for the query occupying `[from, to)` of `src`. */
export function queryMarks(src: string, from: number, to: number, native: boolean): QueryMark[] {
  const text = withoutJavaEscapes(src.slice(from, to));
  const parser = sqlStream(native ? 'portable' : 'jpql') as StreamParser<unknown>;
  const out: QueryMark[] = [];
  for (const span of streamSpans(parser, text)) {
    const kind = KIND[span.cls];
    if (kind) out.push({ from: from + span.from, to: from + span.to, className: `cm-fw cm-fw-jpa-query-${kind}` });
  }
  return out;
}
