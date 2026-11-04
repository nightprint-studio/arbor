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
  /** What it rewrites to, when the example is a refactoring and not only a question. */
  replacement?: string;
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

  // ── rewrites ──────────────────────────────────────────────────────────────
  //
  // Each of these carries its replacement, and each was run against a one-line sample of its own
  // before it was written down here — a template that matches nothing teaches the wrong thing.
  // A hole can also stand for a whole statement or member (`{ $body...$ }`), for the parameters of a
  // method (`m($params...$)`) and — in front of a declaration — for any modifiers and annotations
  // (`$mods...$`); `as` says whether a declaration is a local, a parameter or a field, and `inside`
  // where it lives. The templates below that use them were written against the real engine's tests.
  {
    title: 'Diamond instead of the repeated type',
    why: '`new ArrayList<String>()` → `new ArrayList<>()`, the arguments kept',
    query: 'new ArrayList<$t$>($a...$)',
    replacement: 'new ArrayList<>($a...$)',
  },
  {
    title: 'Constant first in equals',
    why: 'null-safe: `name.equals("admin")` → `"admin".equals(name)`',
    query: '$x$.equals("$lit$")',
    replacement: '"$lit$".equals($x$)',
  },
  {
    title: 'Boxing constructors',
    why: '`new Integer(5)` → `Integer.valueOf(5)`, for the types the JDK deprecated',
    query: 'new $t: ~Integer|Long|Double$($v$)',
    replacement: '$t$.valueOf($v$)',
  },
  {
    title: 'System.out left in the code',
    why: 'to the logger — adjust the receiver to the one your project uses',
    query: 'System.out.println($msg$)',
    replacement: 'log.info($msg$)',
  },
  {
    title: 'printStackTrace',
    why: 'the stack trace goes to the log, with a message and the exception',
    query: '$e$.printStackTrace()',
    replacement: 'log.error("Unexpected error", $e$)',
  },
  {
    title: 'if (c) return true; else return false;',
    why: 'the whole statement becomes `return c;`',
    query: 'if ($c$) { return true; } else { return false; }',
    replacement: 'return $c$;',
  },
  {
    title: 'size() == 0',
    why: '→ `isEmpty()`; the same shape with `> 0` is the next one',
    query: '$l$.size() == 0',
    replacement: '$l$.isEmpty()',
  },
  {
    title: 'size() > 0',
    why: '→ `!isEmpty()`',
    query: '$l$.size() > 0',
    replacement: '!$l$.isEmpty()',
  },
  {
    title: 'length() == 0 on a string',
    why: '→ `isEmpty()`; narrow with `$s: String$` when other types have a length()',
    query: '$s$.length() == 0',
    replacement: '$s$.isEmpty()',
  },
  {
    title: 'A null check that picks a default',
    why: '`x == null ? d : x` → `Objects.requireNonNullElse(x, d)`',
    query: '$a$ == null ? $b$ : $a$',
    replacement: 'Objects.requireNonNullElse($a$, $b$)',
  },
  {
    title: 'Arrays.asList',
    why: '→ `List.of(…)`; the result is immutable, so read the matches before replacing',
    query: 'Arrays.asList($a...$)',
    replacement: 'List.of($a...$)',
  },
  {
    title: 'stream().collect(Collectors.toList())',
    why: '→ `.toList()` (Java 16+) — the same result, shorter, unmodifiable',
    query: '$s$.stream().map($f$).collect(Collectors.toList())',
    replacement: '$s$.stream().map($f$).toList()',
  },
  {
    title: 'StringBuffer where nobody shares it',
    why: '→ `StringBuilder`; check the matches for fields used across threads first',
    query: 'new StringBuffer($a...$)',
    replacement: 'new StringBuilder($a...$)',
  },
  {
    title: 'final on local variables',
    why: 'takes `final` off locals, keeps the other modifiers and the annotations — fields are not locals, so they stay',
    query: '$pre...$ final $post...$ $t$ $n$ = $v$;\nas local',
    replacement: '$pre$ $post$ $t$ $n$ = $v$;',
  },
  {
    title: 'final on parameters',
    why: 'the same for method and constructor parameters — run it after the one for locals to clear both',
    query: '$pre...$ final $post...$ $t$ $n$\nas param',
    replacement: '$pre$ $post$ $t$ $n$',
  },
  {
    title: 'Parameters without final',
    why: 'adds `final` where a parameter has no modifier at all',
    query: '$t$ $n$\nas param',
    replacement: 'final $t$ $n$',
  },
  {
    title: 'Locals declared inside a lambda',
    why: '`inside` says where a match lives: here the nearest enclosing thing is a lambda',
    query: '$mods...$ $t$ $n$ = $v$;\nas local\ninside lambda',
  },
  {
    title: 'Empty catch blocks',
    why: 'the exception is swallowed — `group enclosing` names the methods',
    query: 'catch ($t$ $e$) { }\ngroup enclosing',
  },
  {
    title: 'Loops over a list by index',
    why: 'the candidates for a for-each; `$body...$` is the whole body, whatever it holds',
    query: 'for (int $i$ = 0; $i$ < $l$.size(); $i$++) { $body...$ }',
  },
  {
    title: 'Injected fields, by type',
    why: 'every `@Autowired` field — the census before moving to constructor injection',
    query: '@Autowired $mods...$ $t$ $n$;\nas field\ngroup $t$',
  },
  {
    title: 'Classes by what they extend',
    why: '`$mods...$` accepts any modifiers and annotations, `$members...$` any body',
    query: '$mods...$ class $c$ extends $b$ { $members...$ }\ngroup $b$',
  },
  {
    title: 'Classes by the interface they implement',
    why: 'the implementations of each interface, counted',
    query: '$mods...$ class $c$ implements $i$ { $members...$ }\ngroup $i$',
  },
  {
    title: 'Subclasses of one type',
    why: 'a type constraint on the superclass; the index decides, and what it cannot read is marked undecided',
    query: '$mods...$ class $c$ extends $b: com.acme.BaseDao+$ { $members...$ }',
  },
  {
    title: 'JPA entities',
    why: 'every class annotated `@Entity`, per file — add `@Table(…)` to the pattern to narrow',
    query: '@Entity $mods...$ class $c$ { $members...$ }\ngroup file',
  },
  {
    title: 'Entity relations',
    why: 'the `@ManyToOne` fields, with or without arguments, by target type',
    query: '@ManyToOne $mods...$ $t$ $n$;\nas field\nor @ManyToOne($args...$) $mods...$ $t$ $n$;\nas field\ngroup $t$',
  },
  {
    title: 'Where the code sleeps',
    why: 'every `Thread.sleep`, per file — usually a test waiting for something',
    query: 'Thread.sleep($ms$)\ngroup file',
  },
  {
    title: 'Where the code asks for the clock',
    why: 'every `Calendar.getInstance()` — the list a java.time migration starts from',
    query: 'Calendar.getInstance()\ngroup file',
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
