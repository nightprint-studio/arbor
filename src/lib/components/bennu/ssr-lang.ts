/**
 * The editor language of a **structural query** — highlighting and completion.
 *
 * A query is Java with holes and three clause words in it, so neither a Java mode nor a plain
 * text field is right: Java would paint `$a$` as broken code, and plain text would leave the one
 * thing that needs distinguishing — a hole from the code around it — looking like the code
 * around it.
 *
 * ## What it colours, and why only this much
 *
 * The **clause keywords** at the start of a line, a **placeholder**, the **constraint** inside
 * one — with its `@type` / `&` / `!` operators told apart from the names they act on — and a
 * comment. Everything else is left plain on purpose. A query is mostly a fragment of the user's
 * own code, and syntax-colouring a fragment that is deliberately incomplete produces noise; what
 * matters is telling apart the parts *this language* added.
 *
 * ## What it completes
 *
 * The five things you cannot be expected to remember:
 *
 *   * the **clause keywords**, at the start of a line;
 *   * what `group` accepts — `file`, `module`, `enclosing`, **and the captures the query itself
 *     binds**, which is the one nobody could guess from documentation;
 *   * the **node kinds** after `#`, which are the grammar's vocabulary and are otherwise found
 *     only by opening the Trees panel and reading one off the Syntax tab;
 *   * the **denotations** `@type` and `@value` — the half of the language that answers a question
 *     the syntax cannot, and the half nobody discovers by guessing;
 *   * the **types** after a `:`, from the project's own class index — so `$x: Order` offers
 *     `com.acme.Order` rather than making you type a package you half remember.
 *
 * The captures come from the query text itself, which is why this file parses `$name$` twice: the
 * tokenizer does it to colour, and the completion source does it to offer. They are cheap and
 * independent, and sharing state between a CodeMirror stream parser and a completion source
 * would couple two things that run at different times for different reasons.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';

/** The words that begin a clause. Nothing in Java begins with one, which is what lets a line
 *  starting with them be read as a clause rather than as code. */
const CLAUSES = ['or', 'in', 'group', 'use'];

/** What `group` accepts besides a capture. */
const GROUP_KEYS = [
  { label: 'file', detail: 'one row per file' },
  { label: 'module', detail: 'one row per Maven module' },
  { label: 'enclosing', detail: 'one row per enclosing method or class' },
];

/**
 * The tree-sitter node kinds worth offering after `#`.
 *
 * A hand-picked list, not the grammar's full couple of hundred: the rest are internal
 * (`_expression`, `dimensions`) or so specific that anyone who wants them already knows the name
 * and can type it — the field does not restrict what you write, it only suggests. What is here
 * is what someone reaches for when they want "a literal", "a call", "a lambda".
 */
const NODE_KINDS = [
  ['string_literal', 'a "…" literal'],
  ['character_literal', "a '…' literal"],
  ['decimal_integer_literal', 'a whole number'],
  ['decimal_floating_point_literal', 'a decimal number'],
  ['true', 'the literal true'],
  ['false', 'the literal false'],
  ['null_literal', 'null'],
  ['identifier', 'a bare name'],
  ['type_identifier', 'a name used as a type'],
  ['method_invocation', 'a call'],
  ['object_creation_expression', 'a new …()'],
  ['field_access', 'a.b'],
  ['array_access', 'a[i]'],
  ['lambda_expression', 'a lambda'],
  ['method_reference', 'a Type::name'],
  ['cast_expression', 'a cast'],
  ['binary_expression', 'a op b'],
  ['ternary_expression', 'c ? a : b'],
  ['assignment_expression', 'a = b'],
  ['block', 'a { … }'],
  ['if_statement', 'an if'],
  ['for_statement', 'a for'],
  ['enhanced_for_statement', 'a for-each'],
  ['while_statement', 'a while'],
  ['try_statement', 'a try'],
  ['catch_clause', 'a catch'],
  ['return_statement', 'a return'],
  ['throw_statement', 'a throw'],
  ['class_declaration', 'a class'],
  ['interface_declaration', 'an interface'],
  ['method_declaration', 'a method'],
  ['field_declaration', 'a field'],
  ['annotation', 'an @Annotation'],
] as const;

