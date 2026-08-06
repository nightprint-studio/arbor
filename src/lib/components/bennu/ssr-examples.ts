/**
 * The queries the panel offers to start from.
 *
 * Not a tutorial. Each one is a question somebody on a legacy Java project actually arrives
 * with, and between them they use every part of the language once — which is why they work as
 * an introduction without being written as one. A structural query is easy to get subtly wrong,
 * and the fastest way to learn the shape is to edit one that already works.
 *
 * Kept out of the modal so the modal stays a modal: this is content, and it will grow.
 */

export interface SsrExample {
  /** What it answers, in the words someone would ask it. */
  title: string;
  /** Why it is worth having, in one line. */
  why: string;
  query: string;
}

export const SSR_EXAMPLES: SsrExample[] = [
  {
    title: 'Which methods of a class are used',
    why: 'and how often each — the census before a refactor',
    query: 'use of $m$ on com.acme.OrderService\ngroup $m$',
  },
  {
    title: 'Which of my methods use a deprecated API',
    why: '`enclosing` is the only way to ask this',
    query: 'new $x: java.text.SimpleDateFormat$($p...$)\ngroup enclosing',
  },
  {
    title: 'Logging that was concatenated instead of parameterised',
    why: 'the most common fix, and it is a real replacement',
    query: 'log.$lvl: ~debug|info|warn$("$s$" + $x$)\ngroup $lvl$',
  },
  {
    title: 'Statements that could leak',
    why: 'every createStatement, per module',
    query: '$c$.createStatement()\ngroup module',
  },
  {
    title: 'Actions by the base class they extend',
    why: 'the shape of a Struts codebase in one table',
    query: 'class $c$ extends $b: com.acme.BaseAction+$ { $body...$ }\ngroup $b$',
  },
  {
    title: 'JUnit 4 asserts with the message first',
    why: 'a reorder no textual replace can do — set a replacement of assertEquals($a$, $b$, $msg$)',
    query: 'assertEquals($msg: #string_literal$, $a$, $b$)',
  },
  {
    title: 'A call and its method reference, together',
    why: 'what `or` is for — and both count once',
    query: '$o$.place($a...$)\nor $o$::place\ngroup file',
  },
  {
    title: 'Every string literal in one package',
    why: 'the table names a DAO layer touches, roughly',
    query: '$s: #string_literal$\nin src/main/java/com/acme/dao\ngroup $s$',
  },
];
