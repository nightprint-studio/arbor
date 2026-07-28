/**
 * SQL hover — the facts about the identifier under the pointer.
 *
 * Everything shown here is read straight out of the catalogue the connection
 * already reported: a column's type, whether it accepts NULL, its default, the
 * foreign key it points at; a table's kind, column count and row estimate; a
 * sequence's last value. Nothing is computed, nothing is inferred, and when the
 * identifier resolves to nothing known the card simply does not appear — a hover
 * that says "unknown" is a hover that trained you to stop reading it.
 *
 * Rendering goes through the shared `.cm-hover-card` classes in the editor theme,
 * the same card Bennu's Java hover uses. No colours are chosen here.
 */

import type { EditorView, Tooltip } from '@codemirror/view';
import type { Column, Dialect, TableInfo } from '$lib/types/picus';
import { analyzeStatement, identOf, resolveQualifier, type StatementInfo } from './analysis';
import { ensureRelationDetail, schemaViewFor, type SchemaView } from './schema-view';
import { scanSql, statementAt, type SqlToken } from './tokens';

/** What the card will show, before it becomes DOM. */
interface HoverCard {
  title: string;
  meta: string[];
  doc: string[];
  from: number;
  to: number;
}

function card(dom: HoverCard): Tooltip {
  return {
    pos: dom.from,
    end: dom.to,
    above: true,
    create() {
      const root = document.createElement('div');
      root.className = 'cm-hover-card';

      const title = document.createElement('div');
      title.className = 'cm-hc-title';
      title.textContent = dom.title;
      root.appendChild(title);

      if (dom.meta.length) {
        const meta = document.createElement('div');
        meta.className = 'cm-hc-meta';
        meta.textContent = dom.meta.join('  ·  ');
        root.appendChild(meta);
      }
      if (dom.doc.length) {
        const doc = document.createElement('div');
        doc.className = 'cm-hc-doc';
        doc.textContent = dom.doc.join('\n');
        root.appendChild(doc);
      }
      return { dom: root };
    },
  };
}

/** The foreign key a column takes part in, and the column it points at. */
function foreignKeyOf(rel: TableInfo, column: Column): string | null {
  for (const fk of rel.foreignKeys ?? []) {
    const i = fk.columns.findIndex((c) => c.toUpperCase() === column.name.toUpperCase());
    if (i < 0) continue;
    const target = fk.referencedColumns[i] ?? fk.referencedColumns[0] ?? '';
    const onDelete = fk.onDelete ? ` on delete ${fk.onDelete.toLowerCase()}` : '';
    return `references ${fk.referencedTable}(${target})${onDelete}`;
  }
  return null;
}

function columnCard(rel: TableInfo, col: Column, from: number, to: number): HoverCard {
  const meta = [col.type];
  if (col.primaryKey) meta.push('primary key');
  meta.push(col.notNull ? 'NOT NULL' : 'nullable');

  const doc: string[] = [];
  if (col.defaultValue) doc.push(`default ${col.defaultValue}`);
  const fk = foreignKeyOf(rel, col);
  if (fk) doc.push(fk);

  return { title: `${rel.name}.${col.name}`, meta, doc, from, to };
}

function relationCard(rel: TableInfo, from: number, to: number, alias: string): HoverCard {
  const meta = [rel.kind, `${rel.columns.length} columns`];
  if (rel.estimatedRows != null) meta.push(`~${rel.estimatedRows.toLocaleString()} rows`);

  const doc: string[] = [];
  if (alias) doc.push(`aliased ${alias}`);
  const pk = rel.columns.filter((c) => c.primaryKey).map((c) => c.name);
  if (pk.length) doc.push(`primary key (${pk.join(', ')})`);
  for (const fk of rel.foreignKeys ?? []) {
    doc.push(`${fk.columns.join(', ')} → ${fk.referencedTable}(${fk.referencedColumns.join(', ')})`);
  }
  if (rel.kind === 'view' && rel.definition) doc.push(rel.definition.trim().slice(0, 300));

  return { title: rel.name, meta, doc, from, to };
}

