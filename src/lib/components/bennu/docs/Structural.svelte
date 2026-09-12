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
  While the pattern is parsed a placeholder stands in for an <em>identifier</em>, so it can sit anywhere a name can — a receiver, a method, an
  argument, a type. That is a real limit, and it decides what you can ask:
</p>
<table>
  <thead><tr><th>A run of…</th><th></th></tr></thead>
  <tbody>
    <tr><td><code>f($args...$)</code> — arguments</td><td><strong>Works.</strong> An argument is an expression, and a name is one.</td></tr>
    <tr><td><code>&#123; $body...$ &#125;</code> — statements or members</td><td><strong>Does not.</strong> <code>class A &#123; body &#125;</code> is not Java, so the pattern does not compile and the field says so. There is currently <em>no</em> way to write "a class extending X, whatever its body".</td></tr>
    <tr><td><code>void $m$($args...$)</code> — parameters</td><td><strong>Does not</strong>, for the same reason: a parameter needs a type.</td></tr>
  </tbody>
</table>

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

<h2>Three clauses</h2>
<p>
  A query is a pattern, optionally followed by lines beginning with <code>or</code>, <code>in</code> or <code>group</code>. No Java construct begins
  with those words, so such a line is never mistaken for code — <code>orders.total()</code> is still a pattern.
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
