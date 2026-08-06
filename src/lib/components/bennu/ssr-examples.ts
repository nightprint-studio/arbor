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
  /** The language it is written in. Absent means Java, which is what every example was. */
  dialect?: 'java' | 'jsp' | 'jsp-java';
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
  // Removed rather than left to fail: "Actions by the base class they extend", which was
  // `class $c$ extends $b$ { $body...$ }`. A placeholder stands in for a *name*, and a bare name
  // is not a class member — so that pattern never compiled and the template could only ever
  // report an error. A class-shaped query needs a placeholder recognised through a wrapper,
  // which the engine does not have; offering it meanwhile taught the language wrong.
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

  // ── pages ─────────────────────────────────────────────────────────────────
  //
  // A legacy Struts codebase keeps as much of its logic in JSPs as in classes, and a text
  // search is even weaker there than over Java: the same tag is written across four lines as
  // often as one, so grepping `<s:property value=` finds a fraction of them.
  //
  // Every one of these is written with `$pre...$` / `$post...$` around the attribute it cares
  // about, and that is the idiom rather than clutter. A tag's attributes are matched **in
  // order and in full** — the engine compares children, and it has no notion of a set — so
  // `<s:property value="$x$"/>` finds only the tags whose one and only attribute is `value`.
  // The two runs let the rest of them be there, in any order, which is how real pages are
  // written.
  {
    title: 'Every property a page prints',
    why: 'the value stack the JSPs actually depend on, counted',
    query: '<s:property $pre...$ value="$x$" $post...$/>\ngroup $x$',
    dialect: 'jsp',
  },
  {
    title: 'What the pages iterate over',
    why: 'the collections the actions are expected to expose',
    query: '<s:iterator $pre...$ value="$list$" $post...$>\ngroup $list$',
    dialect: 'jsp',
  },
  {
    title: 'The fields the forms post',
    why: 'every name a page submits — the other half of the action-property lint',
    query: '<s:textfield $pre...$ name="$n$" $post...$/>\ngroup $n$',
    dialect: 'jsp',
  },
  {
    title: 'Inline styles left in the markup',
    why: 'the cleanup list, per file — and `$tag$` says it works on any tag',
    query: '<$tag$ $pre...$ style="$css$" $post...$>\ngroup file',
    dialect: 'jsp',
  },

  // ── the Java inside the pages ───────────────────────────────────────────────
  //
  // The other half of a legacy page, and the half a JSP query cannot reach: to the page grammar
  // a `<% … %>` is one token. These are ordinary Java patterns; only the files walked differ.
  {
    title: 'Everything the pages read out of the session',
    why: 'the keys a rewrite would have to keep working',
    query: 'session.getAttribute($key$)\ngroup $key$',
    dialect: 'jsp-java',
  },
  {
    title: 'Everything the pages put into it',
    why: 'the other direction — where page state is actually born',
    query: 'session.setAttribute($key$, $value$)\ngroup $key$',
    dialect: 'jsp-java',
  },
  {
    title: 'Pages that talk to the database directly',
    why: 'the list nobody wants to have, and exactly the one worth having',
    query: '$c$.createStatement()\nor $c$.prepareStatement($sql...$)\ngroup file',
    dialect: 'jsp-java',
  },
  {
    title: 'Classes the pages instantiate',
    why: 'what a page depends on without any import saying so',
    query: 'new $type$($args...$)\ngroup $type$',
    dialect: 'jsp-java',
  },
];
