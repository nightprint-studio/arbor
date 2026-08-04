/**
 * Live SQL diagnostics — the checks that need no round trip.
 *
 * There is one left: **a write on a read-only connection**. The server refuses it,
 * and saying so now saves the trip and the confusing error — and it needs nothing but
 * the text and a flag this side already has.
 *
 * The catalogue checks that used to live here — unknown table, unknown column,
 * ambiguous column — are **gone**. They reimplemented, against a cached schema, what
 * the database already knows exactly; the price was being wrong whenever the schema
 * had moved and silent whenever they were unsure. The database is now asked directly
 * (`picus_validate`, driven by `stores/picus/validation`), which is authoritative and
 * current. Two sources, kept apart because they fail apart: this one is synchronous
 * and never a matter of not knowing; that one is a round trip and quiet when it cannot
 * be made.
 *
 * The one exclusion that still matters here: **an abbreviation line is not SQL and is
 * not measured as SQL.** `s#t(a)[b=1]` is a shorthand the backend expands; scanned as
 * SQL it is nonsense, so its own refusal is reported instead.
 *
 * Offsets come out in **UTF-8 bytes**, which is the wire coordinate `EditorDiagnostic`
 * is defined in; the editor core maps them back.
 */

import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import type { Dialect } from '$lib/types/picus';
import { abbreviationLines, type AbbreviationLine } from './abbrev';
import { analyzeStatement, identOf } from './analysis';
import { longLineWarnings } from './long-line';
import { schemaViewFor, type SchemaView } from './schema-view';
import { scanSql, type SqlStatement } from './tokens';

/** Above this the analysis is skipped entirely. A 200 KB script is a generated
 *  dump, not something anyone is typing into, and a linear scan per keystroke on
 *  one is exactly the kind of thing that makes an editor feel heavy. */
const MAX_ANALYSED_CHARS = 200_000;

/** However wrong a buffer is, a hundred markers is already more than anyone reads. */
const MAX_DIAGNOSTICS = 100;

interface Marker {
  from: number;   // UTF-16 while collecting; converted to bytes at the end
  to: number;
  severity: EditorDiagnostic['severity'];
  message: string;
}

// ── Entry point ───────────────────────────────────────────────────────────────

/**
 * The one rule left on this side: a write on a read-only connection.
 *
 * The catalogue checks — unknown table, unknown column, ambiguous column — used to
 * live here too, reimplementing the server against a cached schema. They now come
 * from the database itself (`picus_validate`, driven by the validation store), which
 * is authoritative and never out of date. What stays is the check that needs no
 * catalogue and no round trip: the server refuses a write on a read-only session, and
 * saying so now saves the trip and the confusing error.
 */
function checkStatement(stmt: SqlStatement, dialect: Dialect, view: SchemaView, out: Marker[]) {
  const info = analyzeStatement(stmt, dialect);

  if (view.readOnly && info.isWrite && info.leading) {
    out.push({
      from: info.leading.from,
      to: info.leading.to,
      severity: 'error',
      message: `This connection is read-only — the server will refuse ${identOf(info.leading)}.`,
    });
  }
}

/**
 * Diagnostics for a buffer, in UTF-8 byte offsets.
 *
 * Pure with respect to the editor: a Svelte `$derived` calls it and hands the
 * result to `CodeEditor`'s `diagnostics` prop, so it re-runs when the text, the
 * connection or the schema changes — and when the backend's answer about an
 * abbreviation line lands, which is a reactive read for exactly that reason.
 */
export function sqlDiagnostics(
  text: string, dialect: Dialect, connectionId?: string,
): EditorDiagnostic[] {
  if (!text) return [];

  // Reported **before** the size guard below, and that ordering is the whole point:
  // a buffer big enough to have a line the highlighter gives up on is very often a
  // buffer big enough to be skipped here, so putting this after the guard would
  // silence the one warning whose case is precisely a huge buffer.
  const long = longLineWarnings(text).map((w) => ({
    from: w.from,
    to: w.to,
    severity: 'warning' as const,
    message: w.message,
  }));

  if (text.length > MAX_ANALYSED_CHARS) return toWire(text, long);

  // Which lines are abbreviations is the Rust parser's answer, cached — never a
  // shape test repeated here. Read before anything else because it decides both
  // what is skipped below and what is reported instead.
  const abbreviations = abbreviationLines(connectionId, text);
  const markers: Marker[] = refusals(abbreviations);

  const view = schemaViewFor(connectionId);
  // The only statement-level check left needs a read-only connection; without one
  // there is nothing to say here (the semantic checks now come from the server).
  if (view.readOnly) {
    const { statements } = scanSql(text, dialect);
    for (const stmt of statements) {
      if (markers.length >= MAX_DIAGNOSTICS) break;
      // The whole statement, not just the line: an abbreviation carries no `;`, so
      // the scanner runs it together with whatever follows. Losing a real
      // statement's markers for the second the shorthand is on screen is the
      // conservative direction — inventing markers for it is not.
      if (abbreviations.some((a) => a.from < stmt.to && stmt.from < a.to)) continue;
      checkStatement(stmt, dialect, view, markers);
    }
  }

  return toWire(text, [...long, ...markers]);
}

/**
 * Markers to the wire shape: UTF-8 byte offsets, capped.
 *
 * One helper and not two call sites, because the offset conversion is the part that
 * is silently wrong when it is wrong — a marker a few bytes off lands on the wrong
 * character, and only ever on the buffers that have accented text in them.
 *
 * It costs a linear pass (`makeU16ToByte` short-circuits on pure ASCII, which most
 * SQL is). That is paid on the oversized path too, where the rest of the analysis is
 * skipped — a deliberate trade: one pass to be able to say why the colour stopped is
 * worth more than the pass saves on a buffer nobody should be typing into anyway.
 */
function toWire(text: string, markers: Marker[]): EditorDiagnostic[] {
  if (markers.length === 0) return [];
  const u2b = makeU16ToByte(text);
  return markers.slice(0, MAX_DIAGNOSTICS).map((m) => ({
    from: u2b(m.from),
    to: u2b(m.to),
    severity: m.severity,
    message: m.message,
  }));
}

/**
 * The abbreviations that will not expand, as markers.
 *
 * Reported at `warning`, the same weight as an unknown table — the line is a
 * shorthand that has not resolved *yet*, and half of them are refusals for
 * half-typed names that resolve themselves on the next keystroke. The message is
 * the backend's own sentence, verbatim: refusing rather than guessing is the whole
 * posture of the language, and it only works if the reason reaches the person
 * typing.
 */
function refusals(abbreviations: AbbreviationLine[]): Marker[] {
  const out: Marker[] = [];
  for (const line of abbreviations) {
    if (!line.error) continue;
    out.push({ from: line.from, to: line.to, severity: 'warning', message: line.error });
  }
  return out;
}
