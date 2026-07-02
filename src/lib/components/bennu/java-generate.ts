/**
 * java-generate — pure, deterministic Java-member string builders for the
 * "Generate" flow (constructor / getters / setters / withers). No Svelte, no IPC,
 * no DOM: given a class name, a list of fields, and a set of options, it returns
 * the exact source text to insert at the editor caret.
 *
 * Kept a small pure helper on purpose so the generation is unit-testable and the
 * modal stays a thin shell over it (CLAUDE.md · centralize / small files). Each
 * builder is independently testable (see the `describe` blocks a future
 * `*.test.ts` would cover: classic vs fluent getter naming, boolean `is` prefix,
 * fluent setter `return this`, wither `withX`, snake_case accessor naming,
 * constructor variants all/none/required/selected, `final` params, brace/member
 * spacing).
 *
 * SEAM — the real field list will come from the backend symbol model
 * (`bennu_symbols` / the language service) once it lands; today the caller feeds
 * `JavaField[]` derived from the regex outline (`java-outline.ts`) or a mock. The
 * shape here (`JavaField`, `GenerateOptions`) is what that BE model must map to.
 */

/** What set of members to generate. `with` = builder-style withers. */
export type GenerateMode =
  | 'constructor'
  | 'getters'
  | 'setters'
  | 'getters-setters'
  | 'with';
/** Naming convention for the generated accessor identifiers. */
export type NamingStyle = 'camelCase' | 'snake_case';
/**
 * Which fields a constructor takes:
 *   • all       — every field (all-args).
 *   • none      — no fields (no-args / default ctor).
 *   • required  — only `final` / `@NonNull`-annotated fields.
 *   • selected  — the caller's manual checklist.
 */
export type ConstructorVariant = 'all' | 'none' | 'required' | 'selected';

/** A class field the generator can build members for. `type` is the Java type as
 *  written (`String`, `int`, `List<String>`), `name` the identifier. `required`
 *  marks a field the required-args constructor must include (final / @NonNull);
 *  it's optional — callers that can't detect it leave it undefined. */
export interface JavaField {
  name: string;
  type: string;
  required?: boolean;
}

/**
 * Formatting style flags — mirror the "Java Style" settings section. All optional
 * so callers (and tests) can pass a partial; the builders default each to the
 * conventional IDE choice.
 */
export interface JavaStyle {
  /** Generated constructor/setter/with params (and locals) are declared `final`. */
  finalParams?: boolean;
  /** A space inside braces on single-line bodies (unused by the block builders
   *  today; reserved for a future compact-body mode). Kept for shape parity with
   *  the settings section. */
  spaceInBraces?: boolean;
  /** Blank line between generated members (true → one blank line, as most IDEs). */
  blankLineBetweenMembers?: boolean;
}

export interface GenerateOptions {
  /** Enclosing class name — used as the constructor name and fluent/with return type. */
  className: string;
  /** Fluent accessors: getters are record-style (`email()`), setters share the
   *  getter name and `return this` (typed as the class) instead of `void`. */
  fluent: boolean;
  /** Naming convention applied to the generated accessor names. */
  naming: NamingStyle;
  /** Indent unit for the generated body (defaults to 4 spaces). */
  indent?: string;
  /** Formatting style flags (final params, spacing). */
  style?: JavaStyle;
}

const DEFAULT_INDENT = '    ';

// ── Naming helpers ────────────────────────────────────────────────────────────

/** Uppercase the first character (for `getFoo` / `setFoo` accessor stems). */
function upperFirst(s: string): string {
  return s.length ? s[0].toUpperCase() + s.slice(1) : s;
}

/** Convert a camelCase / PascalCase identifier to snake_case. */
export function toSnakeCase(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2')
    .toLowerCase();
}

/** Apply the accessor stem casing implied by `naming` to a field name. In
 *  camelCase the stem is `UpperFirst(name)` (→ `getFooBar`); in snake_case the
 *  stem is the snake form with the leading segment upper-cased (→ `get_foo_bar`). */
function accessorStem(name: string, naming: NamingStyle): string {
  if (naming === 'snake_case') {
    const snake = toSnakeCase(name);
    return snake ? '_' + snake : snake;
  }
  return upperFirst(name);
}

/** The fluent/record-style accessor identifier for a field (no get/set prefix,
 *  just the field name, snake-cased when requested). */
