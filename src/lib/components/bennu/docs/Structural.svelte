<script lang="ts">
  /**
   * Structural search, the query language: why a pattern is not a regex, holes and where they can go, constraints,
   * static versus instance, the three clauses, and `use of`. Running a query is StructuralRun.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Structural search</span>
<h1>Structural search</h1>

<p class="doc-lead">
  Find code by its <strong>shape</strong> rather than by its text — then count it, or rewrite it. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>M</kbd>. This page is the query language; running one is <strong>Searching &amp; replacing</strong>.
</p>

<h2>Why it is not a better regex</h2>
<p>A text search knows nothing about Java, so these are two different strings:</p>
<pre><code>{@html highlightCode(`log.debug("order " + id)

log
  .debug( "order "
        + id )`, 'java')}</code></pre>
<p>
  — and one construct. A structural search compares <strong>nodes</strong>, so whitespace, line breaks and comments in the middle take no part in
  the match. And because each hole is <em>captured</em>, a replacement can move the parts around, which no textual find and replace can do:
</p>
<pre><code>assertEquals($msg$, $a$, $b$)   →   assertEquals($a$, $b$, $msg$)</code></pre>

<h2>The pattern is the language it searches</h2>
<p>
  There is no second syntax. A pattern is the code you are looking for, with <code>$holes$</code> where it may differ, parsed <strong>with the same
  grammar as the file</strong> — which is exactly what makes the match structural. If you can write the statement, you can write its pattern.
</p>
<p>
  The picker beside the query field says which language that is: <strong>Java</strong> searches <code>.java</code>; <strong>JSP</strong>
  searches <code>.jsp</code>, <code>.jspf</code>, <code>.jspx</code> and <code>.tag</code>; <strong>Java in JSP</strong> searches the Java inside
  pages — see <strong>Searching pages</strong>.
</p>
<Callout variant="info" title="Asked, not guessed">
  A pattern compiled against the wrong grammar does not fail loudly: it matches nothing, which reads as "the project contains none of this" — the one
  wrong answer a search must never give quietly.
</Callout>

<h3>Two kinds of hole</h3>
<table>
  <thead><tr><th>Hole</th><th>Matches</th></tr></thead>
  <tbody>
    <tr><td><code>$x$</code></td><td><strong>One</strong> node, captured under that name.</td></tr>
    <tr><td><code>$xs...$</code></td><td>A <strong>run of consecutive siblings</strong>, possibly none — an argument list, a class body, a chain of statements. It captures the original bytes, separators and all, so a captured <code>1, 2, 3</code> comes back as written.</td></tr>
  </tbody>
</table>

<h3>Where a hole can go</h3>
<p>
  A hole is read from where it stands, so it can be the thing that belongs there — not only a name:
</p>
<table>
  <thead><tr><th>Where the hole stands</th><th>It is…</th></tr></thead>
  <tbody>
    <tr><td>a receiver, a method, a type, an argument</td><td>that node. <code>f($args...$)</code> is a run of arguments.</td></tr>
    <tr><td>alone in a <strong>block</strong> — <code>&#123; $body...$ &#125;</code>, <code>if ($c$) &#123; $stmts...$ &#125;</code></td><td>a <strong>statement</strong>, or a run of them (<code>...</code>). Loops, <code>if</code>, <code>try</code>, lambdas.</td></tr>
    <tr><td>alone in a <strong>class body</strong> — <code>class $c$ extends $b$ &#123; $members...$ &#125;</code></td><td>a <strong>member</strong> (method, field, nested type), or a run of them. Classes, interfaces, records; for an enum, after the constants and a <code>;</code>.</td></tr>
    <tr><td>alone in a <strong>parameter list</strong> — <code>void $m$($params...$)</code></td><td>a <strong>parameter</strong>, whole with its type, or a run of them.</td></tr>
    <tr><td><code>$mods...$</code> in <strong>front of a declaration</strong> — <code>$mods...$ final $t$ $n$ = $v$;</code></td><td>zero or more <strong>modifiers and annotations</strong>, none included: the declaration need not have any. Captured, so a replacement can put them back.</td></tr>
  </tbody>
</table>
<p>
  A declaration pattern is exact about its modifiers: <code>final $t$ $n$ = $v$;</code> matches a local that is <code>final</code> and nothing else.
  Write <code>$mods...$</code> first to accept any, or <code>$pre...$ final $post...$</code> to require <code>final</code> among others. The same
  goes for a class: <code>$mods...$ class $c$ extends $b$ &#123; $members...$ &#125;</code> is any class extending <code>$b$</code>, public or not;
  a class with an <code>implements</code> the pattern does not mention is a different shape, so add it or use <code>or</code>.
</p>

<h3>Declarations</h3>
<p>
  Classes, interfaces, enums and records (with <code>extends</code>, <code>implements</code>, generics, annotations), methods, constructors, fields, locals,
  parameters, <code>catch</code> parameters and lambda parameters can all be written as patterns:
</p>
<pre><code>@Autowired private $t$ $n$;
catch ($t$ $e$) &#123; &#125;
for (int $i$ = 0; $i$ &lt; $l$.size(); $i$++) &#123; $body...$ &#125;
$mods...$ $r$ $m$($params...$) throws $e$ &#123; $body...$ &#125;</code></pre>

<h3><code>as</code> — how the text is to be read</h3>
<p>
  <code>final $t$ $n$ = $v$;</code> is legal Java as a field and as a local variable, and nothing in the text says which. Without help the compiler took the
  first reading that parsed — a field — so there was no way to ask for a local. <code>as</code> says which, on a line of its own, and applies to the
  alternative it follows:
</p>
<table>
  <thead><tr><th>Line</th><th>The pattern is…</th></tr></thead>
  <tbody>
    <tr><td><code>as local</code></td><td>a local variable declaration</td></tr>
    <tr><td><code>as param</code></td><td>a method or constructor parameter (one)</td></tr>
    <tr><td><code>as field</code></td><td>a field</td></tr>
    <tr><td><code>as member</code></td><td>a method, field or nested type of a class body</td></tr>
    <tr><td><code>as statement</code>, <code>as expression</code></td><td>a statement, an expression</td></tr>
    <tr><td><code>as class</code></td><td>a class, interface, enum or record</td></tr>
    <tr><td><code>as catch</code></td><td>a <code>catch</code> clause</td></tr>
  </tbody>
</table>
<p>
  With <code>as</code> the pattern is read in that one way and no other, so a match is never found twice and a field never answers for a local.
</p>

<h3><code>inside</code> — where the match lives</h3>
<pre><code>g()
inside method, lambda</code></pre>
<p>
  The match must be in the <strong>nearest</strong> enclosing declaration of one of the kinds listed: <code>method</code>, <code>constructor</code>,
  <code>initializer</code>, <code>lambda</code>, <code>class</code>, <code>interface</code>, <code>enum</code>, <code>record</code>,
  <code>anonymous</code> — or <code>body</code> for the first four and <code>type</code> for the last five. A field is <code>inside class</code>; a
  local or a parameter of a method is <code>inside method</code>; the same code in a lambda in that method is <code>inside lambda</code>. (<code>in</code>
  is already a path, which is why this is not spelled that way.)
</p>

<h3>Removing <code>final</code> from locals and parameters, not fields</h3>
<p>Two queries, each with its replacement — run them one after the other:</p>
<pre><code>$pre...$ final $post...$ $t$ $n$ = $v$;
as local
→  $pre$ $post$ $t$ $n$ = $v$;

$pre...$ final $post...$ $t$ $n$
as param
→  $pre$ $post$ $t$ $n$</code></pre>
<p>
  Other modifiers and annotations stay (<code>@Deprecated final int k</code> becomes <code>@Deprecated int k</code>); a hole that matched nothing takes
  the space after it, so nothing is left indented. Fields are not locals, so they are untouched. Both are in the Templates column.
</p>

<h3>Questions about the model</h3>
<p>
  Asking about the structure of the code — which classes, relations, beans, endpoints — is a declaration pattern with a constraint, answered with the
  project's index:
</p>
<table>
  <thead><tr><th>Question</th><th>Query</th></tr></thead>
  <tbody>
    <tr><td>Every use of a member</td><td><code>use of $m$ on com.acme.Type</code></td></tr>
    <tr><td>Subclasses of a type</td><td><code>$mods...$ class $c$ extends $b: com.acme.Base+$ &#123; $members...$ &#125;</code></td></tr>
    <tr><td>Classes by superclass or interface</td><td><code>$mods...$ class $c$ implements $i$ &#123; $members...$ &#125;</code> then <code>group $i$</code></td></tr>
    <tr><td>JPA entities</td><td><code>@Entity $mods...$ class $c$ &#123; $members...$ &#125;</code></td></tr>
    <tr><td>Their relations</td><td><code>@ManyToOne $mods...$ $t$ $n$;</code> with <code>as field</code>, and an <code>or</code> with <code>@ManyToOne($args...$)</code></td></tr>
    <tr><td>Injected fields</td><td><code>@Autowired $mods...$ $t$ $n$;</code> with <code>as field</code></td></tr>
    <tr><td>Methods taking a type</td><td><code>$mods...$ $r$ $m$($pre...$, $p: Order$ $n$, $post...$) &#123; $body...$ &#125;</code></td></tr>
  </tbody>
</table>
<p>
  These are patterns, so what a class <em>says</em> is found and what it inherits is not; a constraint the index cannot decide is kept and marked
  undecided. This is the minimal set — <code>use of</code> is the only form with a query kind of its own.
</p>

<h2>Constraining a hole</h2>
<p>A colon after the name narrows what the hole accepts:</p>
<table>
  <thead><tr><th>Written</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td><code>$x: com.acme.Order$</code></td><td>its static type <em>is</em> that</td></tr>
    <tr><td><code>$x: Order$</code></td><td>the same, matched on the simple name</td></tr>
    <tr><td><code>$x: Order+$</code></td><td>…or a subtype — the <code>+</code> walks the hierarchy</td></tr>
    <tr><td><code>$x: *Dao$</code></td><td>a glob on the type name</td></tr>
    <tr><td><code>$x: #string_literal$</code></td><td>a node of the <strong>grammar</strong>'s kind, not a type</td></tr>
    <tr><td><code>$x: ~get*$</code></td><td>a glob on the node's own <strong>text</strong></td></tr>
    <tr><td><code>$x: @type$</code></td><td>it <strong>names a class</strong> — a static access</td></tr>
    <tr><td><code>$x: @value$</code></td><td>it names a variable, parameter or field — an instance access</td></tr>
    <tr><td><code>$x: !equals$</code></td><td>the negation of any of the above</td></tr>
    <tr><td><code>$x: @type &amp; Files$</code></td><td>all of them at once</td></tr>
  </tbody>
</table>
<ul>
  <li>The <code>#</code> keeps two vocabularies apart: a <em>type</em> is a word from Java, a <em>kind</em> a word from the grammar —
    <code>string_literal</code>, <code>method_invocation</code>, <code>lambda_expression</code>. The <strong>Trees</strong> panel
    (<kbd>Alt</kbd> + <kbd>9</kbd>) says what a construct is called: click a node on its <em>Syntax</em> tab.</li>
  <li><code>~</code> and the type globs are <strong>globs, not regexes</strong>: <code>*</code> is any run, <code>|</code> separates alternatives,
    everything else is literal, and the whole is anchored — <code>~place|cancel</code>, <code>~get*</code>, <code>*Service</code>.</li>
  <li><code>&amp;</code> binds <strong>looser</strong> than <code>!</code>, so <code>!~get* &amp; @value</code> reads "not a getter, and a value".
    There are no parentheses: a constraint needing them is better written as two alternatives.</li>
</ul>

<h3>Static or instance: <code>@type</code> and <code>@value</code></h3>
<p>
  <code>orders.total()</code> and <code>Orders.total()</code> are the <strong>same shape</strong> — a call whose receiver is an identifier — so no
  pattern separates them. The difference is in what the name <em>denotes</em>, a question for the resolver, and <code>@type</code> and
  <code>@value</code> are how you ask it:
</p>
<table>
  <thead><tr><th>Query</th><th>Finds</th></tr></thead>
  <tbody>
    <tr><td><code>$o: @type$.$m$($a...$)</code></td><td>every static call, anywhere</td></tr>
    <tr><td><code>$o: @type &amp; java.nio.file.Files$.$m$($a...$)</code><br /><code>group $m$</code></td><td>which <code>Files</code> statics this project uses, and how often each</td></tr>
    <tr><td><code>$o: @value &amp; Connection$.$m$($a...$)</code></td><td>calls on a JDBC connection, never on the class</td></tr>
  </tbody>
</table>
<p>
  A plain type constraint stays blind to the distinction on purpose: <code>$o: Order$</code> admits <code>order.f()</code> <em>and</em>
  <code>Order.f()</code>, since the type in play is <code>Order</code> in both. Add <code>@value</code> or <code>@type</code> when you mean one.
</p>
<Callout variant="info" title="Undecided is not no">
  A receiver the classpath cannot reach comes back <strong>undecided</strong>, not "not a type" — turning an unknown into a no is how an incomplete
  classpath starts inventing answers.
</Callout>

<h2>The clauses</h2>
<p>
  A query is a pattern, optionally followed by lines beginning with <code>or</code>, <code>in</code>, <code>group</code>, <code>as</code> or
  <code>inside</code> (the last two are described under the holes above). No Java construct begins with those words, so such a line is never mistaken
  for code — <code>orders.total()</code> is still a pattern.
</p>

<h3><code>or</code> — the same question, another shape</h3>
<pre><code>$o$.$m$($args...$)
or $o$::$m$
group $m$</code></pre>
<p>Java spells a method use several ways, and a count seeing only one is simply wrong. Two rules keep it safe:</p>
<ul>
  <li><strong>A capture used to group must be bound by every alternative.</strong> Otherwise the table would have rows with an empty column, and a
    hole in an aggregate reads as "none" rather than "this branch cannot answer". The query is refused as you write it, naming the branch missing it.</li>
  <li><strong>One place in a file is one hit.</strong> <code>$o$.$m$()</code> and <code>$o$.close()</code> both match <code>stream.close()</code>, and
    counting that twice gives a plausible, wrong number. Hits are de-duplicated by range.</li>
</ul>

<h3><code>in</code> — where to look</h3>
<pre><code>in modules/core, modules/web</code></pre>
<p>Project-relative path prefixes, comma-separated. Without it, everywhere.</p>

<h3><code>group</code> — the statistics</h3>
<p>
  With a <code>group</code>, the answer stops being a list of places and becomes a <strong>table</strong>: the key, how many, in how many files. Four
  keys, and deliberately no more — each is a question people arrive with:
</p>
<table>
  <thead><tr><th>Key</th><th>Answers</th></tr></thead>
  <tbody>
    <tr><td><code>group $m$</code></td><td>which methods, and how often each</td></tr>
    <tr><td><code>group file</code></td><td>which files, and how many per file</td></tr>
    <tr><td><code>group module</code></td><td>which module is full of this</td></tr>
    <tr><td><code>group enclosing</code></td><td>which of <em>my</em> methods do this</td></tr>
  </tbody>
</table>
<p>
  <code>enclosing</code> cannot be written any other way: the method a match sits in is not part of the pattern, so there is nothing to capture — yet
  "who in my code calls this" is the question asked most.
</p>

<h2><code>use of</code> — every shape at once</h2>
<pre><code>use of $m$ on com.acme.OrderService+
group $m$</code></pre>
<p>
  <em>Which methods of a class are used, where, and how often</em> — without enumerating the ways Java spells a use. It is a shortcut for a set of
  patterns, and the panel <strong>shows the expansion</strong>, so it teaches rather than hides: copy it out, edit it, run it as an ordinary query.
</p>
<p>
  Name a member — <code>use of place on …</code> — to pin it, or leave <code>$m$</code> to count them all. The <code>+</code> includes uses through a subtype.
</p>
<Callout variant="info" title="What it covers">
  Uses <em>through a reference to the type</em>: a call on a receiver, and both forms of method reference. Not a class calling its own member —
  <code>this.place(o)</code>, a bare <code>place(o)</code>, <code>super.place(o)</code> — which needs the enclosing hierarchy a pattern cannot express.
  That is usually what you want anyway: "who uses OrderService" is about its consumers, and its own internals would inflate every answer.
</Callout>
