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

const TYPE_RE = /^\s*(?:public\s+|private\s+|protected\s+|abstract\s+|final\s+|static\s+|sealed\s+|non-sealed\s+)*(class|interface|enum)\s+([A-Za-z_]\w*)/;
// A method: modifiers, optional generics/return type, name, '(' … ')' then '{' or ';'.
const METHOD_RE = /^\s*(?:public\s+|private\s+|protected\s+|abstract\s+|final\s+|static\s+|synchronized\s+|native\s+|default\s+)+(?:<[^>]+>\s*)?([\w.<>\[\],\s]+?\s+)?([A-Za-z_]\w*)\s*\([^;{]*\)\s*(?:throws [\w.,\s]+)?\s*[{;]/;
// A field: modifiers, type, name, then '=' or ';' (no parens → not a method).
const FIELD_RE = /^\s*(?:public\s+|private\s+|protected\s+|static\s+|final\s+|transient\s+|volatile\s+)+([\w.<>\[\],\s]+?)\s+([A-Za-z_]\w*)\s*[=;]/;

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
