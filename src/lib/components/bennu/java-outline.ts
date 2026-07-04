/**
 * Lightweight Java outline — a regex-based symbol extractor for the Structure /
 * Outline panels. Deliberately NOT tree-sitter based: the shared code-editor owns
 * the parser for highlighting, and re-parsing here would duplicate that work; a
 * cheap line scan is plenty for a scannable symbol list and jump-to-line.
 *
 * When the language service (`bennu_completion` / a real symbol index) lands, this
 * is the seam to replace — the shapes (`JavaSymbol[]`, `JavaNode[]`) can stay and
 * just be fed from the backend instead.
 *
 * Two views over the same scan:
 *   • `javaOutline(source): JavaSymbol[]` — the flat, ordered list (kept for the
 *     accessor detection / Generate consumers that don't want a tree).
 *   • `javaStructure(source): JavaNode[]` — a HIERARCHY: each top-level type is a
 *     root whose children are the `fields` / `methods` group buckets and its NESTED
 *     types (recursive). This is what the Structure/Outline tree renders.
 *
 * A shared brace-depth line scan produces the raw declaration list once; both views
 * are projections of it, so a fix to the regexes fixes both.
 */

/** Member visibility, parsed from the leading modifiers on the declaration line. */
export type JavaVisibility = 'public' | 'protected' | 'private' | 'package';

export type JavaSymbolKind = 'class' | 'interface' | 'enum' | 'record' | 'method' | 'field';

export interface JavaSymbol {
  kind: JavaSymbolKind;
  /** Display name (identifier). */
  name: string;
  /** Optional signature detail (e.g. return type / params) shown muted. */
  detail?: string;
  /** 1-based line number of the declaration. */
  line: number;
  /** Access level parsed from the leading modifiers (defaults to package). */
  visibility: JavaVisibility;
  /** A method carrying an `@Override` annotation (overrides / implements a supertype
   *  member). Only ever set on `method` symbols. */
  overrides?: boolean;
}

/** Node kinds in the hierarchical structure view. `group` is a synthetic bucket
 *  ("Fields" / "Methods") that owns a type's members; the rest map to declarations. */
export type JavaNodeKind = JavaSymbolKind | 'group';

export interface JavaNode {
  /** Stable id for the Tree widget (kind + name + line, unique per file). */
  id: string;
  kind: JavaNodeKind;
  name: string;
  detail?: string;
  /** 1-based line of the declaration. Group buckets reuse their owner's line so a
   *  click still lands somewhere sensible; the panel only jumps on real members. */
  line: number;
  visibility?: JavaVisibility;
  /** A method annotated `@Override` (mirrors {@link JavaSymbol.overrides}). */
  overrides?: boolean;
  children?: JavaNode[];
}

// A single type token: dotted name + optional `<…>` generics + optional `[]` arrays.
// No top-level whitespace inside it (generic-internal spaces live inside the `<…>`), so
// the patterns below never nest a whitespace-matching quantifier against a trailing
// `\s+` — the ambiguity that made the old expressions backtrack catastrophically on
// long non-Java lines (e.g. a JSP scriptlet), which froze the UI when Generate/Outline
// ran on a `.jsp`.
const TYPE_TOK = String.raw`[\w.$]+(?:<[^>]*>)?(?:\[\])*`;
const TYPE_RE = /^\s*(?:(?:public|private|protected|abstract|final|static|sealed|non-sealed)\s+)*(class|interface|enum|record)\s+([A-Za-z_]\w*)/;
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

// A leading run of annotations on a declaration line (`@Override`, `@SuppressWarnings("x")`,
// `@com.foo.Bar(1)`). Peeled off before the type/method/field regexes so an inline
// `@Override public void f()` is still recognised, and so the override marker is detected
// uniformly whether the annotation is inline or on its own line. Single-line arg matching
// only (`\([^)]*\)`) — a rare multi-line / nested-paren annotation just yields a partial
// strip and the decl regex declines that line, exactly as an unparseable line does today.
const ANNO_LEAD_RE = /^\s*(?:@[\w.]+(?:\s*\([^)]*\))?\s*)+/;
const OVERRIDE_ANNO_RE = /@Override\b/;

/** Parse the access level from the head of a (short) declaration line. Java's default
 *  (no modifier) is package-private, so absence maps to `package`. */
function visibilityOf(line: string): JavaVisibility {
  if (/\bpublic\b/.test(line)) return 'public';
  if (/\bprotected\b/.test(line)) return 'protected';
  if (/\bprivate\b/.test(line)) return 'private';
  return 'package';
}

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