/**
 * The same list for a page. A JSP grammar has a couple of dozen kinds and almost all of them are
 * worth offering, because unlike Java nobody has them memorised.
 */
const JSP_NODE_KINDS = [
  ['start_tag', 'an opening <tag …>'],
  ['end_tag', 'a closing </tag>'],
  ['self_closing_tag', 'a <tag …/>'],
  ['tag_name', 'the name in a tag'],
  ['attribute', 'a name="value" pair'],
  ['attribute_name', 'the name half of one'],
  ['attribute_fragment', 'literal text inside a value'],
  ['el_expression', 'a ${…}'],
  ['ognl_expression', 'a %{…}'],
  ['jsp_directive', 'a <%@ … %>'],
  ['jsp_scriptlet', 'a <% … %>'],
  ['jsp_expression', 'a <%= … %>'],
  ['jsp_declaration', 'a <%! … %>'],
  ['jsp_comment', 'a <%-- … --%>'],
  ['html_comment', 'an <!-- … -->'],
  ['script_element', 'a <script> and its body'],
  ['style_element', 'a <style> and its body'],
  ['text', 'a run of page text'],
] as const;

/**
 * Which language the query field is currently reading.
 *
 * Module-level for the same reason the replacement's captures are: a `LanguageDescriptor` is
 * taken once at mount and its completion source is a plain function, and exactly one structural
 * search is open at a time. It changes only what is *offered* — the field never restricts what
 * you type.
 */
let queryDialect: 'java' | 'jsp' = 'java';

export function setQueryDialect(dialect: 'java' | 'jsp'): void {
  queryDialect = dialect;
}

/** Every `$name$` / `$name...$` the text binds, in order, deduplicated. */
export function capturesIn(text: string): string[] {
  const out: string[] = [];
  for (const match of text.matchAll(/\$([A-Za-z0-9_]+)(?:\.\.\.)?(?::[^$]*)?\$/g)) {
    const name = match[1];
    if (!out.includes(name)) out.push(name);
  }
  return out;
}

/**
 * The tokenizer.
 *
 * Two pieces of state, and both are there because this language is context-sensitive in exactly
 * two places:
 *
 *   * `atLineStart` — a clause word is only a clause word at the start of a line, so `orders` is
 *     code and `or ders` is a clause. The same rule the parser applies; colouring them by a
 *     different one would be a lie in the field.
 *   * `inConstraint` — everything between a placeholder's `:` and its closing `$` is a different
 *     little language (`@type & Order+`, `!~get*`, `#string_literal`), and its operators want
 *     colouring apart from the name they constrain.
 */
