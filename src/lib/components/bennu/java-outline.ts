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