/** One raw declaration the scan found, carrying the brace depth it was declared at so
 *  a later pass can rebuild the parent/child nesting. */
interface RawDecl {
  symbol: JavaSymbol;
  /** Brace nesting depth *before* this line's own braces — a top-level type is 0,
   *  its members are 1, a member of a nested type is 2, … */
  depth: number;
  isType: boolean;
}

/**
 * Single shared scan: walk the source once, tracking brace depth, and emit every
 * declaration we recognise with the depth it sits at. Both `javaOutline` (flat) and
 * `javaStructure` (tree) are built from this list, so the recognition rules — and
 * their perf caps — live in exactly one place.
 *
 * Brace depth is counted from the running `{`/`}` balance, ignoring braces inside
 * `"…"` / `'…'` / `//` line comments (a cheap char scan, no full lexer). Block
 * comments are handled coarsely by the existing `*` / `/*` line skips; a stray brace
 * inside a `/* … *\/` block is rare in declaration-adjacent code and at worst mis-nests
 * a symbol by one level — acceptable for an outline.
 */
function scanDeclarations(source: string): RawDecl[] {
  const out: RawDecl[] = [];
  const lines = source.split(/\r?\n/);
  let depth = 0;
  // Whether an `@Override` seen on a preceding annotation / blank / comment line is still
  // waiting to be attached to the method it annotates. Reset the moment a real declaration
  // or statement is consumed. Annotated to break the CFA inference cycle (it's reassigned
  // from an expression that itself reads this variable).
  let pendingOverride: boolean = false;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    // Depth to attribute to a declaration on this line is the depth *before* the
    // line's own opening brace, so a `class X {` on one line sits at the current
    // depth while its members (next lines) sit one deeper.
    const lineDepth = depth;

    if (raw.length <= MAX_DECL_LINE) {
      const line = raw.replace(/\/\/.*$/, '');
      const trimmedFull = line.trim();

      // Peel leading annotations, remembering whether @Override was among them.
      const annoLead = ANNO_LEAD_RE.exec(line);
      const hasOverrideHere = !!annoLead && OVERRIDE_ANNO_RE.test(annoLead[0]);
      const body = annoLead ? line.slice(annoLead[0].length) : line;
      const trimmed = body.trim();
      const overridePending: boolean = pendingOverride || hasOverrideHere;

      const skip =
        !trimmed || trimmed.startsWith('*') || trimmed.startsWith('/*') || trimmed.startsWith('//') ||
        trimmed.startsWith('import ') || trimmed.startsWith('package ');

      if (skip) {
        // Carry the override flag across annotation-only / blank / comment lines between
        // `@Override` and the method it annotates; a `package`/`import` line breaks the run.
        const isCarrier =
          !trimmedFull || trimmedFull.startsWith('@') ||
          trimmedFull.startsWith('*') || trimmedFull.startsWith('/*') || trimmedFull.startsWith('//');
        pendingOverride = overridePending && isCarrier;
      } else {
        const t = TYPE_RE.exec(body);
        if (t) {
          out.push({
            symbol: { kind: t[1] as JavaSymbolKind, name: t[2], line: i + 1, visibility: visibilityOf(body) },
            depth: lineDepth,
            isType: true,
          });
          pendingOverride = false;
        } else if (!/^\s*(if|for|while|switch|catch|synchronized|return|new)\b/.test(trimmed)) {
          const m = METHOD_RE.exec(body);
          if (m) {
            const ret = (m[1] ?? '').trim();
            out.push({
              symbol: {
                kind: 'method', name: m[2], detail: ret || undefined, line: i + 1,
                visibility: visibilityOf(body), overrides: overridePending || undefined,
              },
              depth: lineDepth,
              isType: false,
            });
          } else {
            const f = FIELD_RE.exec(body);
            if (f) {
              out.push({
                symbol: { kind: 'field', name: f[2], detail: f[1].trim(), line: i + 1, visibility: visibilityOf(body) },
                depth: lineDepth,
                isType: false,
              });
            }
          }
          pendingOverride = false; // a method/field/other statement consumed the flag
        } else {
          pendingOverride = false; // a control-flow statement
        }
      }
    }

    depth += braceDelta(raw);
    if (depth < 0) depth = 0; // never underflow on unbalanced input
  }
  return out;
}

/** Net `{` minus `}` on a line, skipping braces inside string/char literals and
 *  after a `//` comment. Linear single pass — no regex, no backtracking. */