/** The token under the pointer, when it is an identifier. */
function identifierAt(tokens: SqlToken[], pos: number): { index: number; token: SqlToken } | null {
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.from > pos) break;
    if (pos > t.to) continue;
    if (t.kind === 'word' || t.kind === 'quoted') return { index: i, token: t };
    return null;
  }
  return null;
}

/** A column of exactly one relation in scope — the only case where an unqualified
 *  word can be attributed to a table without guessing. */
function uniqueScopedColumn(
  info: StatementInfo, view: SchemaView, name: string,
): { rel: TableInfo; col: Column } | null {
  const hits: Array<{ rel: TableInfo; col: Column }> = [];
  for (const ref of info.relations) {
    if (ref.opaque || !ref.name) continue;
    const rel = view.relation(ref.name);
    const col = rel?.columns.find((c) => c.name.toUpperCase() === name.toUpperCase());
    if (rel && col && !hits.some((h) => h.rel.name === rel.name)) hits.push({ rel, col });
  }
  return hits.length === 1 ? hits[0] : null;
}

/**
 * Build the hover source for one dialect, bound to one connection.
 *
 * Async only because a column's foreign key may not have been read yet: the
 * snapshot carries no constraints, so the first hover on a table pulls its detail
 * in and every hover after that is instant.
 */
export function createSqlHover(dialect: Dialect, connectionId?: string) {
  return async function sqlHover(view: EditorView, pos: number): Promise<Tooltip | null> {
    const schema = schemaViewFor(connectionId);
    if (!schema.known) return null;

    const src = view.state.doc.toString();
    const { scan, statements } = scanSql(src, dialect);
    const hit = identifierAt(scan.tokens, pos);
    if (!hit) return null;

    const { token } = hit;
    const name = identOf(token);
    const stmt = statementAt(statements, pos);
    const info = stmt ? analyzeStatement(stmt, dialect) : null;

    // Is this identifier qualified — `c.NOME`, `CLIENTI.NOME`?
    const dotBefore = scan.tokens[hit.index - 1];
    const qualifierToken = dotBefore && dotBefore.kind === 'punct' && dotBefore.text === '.'
      ? scan.tokens[hit.index - 2]
      : undefined;
    const qualifier = identOf(qualifierToken);

    if (qualifier) {
      const ref = info ? resolveQualifier(info, qualifier) : null;
      if (ref?.opaque) return null;
      const rel = schema.relation(ref?.name || qualifier);
      if (!rel) return null;
      const col = rel.columns.find((c) => c.name.toUpperCase() === name.toUpperCase());
      if (!col) return null;
      const full = (await ensureRelationDetail(connectionId, rel.name)) ?? rel;
      return card(columnCard(full, col, token.from, token.to));
    }

    // A relation named in full.
    const rel = schema.relation(name);
    if (rel) {
      const full = (await ensureRelationDetail(connectionId, rel.name)) ?? rel;
      const alias = info?.relations.find((r) => r.name.toUpperCase() === name.toUpperCase())?.alias ?? '';
      return card(relationCard(full, token.from, token.to, alias));
    }

    // An alias — show what it stands for.
    const aliased = info?.relations.find((r) => r.alias.toUpperCase() === name.toUpperCase());
    if (aliased && !aliased.opaque) {
      const target = schema.relation(aliased.name);
      if (target) {
        const full = (await ensureRelationDetail(connectionId, target.name)) ?? target;
        return card(relationCard(full, token.from, token.to, aliased.alias));
      }
    }

    const seq = schema.sequence(name);
    if (seq) {
      return card({
        title: seq.name,
        meta: ['sequence', `last value ${seq.lastValue}`],
        doc: [
          `increment by ${seq.incrementBy}${seq.cycle ? ' · cycles' : ''}`,
          seq.cacheSize != null ? `cache ${seq.cacheSize}` : '',
        ].filter(Boolean),
        from: token.from,
        to: token.to,
      });
    }

    // An unqualified column, when exactly one relation in scope has it.
    if (info) {
      const unique = uniqueScopedColumn(info, schema, name);
      if (unique) {
        const full = (await ensureRelationDetail(connectionId, unique.rel.name)) ?? unique.rel;
        const col = full.columns.find((c) => c.name.toUpperCase() === name.toUpperCase()) ?? unique.col;
        return card(columnCard(full, col, token.from, token.to));
      }
    }

    return null;
  };
}