function fluentName(name: string, naming: NamingStyle): string {
  return naming === 'snake_case' ? toSnakeCase(name) : name;
}

/** A getter reads a boolean with `is` rather than `get` (JavaBeans convention). */
function isBooleanType(type: string): boolean {
  return type === 'boolean' || type === 'Boolean';
}

/** Render a single param declaration, honouring `final`. */
function param(field: JavaField, style: JavaStyle | undefined): string {
  const fin = style?.finalParams ? 'final ' : '';
  return `${fin}${field.type} ${field.name}`;
}

// ── Member builders ───────────────────────────────────────────────────────────

/** Build a single getter.
 *  Classic: `public String getEmail() { return email; }` (boolean → `isEmail`).
 *  Fluent : `public String email() { return email; }` (record-style, no prefix). */
export function buildGetter(field: JavaField, opts: GenerateOptions): string {
  const i = opts.indent ?? DEFAULT_INDENT;
  let method: string;
  if (opts.fluent) {
    method = fluentName(field.name, opts.naming);
  } else {
    const stem = accessorStem(field.name, opts.naming);
    method = `${isBooleanType(field.type) ? 'is' : 'get'}${stem}`;
  }
  return (
    `${i}public ${field.type} ${method}() {\n` +
    `${i}${i}return ${field.name};\n` +
    `${i}}`
  );
}

/** Build a single setter.
 *  Classic: `public void setEmail(String email) { this.email = email; }`.
 *  Fluent : `public Foo email(String email) { this.email = email; return this; }`
 *  (same identifier as the fluent getter, returns the class). */
export function buildSetter(field: JavaField, opts: GenerateOptions): string {
  const i = opts.indent ?? DEFAULT_INDENT;
  const method = opts.fluent
    ? fluentName(field.name, opts.naming)
    : `set${accessorStem(field.name, opts.naming)}`;
  const retType = opts.fluent ? opts.className : 'void';
  const body =
    `${i}${i}this.${field.name} = ${field.name};\n` +
    (opts.fluent ? `${i}${i}return this;\n` : '');
  return (
    `${i}public ${retType} ${method}(${param(field, opts.style)}) {\n` +
    body +
    `${i}}`
  );
}

/** Build a single wither (builder-style, returns the class):
 *  `public Foo withEmail(String email) { this.email = email; return this; }`. */
export function buildWith(field: JavaField, opts: GenerateOptions): string {
  const i = opts.indent ?? DEFAULT_INDENT;
  const method = `with${accessorStem(field.name, opts.naming)}`;
  return (
    `${i}public ${opts.className} ${method}(${param(field, opts.style)}) {\n` +
    `${i}${i}this.${field.name} = ${field.name};\n` +
    `${i}${i}return this;\n` +
    `${i}}`
  );
}

/** Build a constructor over `fields`. The caller decides which fields to pass
 *  (all / none / required / selected); this just assembles them. An empty list
 *  yields a no-args constructor. */
export function buildConstructor(fields: JavaField[], opts: GenerateOptions): string {
  const i = opts.indent ?? DEFAULT_INDENT;
  const params = fields.map((f) => param(f, opts.style)).join(', ');
  const assigns = fields
    .map((f) => `${i}${i}this.${f.name} = ${f.name};`)
    .join('\n');
  return (
    `${i}public ${opts.className}(${params}) {\n` +
    (assigns ? assigns + '\n' : '') +
    `${i}}`
  );
}

/**
 * Assemble the full text to insert for `mode` over `fields`. Members are joined
 * with a blank line (or a single newline when `style.blankLineBetweenMembers` is
 * false), matching typical IDE output. The constructor (when the mode includes
 * it) leads. Returns '' when there's nothing to generate.
 */
export function generateMembers(
  mode: GenerateMode,
  fields: JavaField[],
  opts: GenerateOptions,
): string {
  const blocks: string[] = [];

  if (mode === 'constructor') {
    blocks.push(buildConstructor(fields, opts));
  }
  if (mode === 'getters' || mode === 'getters-setters') {
    for (const f of fields) blocks.push(buildGetter(f, opts));
  }
  if (mode === 'setters' || mode === 'getters-setters') {
    for (const f of fields) blocks.push(buildSetter(f, opts));
  }
  if (mode === 'with') {
    for (const f of fields) blocks.push(buildWith(f, opts));
  }

  const sep = opts.style?.blankLineBetweenMembers === false ? '\n' : '\n\n';
  return blocks.join(sep);
}