function braceDelta(line: string): number {
  let delta = 0;
  let inStr: '"' | "'" | null = null;
  for (let j = 0; j < line.length; j++) {
    const c = line[j];
    if (inStr) {
      if (c === '\\') { j++; continue; } // skip escaped char
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'") { inStr = c; continue; }
    if (c === '/' && line[j + 1] === '/') break; // rest of line is a comment
    if (c === '{') delta++;
    else if (c === '}') delta--;
  }
  return delta;
}

/** Extract a flat, ordered symbol list from Java source. Comment/blank lines and
 *  obvious control-flow lines are skipped so the list stays declaration-only. */
export function javaOutline(source: string): JavaSymbol[] {
  return scanDeclarations(source).map((d) => d.symbol);
}

/**
 * The real CLASS FIELDS only — fields declared directly inside a type body (an
 * enclosing type is open at `depth-1`), NOT `final` locals inside method bodies that
 * `FIELD_RE` also matches. Deduped by name (a class can't have two fields with the same
 * name; dedup also guards a keyed `{#each}` against a crash when a local shadows a
 * field). This is what the Generate modal wants — the accessor targets — as opposed to
 * the flat `javaOutline` which lists every declaration including locals.
 */
export function javaClassFields(source: string): JavaSymbol[] {
  const decls = scanDeclarations(source);
  const openAtDepth: boolean[] = []; // openAtDepth[d] = a type is open at depth d
  const out: JavaSymbol[] = [];
  const seen = new Set<string>();
  for (const d of decls) {
    if (d.isType) {
      openAtDepth[d.depth] = true;
      openAtDepth.length = d.depth + 1;
    } else if (d.symbol.kind === 'field' && d.depth > 0 && openAtDepth[d.depth - 1]) {
      if (seen.has(d.symbol.name)) continue;
      seen.add(d.symbol.name);
      out.push(d.symbol);
    }
  }
  return out;
}

/** Group buckets, in the order the Structure tree presents them under a type. */
const GROUP_ORDER = { field: 'Fields', method: 'Methods' } as const;

/**
 * Build the HIERARCHICAL structure: top-level types as roots, each with `Fields` /
 * `Methods` group buckets and its NESTED types (recursive). Members and nested types
 * are attached to the closest enclosing type by brace depth.
 *
 * A stack of "currently open" types keyed by depth lets us route each declaration to
 * its owner without a real parser: a declaration at depth D belongs to the type most
 * recently opened at depth D-1. Declarations with no enclosing type (rare — top-level
 * fields/methods only occur in malformed source) are dropped from the tree; the flat
 * `javaOutline` still surfaces them for any consumer that wants everything.
 */
export function javaStructure(source: string): JavaNode[] {
  const decls = scanDeclarations(source);
  const roots: JavaNode[] = [];
  // openTypes[d] = the type node opened at depth d whose members live at depth d+1.
  const openTypes: (JavaNode | undefined)[] = [];

  const nodeId = (s: JavaSymbol) => `${s.kind}:${s.name}:${s.line}`;

  /** Lazily create + return the `Fields`/`Methods` bucket under a type node. */
  function bucket(type: JavaNode, key: 'field' | 'method'): JavaNode {
    const label = GROUP_ORDER[key];
    type.children ??= [];
    let g = type.children.find((c) => c.kind === 'group' && c.name === label);
    if (!g) {
      g = { id: `group:${key}:${type.id}`, kind: 'group', name: label, line: type.line, children: [] };
      type.children.push(g);
    }
    return g;
  }

  for (const d of decls) {
    const s = d.symbol;
    const node: JavaNode = {
      id: nodeId(s),
      kind: s.kind,
      name: s.name,
      detail: s.detail,
      line: s.line,
      visibility: s.visibility,
      overrides: s.overrides,
    };

    const parent = d.depth > 0 ? openTypes[d.depth - 1] : undefined;

    if (d.isType) {
      if (parent) {
        // Nested type — sits directly under its enclosing type (after the buckets,
        // but rendered by the panel in declaration order regardless).
        (parent.children ??= []).push(node);
      } else if (d.depth === 0) {
        roots.push(node);
      }
      // Register as the open type for its depth, and clear any deeper entries so a
      // sibling type opened later doesn't inherit a stale nested owner.
      openTypes[d.depth] = node;
      openTypes.length = d.depth + 1;
    } else if (parent) {
      // A field / method → its group bucket under the enclosing type.
      const g = bucket(parent, s.kind === 'field' ? 'field' : 'method');
      (g.children ??= []).push(node);
    }
    // Members with no enclosing type are intentionally dropped from the tree.
  }

  return roots;
}
