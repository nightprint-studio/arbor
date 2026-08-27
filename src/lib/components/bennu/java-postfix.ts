/**
 * Java's postfix templates — the table the shared postfix engine is fed.
 *
 * Everything mechanical (finding the expression, matching what you typed, replacing it as one undo
 * step) lives in `shared/ui/code-editor/postfix.ts`. This is only *what the templates mean in Java*.
 *
 * ## The language level is part of the table
 *
 * A template is offered only if it compiles. `var` arrived in Java 10 and Bennu's projects are
 * routinely Java 8, so `.var` and `.for` come in two forms and the level picks one — a template that
 * emitted `var` on a Java 8 project would produce code that reads fine and doesn't build, which is
 * worse than not offering it. The pre-10 forms put a caret stop where the type goes, so the flow is
 * the same: expand, type the type, Tab into the body.
 *
 * ## Choosing the names
 *
 * They are IntelliJ's, deliberately and without improvement. Muscle memory is the entire value of a
 * postfix template; a better name nobody's fingers know is a worse name.
 */

import { CARET, type PostfixTemplate } from '$lib/components/shared/ui/code-editor/postfix';

/** The context a template is built for. */
export interface JavaPostfixContext {
  /** The project's Java language level — see the module doc. */
  level: number;
}

/** A block body: `{`, an indented caret line, `}` — the shape most of these end in. */
const block = (indent: string, unit: string, inner = `${CARET}`) =>
  `{\n${indent}${unit}${inner}\n${indent}}`;

/**
 * The Java postfix templates for a project at `level`.
 *
 * Order is relevance order: the engine boosts by position, so the ones you reach for hourly come
 * before the ones you reach for weekly.
 */
export function javaPostfixTemplates({ level }: JavaPostfixContext): PostfixTemplate[] {
  const hasVar = level >= 10;
  /** The declaration keyword, or a stop where the type goes on a project without `var`. */
  const decl = hasVar ? 'var' : `${CARET}Object`;

  const templates: PostfixTemplate[] = [
    {
      name: 'nn',
      detail: 'if (expr != null) { … }',
      expand: (e, i, u) => `if (${e} != null) ${block(i, u)}`,
    },
    {
      name: 'null',
      detail: 'if (expr == null) { … }',
      expand: (e, i, u) => `if (${e} == null) ${block(i, u)}`,
    },
    {
      name: 'if',
      detail: 'if (expr) { … }',
      expand: (e, i, u) => `if (${e}) ${block(i, u)}`,
    },
    {
      name: 'else',
      detail: 'if (!expr) { … }',
      expand: (e, i, u) => `if (!(${e})) ${block(i, u)}`,
    },
    {
      name: 'var',
      detail: 'declare a local holding expr',
      expand: (e) => `${decl} ${CARET}name = ${e};`,
    },
    {
      name: 'return',
      detail: 'return expr;',
      expand: (e) => `return ${e};`,
    },
    {
      name: 'sout',
      detail: 'System.out.println(expr);',
      expand: (e) => `System.out.println(${e});`,
    },
    {
      name: 'for',
      detail: 'for (each : expr) { … }',
      expand: (e, i, u) => `for (${decl} ${CARET}item : ${e}) ${block(i, u)}`,
    },
    {
      name: 'fori',
      detail: 'for (int i = 0; i < expr; i++) { … }',
      expand: (e, i, u) => `for (int ${CARET}i = 0; i < ${e}; i++) ${block(i, u)}`,
    },
    {
      name: 'forr',
      detail: 'for (int i = expr - 1; i >= 0; i--) { … }',
      expand: (e, i, u) => `for (int ${CARET}i = ${e} - 1; i >= 0; i--) ${block(i, u)}`,
    },
    {
      name: 'while',
      detail: 'while (expr) { … }',
      expand: (e, i, u) => `while (${e}) ${block(i, u)}`,
    },
    {
      name: 'not',
      detail: '!expr',
      expand: (e) => `!(${e})`,
    },
    {
      name: 'par',
      detail: '(expr)',
      expand: (e) => `(${e})`,
    },
    {
      name: 'try',
      detail: 'try { expr; } catch (Exception e) { … }',
      expand: (e, i, u) =>
        `try {\n${i}${u}${e};\n${i}} catch (Exception ${CARET}ex) {\n${i}${u}${CARET}\n${i}}`,
    },
    {
      name: 'throw',
      detail: 'throw expr;',
      expand: (e) => `throw ${e};`,
    },
    {
      name: 'switch',
      detail: 'switch (expr) { … }',
      expand: (e, i, u) => `switch (${e}) ${block(i, u)}`,
    },
    {
      name: 'cast',
      detail: '((Type) expr)',
      expand: (e) => `((${CARET}Object) ${e})`,
    },
    {
      name: 'instanceof',
      detail: 'expr instanceof Type',
      expand: (e) => `${e} instanceof ${CARET}Object`,
    },
    {
      name: 'opt',
      detail: 'Optional.ofNullable(expr)',
      expand: (e) => `Optional.ofNullable(${e})`,
    },
    {
      name: 'assert',
      detail: 'assert expr;',
      expand: (e) => `assert ${e};`,
    },
    {
      name: 'synchronized',
      detail: 'synchronized (expr) { … }',
      expand: (e, i, u) => `synchronized (${e}) ${block(i, u)}`,
    },
    {
      name: 'serr',
      detail: 'System.err.println(expr);',
      expand: (e) => `System.err.println(${e});`,
    },
  ];

  // `.forEach` and `.stream` need Java 8. Nothing Bennu opens is below it in practice, but the
  // table is the place that decides what compiles, so it decides here too.
  if (level >= 8) {
    templates.push(
      {
        name: 'forEach',
        detail: 'expr.forEach(item -> { … })',
        expand: (e, i, u) => `${e}.forEach(${CARET}item -> ${block(i, u)});`,
      },
      {
        name: 'stream',
        detail: 'expr.stream()',
        expand: (e) => `${e}.stream()`,
      },
    );
  }
  return templates;
}
