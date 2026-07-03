/**
 * Lightweight Java outline — a regex-based symbol extractor for the Structure /
 * Outline panels. Deliberately NOT tree-sitter based: the shared code-editor owns
 * the parser for highlighting, and re-parsing here would duplicate that work; a
 * cheap line scan is plenty for a scannable symbol list and jump-to-line.
 *
 * When the language service (`bennu_completion` / a real symbol index) lands, this
 * is the seam to replace — the shape (`JavaSymbol[]`) can stay and just be fed from
 * the backend instead.
 */

export type JavaSymbolKind = 'class' | 'interface' | 'enum' | 'method' | 'field';

export interface JavaSymbol {
  kind: JavaSymbolKind;
  /** Display name (identifier). */
  name: string;
  /** Optional signature detail (e.g. return type / params) shown muted. */
  detail?: string;
  /** 1-based line number of the declaration. */
  line: number;
}

// A single type token: dotted name + optional `<…>` generics + optional `[]` arrays.
// No top-level whitespace inside it (generic-internal spaces live inside the `<…>`), so
// the patterns below never nest a whitespace-matching quantifier against a trailing
// `\s+` — the ambiguity that made the old expressions backtrack catastrophically on
// long non-Java lines (e.g. a JSP scriptlet), which froze the UI when Generate/Outline
// ran on a `.jsp`.
const TYPE_TOK = String.raw`[\w.$]+(?:<[^>]*>)?(?:\[\])*`;
const TYPE_RE = /^\s*(?:(?:public|private|protected|abstract|final|static|sealed|non-sealed)\s+)*(class|interface|enum)\s+([A-Za-z_]\w*)/;
// A method: modifiers, optional generics, optional return type (one token), name,
// '(' … ')' then '{' or ';'.
const METHOD_RE = new RegExp(
  String.raw`^\s*(?:(?:public|private|protected|abstract|final|static|synchronized|native|default)\s+)+` +
  String.raw`(?:<[^>]*>\s*)?(?:(${TYPE_TOK})\s+)?([A-Za-z_$]\w*)\s*\([^;{]*\)\s*(?:throws [\w.,\s]+)?\s*[{;]`,
);
// A field: modifiers, type (one token), name, then '=' or ';' (no parens → not a method).
const FIELD_RE = new RegExp(
  String.raw`^\s*(?:(?:public|private|protected|static|final|transient|volatile)\s+)+` +
  String.raw`(${TYPE_TOK})\s+([A-Za-z_$]\w*)\s*[=;]`,
);

/** Declaration lines are short; a very long line is a body / minified / JSP line that
 *  can't hold a single declaration we care about. Skipping it is both a perf win and a
 *  hard backstop against pathological regex input. */
const MAX_DECL_LINE = 400;

/**
 * Names of fields declared `final` or annotated `@NonNull` / `@NotNull` / `@Nonnull`
 * — the "required" fields a required-args constructor should take. Best-effort
 * line scan: matches a `final` modifier on the declaration line, or a nullability
 * annotation immediately preceding it (same line, or the line above). Pure helper,
 * unit-testable; the reliable source is the BE symbol model when it lands.
 */
const NONNULL_RE = /@(?:NonNull|NotNull|Nonnull)\b/;
export function requiredFieldNames(source: string): Set<string> {
  const req = new Set<string>();
  const lines = source.split(/\r?\n/);
  let pendingAnnotation = false; // a nullability annotation seen on its own line
  for (const raw of lines) {
    if (raw.length > MAX_DECL_LINE) { continue; }
    const line = raw.replace(/\/\/.*$/, '');
    const trimmed = line.trim();
    if (!trimmed) { continue; }

    const annotatedInline = NONNULL_RE.test(line);
    const f = FIELD_RE.exec(line);
    if (f) {
      const head = line.slice(0, f.index + f[0].length);
      // FIELD_RE only matches modifier-led declarations; `final` (if present) is
      // among those leading modifiers, before the type/name.
      const isFinal = /\bfinal\b/.test(head);
      // static fields are class constants, never constructor params; and a field
      // with an inline initializer is already assigned → not a required arg.
      const isStatic = /\bstatic\b/.test(head);
      const hasInitializer = /=/.test(line);
      if (!isStatic && !hasInitializer && (isFinal || annotatedInline || pendingAnnotation)) {
        req.add(f[2]);
      }
      pendingAnnotation = false;
      continue;
    }
    // A bare `@NonNull` line preceding a field declaration.
    pendingAnnotation = annotatedInline && /^@\w/.test(trimmed);
  }
  return req;
}

/** Extract a flat, ordered symbol list from Java source. Comment/blank lines and
 *  obvious control-flow lines are skipped so the list stays declaration-only. */
export function javaOutline(source: string): JavaSymbol[] {
  const out: JavaSymbol[] = [];
  const lines = source.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    if (raw.length > MAX_DECL_LINE) continue;
    const line = raw.replace(/\/\/.*$/, '');
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('*') || trimmed.startsWith('/*') || trimmed.startsWith('//')) continue;
    if (trimmed.startsWith('@') || trimmed.startsWith('import ') || trimmed.startsWith('package ')) continue;

    const t = TYPE_RE.exec(line);
    if (t) {
      out.push({ kind: t[1] as JavaSymbolKind, name: t[2], line: i + 1 });
      continue;
    }
    // Guard method match against control-flow keywords that also end in "(...) {".
    if (!/^\s*(if|for|while|switch|catch|synchronized|return|new)\b/.test(trimmed)) {
      const m = METHOD_RE.exec(line);
      if (m) {
        const ret = (m[1] ?? '').trim();
        out.push({ kind: 'method', name: m[2], detail: ret || undefined, line: i + 1 });
        continue;
      }
    }
    const f = FIELD_RE.exec(line);
    if (f) {
      out.push({ kind: 'field', name: f[2], detail: f[1].trim(), line: i + 1 });
    }
  }
  return out;
}
