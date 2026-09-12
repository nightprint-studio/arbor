/**
 * The vocabulary of a Jinja code template, what a template file is called, and the names its language answers
 * to.
 *
 * Two things colour a template: the editor's stream parser (`bennu/jinja-lang.ts`), for a template
 * being edited, and a Prism grammar (`prism-languages/jinja.ts`), for one shown anywhere else — a
 * page of the docs, a fenced block in a Markdown preview. They are separate tokenizers for the same
 * reason every language here has two, but the words are one list: a tag the editor knows and the
 * docs paint as a plain name is the two disagreeing about the language.
 */

/** The statement names `{%` can open with. */
export const JINJA_TAGS = [
  'if', 'elif', 'else', 'endif', 'for', 'endfor', 'set', 'endset', 'macro', 'endmacro', 'call',
  'endcall', 'filter', 'endfilter', 'with', 'endwith', 'block', 'endblock', 'extends', 'include',
  'import', 'from', 'raw', 'endraw', 'autoescape', 'endautoescape', 'break', 'continue', 'do',
] as const;

/** Words that are operators inside an expression. */
export const JINJA_WORD_OPERATORS = [
  'and', 'or', 'not', 'in', 'is', 'if', 'else', 'as', 'recursive', 'with', 'without', 'context', 'ignore', 'missing',
] as const;

export const JINJA_CONSTANTS = ['true', 'false', 'none', 'True', 'False', 'None'] as const;

/** A Jinja template's file name: `*.jinja`, `*.jinja2`, `*.j2`. */
export const JINJA_FILE = /\.(jinja2?|j2)$/i;

export function isJinjaFile(name: string): boolean {
  return JINJA_FILE.test(name);
}

/** The language a template generates, from its name — `java` for `OrderTest.java.jinja` — or `''` when the
 *  name does not say. */
export function jinjaInnerLanguage(fileName: string): string {
  const parts = fileName.toLowerCase().split('.');
  const extension = parts.length >= 3 ? parts[parts.length - 2] : '';
  return JINJA_INNER_BY_EXTENSION[extension] ?? '';
}

/** The Prism grammar for a template file: `jinja-java` for `OrderTest.java.jinja`, `jinja` when the name does
 *  not say what it writes — the canonical names of `jinjaFences`. */
export function jinjaPrismLanguage(fileName: string): string {
  const inner = jinjaInnerLanguage(fileName);
  return inner ? `jinja-${inner}` : 'jinja';
}

/**
 * The languages a template can generate, by the extension before `.jinja`.
 *
 * The extension is not decoration: it decides how the template is coloured, and — for an
 * abbreviation — which files it is offered in. A language belongs here once both tokenizers can
 * answer for it, so what is listed is exactly what a template can be written in.
 */
export const JINJA_INNER_BY_EXTENSION: Record<string, string> = {
  java: 'java', kt: 'kotlin', kts: 'kotlin', xml: 'xml', html: 'html', htm: 'html', yaml: 'yaml',
  yml: 'yaml', properties: 'properties', toml: 'toml', py: 'python', js: 'javascript', ts: 'javascript',
  rs: 'rust', sql: 'sql', sh: 'shell', bash: 'shell', go: 'go', lua: 'lua', css: 'css', scss: 'css',
  json: 'json',
  // A JSP is markup with scriptlets in it; the markup half is what a template writes and what can
  // be coloured, so it reads as HTML rather than as nothing.
  jsp: 'html', jspf: 'html', tag: 'html',
};

/** What a language is called where a person reads it — the picker in *New template…*. */
export const JINJA_LANGUAGE_LABELS: Record<string, string> = {
  java: 'Java', kotlin: 'Kotlin', xml: 'XML', html: 'HTML / JSP', yaml: 'YAML',
  properties: 'Properties', toml: 'TOML', python: 'Python', javascript: 'JavaScript / TypeScript',
  rust: 'Rust', sql: 'SQL', shell: 'Shell', go: 'Go', lua: 'Lua', css: 'CSS', json: 'JSON',
};

/** One language a template can be written in: the extension it is named with, and its label. */
export interface JinjaLanguage {
  extension: string;
  label: string;
}

/**
 * The languages offered when a template is created, each with the extension that names its file —
 * the first extension mapping to that language, so `rust` is written `.rs.jinja` and not `.rust`.
 *
 * Java leads because most templates are Java's; the rest follow alphabetically. The empty extension
 * is last and is a real choice: a template that writes plain text — a licence header, a banner — has
 * no language, and an abbreviation with none is offered in every file.
 */
export function jinjaLanguages(): JinjaLanguage[] {
  const first = new Map<string, string>();
  for (const [extension, language] of Object.entries(JINJA_INNER_BY_EXTENSION)) {
    if (!first.has(language)) first.set(language, extension);
  }
  const entries = [...first.entries()]
    .map(([language, extension]) => ({ extension, label: JINJA_LANGUAGE_LABELS[language] ?? language }))
    .sort((a, b) => (a.extension === 'java' ? -1 : b.extension === 'java' ? 1 : a.label.localeCompare(b.label)));
  return [...entries, { extension: '', label: 'Plain text — no language' }];
}

export interface JinjaFence {
  /** The generated language — a value of the table above, or `''` for a template on its own. */
  inner: string;
  /** The canonical name first, then its aliases. */
  names: string[];
}

/**
 * Jinja as the language of a fenced code block: ```` ```jinja ```` for a template on its own,
 * ```` ```java.jinja ```` (or `java+jinja`, `jinja-java`) for one that writes Java — named the way the
 * file would be, so a snippet pasted from a template reads as the template does in its own tab.
 */
export function jinjaFences(): JinjaFence[] {
  const inners = [...new Set(Object.values(JINJA_INNER_BY_EXTENSION))];
  const generating = inners.map((inner) => {
    const extensions = Object.entries(JINJA_INNER_BY_EXTENSION)
      .filter(([, language]) => language === inner)
      .map(([extension]) => extension);
    const aliases = extensions.flatMap((ext) => [`${ext}.jinja`, `${ext}+jinja`, `jinja-${ext}`]);
    return { inner, names: [...new Set([`jinja-${inner}`, ...aliases])] };
  });
  return [{ inner: '', names: ['jinja', 'jinja2', 'j2'] }, ...generating];
}