const parser: StreamParser<{ atLineStart: boolean; inConstraint: boolean }> = {
  name: 'bennu-ssr',
  startState: () => ({ atLineStart: true, inConstraint: false }),

  token(stream, state) {
    if (stream.sol()) {
      state.atLineStart = true;
      // A constraint cannot span lines — an unclosed `$` is someone mid-typing, and carrying the
      // mode to the next line would paint the whole rest of the query as one.
      state.inConstraint = false;
    }
    if (stream.eatSpace()) return null;

    // ── inside `$x: … $` ───────────────────────────────────────────────────────
    if (state.inConstraint) {
      if (stream.eat('$')) {
        state.inConstraint = false;
        return 'variableName.special';
      }
      // The two that join and invert. Distinct from the constraints themselves, because
      // `!~get* & @value` is only readable if you can see where one part ends.
      if (stream.eat('&') || stream.eat('!')) return 'operator';
      if (stream.match(/^@[A-Za-z]*/)) return 'keyword'; // @type / @value
      if (stream.match(/^#[A-Za-z0-9_]*/)) return 'labelName'; // a grammar node kind
      if (stream.match(/^~[^$&]*/)) return 'string'; // a glob over the node's own text
      if (stream.match(/^[^$&!]+/)) return 'typeName'; // a type name, `*` and `+` included
      stream.next();
      return null;
    }

    // A comment, so a query worth keeping can say what it is for.
    if (state.atLineStart && stream.match('--')) {
      stream.skipToEnd();
      return 'comment';
    }

    if (state.atLineStart) {
      for (const word of CLAUSES) {
        // The trailing space is the rule the parser applies too: `in ` is a clause, `input` is
        // a receiver.
        if (stream.match(new RegExp(`^${word}(?=\\s)`))) {
          state.atLineStart = false;
          return 'keyword';
        }
      }
      // `use of … on …` — the two words inside it read as part of the clause.
      state.atLineStart = false;
    } else if (stream.match(/^(of|on)(?=\s)/)) {
      return 'keyword';
    }

    // A placeholder: `$name$`, `$name...$`, `$name: constraint$`.
    if (stream.peek() === '$') {
      stream.next();
      if (stream.eat('$')) return 'variableName.special'; // a literal `$$`, not a hole
      // `${` is EL, not a hole — a name cannot begin with a brace. The compiler reads it as a
      // literal `$`, so the field must not paint it as a mistake.
      if (stream.peek() === '{') return null;
      // `+`, not `*`: an unnamed `$$` is not a capture, and colouring it as one would hide a
      // typo the parser is about to refuse.
      const named = stream.match(/^[A-Za-z0-9_]+(\.\.\.)?/);
      if (stream.peek() === ':') {
        stream.next();
        state.inConstraint = true;
        return named ? 'variableName.special' : 'invalid';
      }
      stream.eat('$');
      return named ? 'variableName.special' : 'invalid';
    }

    stream.next();
    return null;
  },
};

/**
 * Where the token being completed starts, and what kind it is.
 *
 * The one thing this file decides for CodeMirror. Kept shallow deliberately: get it wrong and a
 * completion is inserted at a slightly wrong offset — never that the wrong candidates appear.
 */
function classify(context: CompletionContext) {
  const line = context.state.doc.lineAt(context.pos);
  const head = line.text.slice(0, context.pos - line.from);

  // `group ` — its keys and the query's own captures.
  const group = /^\s*group\s+(\$?[A-Za-z0-9_]*)$/.exec(head);
  if (group) return { kind: 'group' as const, from: context.pos - group[1].length };

  // Are we inside a placeholder's constraint? The last `$` on the line opened one, and it has
  // a `:` in it. One test instead of a regex per constraint form — which is what let `&` be
  // supported everywhere `:` is without three near-identical patterns.
  const open = head.lastIndexOf('$');
  if (open >= 0 && head.slice(open).includes(':')) {
    const denote = /@([A-Za-z]*)$/.exec(head);
    // `- 1` reaches back over the `@`, so accepting replaces it rather than doubling it.
    if (denote) return { kind: 'denote' as const, from: context.pos - denote[1].length - 1 };

    const kindAt = /#([A-Za-z0-9_]*)$/.exec(head);
    if (kindAt) return { kind: 'node' as const, from: context.pos - kindAt[1].length };

    const typeAt = /([A-Za-z0-9_.]*)$/.exec(head);
    if (typeAt) return { kind: 'type' as const, from: context.pos - typeAt[1].length };
  }

  // The start of a line — a clause keyword.
  const clause = /^(\s*)([a-z]*)$/.exec(head);
  if (clause) return { kind: 'clause' as const, from: context.pos - clause[2].length };

  return null;
}

/** The two denotations, offered wherever a constraint can go. */
const DENOTATIONS = [
  { label: '@type', type: 'keyword', detail: 'it names a class — a static access' },
  { label: '@value', type: 'keyword', detail: 'it names a variable or field — an instance one' },
];

async function complete(context: CompletionContext): Promise<CompletionResult | null> {
  const where = classify(context);
  if (!where) return null;
  // An explicit Ctrl+Space always answers; typing only answers once there is something to
  // narrow by, so the menu does not open on every space.
  if (!context.explicit && where.from === context.pos && where.kind !== 'group') return null;

  switch (where.kind) {
    case 'clause':
      return {
        from: where.from,
        options: [
          { label: 'or', type: 'keyword', detail: 'another shape of the same question' },
          { label: 'in', type: 'keyword', detail: 'scope it to a path' },
          { label: 'group', type: 'keyword', detail: 'count, instead of listing' },
          { label: 'use of', type: 'keyword', detail: 'every use of a member' },
        ],
      };

    case 'group': {
      // The captures the query itself binds — the completion nobody could get from a manual.
      const captures = capturesIn(context.state.doc.toString()).map((name) => ({
        label: `$${name}$`,
        type: 'variable',
        detail: 'one row per distinct match of this capture',
      }));
      return {
        from: where.from,
        options: [...captures, ...GROUP_KEYS.map((k) => ({ ...k, type: 'enum' }))],
      };
    }

    case 'node':
      return {
        from: where.from,
        options: (queryDialect === 'jsp' ? JSP_NODE_KINDS : NODE_KINDS).map(
          ([label, detail]) => ({ label, type: 'class', detail }),
        ),
      };

    case 'denote':
      return { from: where.from, options: DENOTATIONS };

    case 'type': {
      const root = projectStore.project?.root;
      // The denotations are offered even with no project open: they need no index, and they are
      // the half of the constraint language nobody guesses exists.
      if (!root) return { from: where.from, options: DENOTATIONS };
      const classes = await bennuIndexStore.classesForRoot(root);
      return {
        from: where.from,
        options: [
          ...DENOTATIONS,
          ...classes.map((c) => ({
            // The fully-qualified name: on a legacy tree four classes are called `Order`, and
            // the package is the only thing that says which one.
            label: c.fqcn,
            type: 'class',
            detail: c.simple,
          })),
        ],
        // The whole class index, filtered by CodeMirror rather than re-fetched per keystroke.
        // `@` is in the set so typing it keeps the list alive and narrows it to the two.
        validFor: /^[@A-Za-z0-9_.]*$/,
      };
    }
  }
}

/** The descriptor a query field hands to {@link CodeEditor}. */
export const ssrQueryLanguage: LanguageDescriptor = {
  id: 'bennu-ssr',
  // Never called: `cmExtension` takes precedence, and this language has no tree-sitter grammar.
  createParser: () => Promise.reject(new Error('the query language is a stream mode')),
  cmExtension: StreamLanguage.define(parser),
  intel: { completion: complete },
};

/**
 * The replacement field's language.
 *
 * The same tokenizer — a template is holes plus code, exactly like a pattern — but it completes
 * only the captures, because that is the only thing a template may name. Offering clause
 * keywords there would suggest a template can be scoped or grouped, which it cannot.
 */
export const ssrReplacementLanguage: LanguageDescriptor = {
  id: 'bennu-ssr-replacement',
  createParser: () => Promise.reject(new Error('the query language is a stream mode')),
  cmExtension: StreamLanguage.define(parser),
  intel: {
    completion: (context: CompletionContext) => {
      const line = context.state.doc.lineAt(context.pos);
      const head = line.text.slice(0, context.pos - line.from);
      const at = /\$([A-Za-z0-9_]*)$/.exec(head);
      if (!at) return null;
      // Deliberately the captures of the QUERY, which this field cannot see — the panel keeps
      // them in sync by passing the query's text through `capturesIn`. See `BennuSsrModal`.
      const names = ssrReplacementCaptures;
      if (!names.length) return null;
      return {
        from: context.pos - at[1].length,
        options: names.map((name) => ({ label: `${name}$`, type: 'variable', detail: 'a capture' })),
      };
    },
  },
};

/**
 * The capture names the replacement field offers.
 *
 * A module-level value rather than a prop because a `LanguageDescriptor` is static — the editor
 * takes it at mount and its completion source is a plain function. The panel writes this
 * whenever the query changes; there is exactly one structural-search modal open at a time, so
 * there is nothing to key it by.
 */
export let ssrReplacementCaptures: string[] = [];

export function setReplacementCaptures(names: string[]): void {
  ssrReplacementCaptures = names;
}
