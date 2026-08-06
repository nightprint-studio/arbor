<!-- Bennu docs — structural search & replace: the query language, the statistics, the rewrite. -->
<h1>Structural search &amp; replace</h1>
<p class="doc-lead">
  Find code by its <strong>shape</strong> rather than by its text — then count it, or rewrite it.
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd>.
</p>

<h2>Why it is not a better regex</h2>
<p>
  A text search knows nothing about Java, so these are two different strings:
</p>
<pre><code>log.debug("order " + id)

log
  .debug( "order "
        + id )</code></pre>
<p>
  and one construct. A structural search compares <strong>nodes</strong>, so whitespace, line
  breaks and comments in the middle take no part in the match. And because each hole is
  <em>captured</em>, a replacement can move its parts around — which no textual find/replace can
  do at all:
</p>
<pre><code>assertEquals($msg$, $a$, $b$)   →   assertEquals($a$, $b$, $msg$)</code></pre>

<h2>The pattern is the language it searches</h2>
<p>
  There is no second syntax. A pattern is the code you are looking for, with
  <code>$holes$</code> where it may differ, and it is parsed <strong>with the same grammar as the
  file</strong> — which is exactly what makes the match structural. If you can write the
  statement, you can write the pattern for it.
</p>
<p>
  Which language that is, is the picker beside the query field: <strong>Java</strong> searches
  <code>.java</code>, <strong>JSP</strong> searches <code>.jsp</code> / <code>.jspf</code> /
  <code>.jspx</code> / <code>.tag</code>. It is asked rather than guessed, because a pattern
  compiled against the wrong grammar does not fail loudly — it matches nothing, which reads as
  “the project contains none of this”, and that is the one wrong answer a search must never give
  quietly.
</p>
<p>Two kinds of hole, and no more:</p>
<table class="doc-table">
  <thead><tr><th>Hole</th><th>Matches</th></tr></thead>
  <tbody>
    <tr>
      <td><code>$x$</code></td>
      <td><strong>one</strong> node, captured under that name.</td>
    </tr>
    <tr>
      <td><code>$xs...$</code></td>
      <td>
        a <strong>run of consecutive siblings</strong>, possibly none — an argument list, a class
        body, a chain of statements. It captures the original bytes, separators and all, so a
        captured <code>1, 2, 3</code> comes back as it was written rather than as something
        rejoined by guesswork.
      </td>
    </tr>
  </tbody>
</table>
<p>
  <strong>Where a hole can go.</strong> A placeholder stands in for an <em>identifier</em> while
  the pattern is parsed, so it can sit anywhere a name can — a receiver, a method, an argument, a
  type. This is a real limit and it is worth knowing rather than discovering, because it decides
  what you can ask:
</p>
<table class="doc-table">
  <thead><tr><th>A run of…</th><th></th></tr></thead>
  <tbody>
    <tr>
      <td><code>f($args...$)</code> — arguments</td>
      <td><strong>Works.</strong> An argument is an expression, and a name is one.</td>
    </tr>
    <tr>
      <td><code>&#123; $body...$ &#125;</code> — statements or class members</td>
      <td>
        <strong>Does not.</strong> <code>class A &#123; body &#125;</code> is not Java, so the
        pattern does not compile and the field says so. There is currently <em>no</em> way to
        write “a class extending X, whatever its body”.
      </td>
    </tr>
    <tr>
      <td><code>void $m$($args...$)</code> — parameters</td>
      <td><strong>Does not</strong>, for the same reason: a parameter needs a type.</td>
    </tr>
  </tbody>
</table>

<h2>Constraining a hole</h2>
<p>
  A colon after the name narrows what it will accept.
</p>
<table class="doc-table">
  <thead><tr><th>Written</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td><code>$x: com.acme.Order$</code></td><td>its static type <em>is</em> that</td></tr>
    <tr><td><code>$x: Order$</code></td><td>the same, matched on the simple name</td></tr>
    <tr><td><code>$x: Order+$</code></td><td>…or a subtype of it — the <code>+</code> walks the hierarchy</td></tr>
    <tr><td><code>$x: *Dao$</code></td><td>a glob on the type name</td></tr>
    <tr><td><code>$x: #string_literal$</code></td><td>a node of the <strong>grammar</strong>'s kind, not a type</td></tr>
    <tr><td><code>$x: ~get*$</code></td><td>a glob on the node's own <strong>text</strong></td></tr>
    <tr><td><code>$x: @type$</code></td><td>it <strong>names a class</strong> — a static access</td></tr>
    <tr><td><code>$x: @value$</code></td><td>it names a variable, parameter or field — an instance access</td></tr>
    <tr><td><code>$x: !equals$</code></td><td>the negation of any of the above</td></tr>
    <tr><td><code>$x: @type &amp; Files$</code></td><td>all of them at once</td></tr>
  </tbody>
</table>
<p>
  The <code>#</code> is what keeps the two vocabularies apart: a <em>type</em> is a word from
  Java, a <em>kind</em> is a word from the grammar (<code>string_literal</code>,
  <code>method_invocation</code>, <code>lambda_expression</code>). Open the
  <strong>Trees</strong> panel (<kbd>Alt</kbd> + <kbd>9</kbd>) and click a node on the
  <em>Syntax</em> tab to find out what a construct is called.
</p>
<p>
  <code>~</code> and the type globs are <strong>globs, not regexes</strong>: <code>*</code> stands
  for any run, <code>|</code> separates alternatives, everything else is literal, and the whole
  thing is anchored. <code>~place|cancel</code>, <code>~get*</code>, <code>*Service</code>.
</p>

<h3>Static or instance: <code>@type</code> and <code>@value</code></h3>
<p>
  <code>orders.total()</code> and <code>Orders.total()</code> are the <strong>same shape</strong>.
  The grammar reads both as a call whose receiver is an identifier, so no pattern can separate
  them — the difference is not in the syntax, it is in what the name <em>denotes</em>. That is a
  question for the resolver, and <code>@type</code> / <code>@value</code> is how you ask it.
</p>
<table class="doc-table">
  <thead><tr><th>Query</th><th>Finds</th></tr></thead>
  <tbody>
    <tr><td><code>$o: @type$.$m$($a...$)</code></td><td>every static call, anywhere</td></tr>
    <tr><td><code>$o: @type &amp; java.nio.file.Files$.$m$($a...$)</code><br /><code>group $m$</code></td><td>which <code>Files</code> statics this project uses, and how often each</td></tr>
    <tr><td><code>$o: @value &amp; Connection$.$m$($a...$)</code></td><td>calls on a JDBC connection, never on the class</td></tr>
  </tbody>
</table>
<p>
  A plain type constraint stays deliberately blind to the distinction: <code>$o: Order$</code>
  admits <code>order.f()</code> <em>and</em> <code>Order.f()</code>, because in both the type in
  play is <code>Order</code>. Add <code>@value</code> or <code>@type</code> when you mean one of
  them.
</p>
<p>
  <code>&amp;</code> joins constraints on one hole and binds <strong>looser</strong> than
  <code>!</code>, so <code>!~get* &amp; @value</code> reads as “not a getter, and a value”. There
  are no parentheses: a constraint that needed them is a query better written as two
  alternatives.
</p>
<p>
  A receiver the classpath cannot reach comes back <strong>undecided</strong> rather than as
  “not a type” — the same honesty the type constraints have, and for the same reason: turning an
  unknown into a no is how an incomplete classpath starts inventing answers.
</p>

<h2>Three clauses</h2>
<p>
  A query is a pattern, optionally followed by lines beginning with <code>or</code>,
  <code>in</code> or <code>group</code>. Java has no construct that begins with any of those
  words, so a line starting with one is never mistaken for code — <code>orders.total()</code> is
  still a pattern.
</p>

<h3><code>or</code> — the same question, another shape</h3>
<pre><code>$o$.$m$($args...$)
or $o$::$m$
group $m$</code></pre>
<p>
  Java spells a method use several ways, and a count that sees only one of them is simply wrong.
  Two rules make this safe:
</p>
<ul>
  <li>
    <strong>A capture used to group must be bound by every alternative.</strong> Otherwise the
    table would have rows with an empty column, and a hole in an aggregate reads as “none” rather
    than as “this branch cannot answer”. The query is refused when you write it, naming which
    branch is missing it.
  </li>
  <li>
    <strong>One place in a file is one hit.</strong> Two alternatives can describe the same
    bytes — <code>$o$.$m$()</code> and <code>$o$.close()</code> both match
    <code>stream.close()</code> — and counting that twice produces a number that is plausible and
    wrong, which is the worst kind. Hits are de-duplicated by range.
  </li>
</ul>

<h3><code>in</code> — where to look</h3>
<pre><code>in modules/core, modules/web</code></pre>
<p>Project-relative path prefixes, comma-separated. Without it, everywhere.</p>

<h3><code>group</code> — the statistics</h3>
<p>
  With a <code>group</code>, the answer stops being a list of places and becomes a
  <strong>table</strong>: the key, how many, and in how many files. Four keys, and deliberately
  not more — each is a question people actually arrive with.
</p>
<table class="doc-table">
  <thead><tr><th>Key</th><th>Answers</th></tr></thead>
  <tbody>
    <tr><td><code>group $m$</code></td><td>“which methods, and how often each”</td></tr>
    <tr><td><code>group file</code></td><td>“which files, and how many per file”</td></tr>
    <tr><td><code>group module</code></td><td>“which module is full of this”</td></tr>
    <tr><td><code>group enclosing</code></td><td>“which of <em>my</em> methods do this”</td></tr>
  </tbody>
</table>
<p>
  <code>enclosing</code> is the one that cannot be written any other way: the method a match sits
  inside is not part of the pattern, so there is nothing to capture — and yet “who in my code
  calls this” is the question asked most often.
</p>

<h2><code>use of</code> — every shape at once</h2>
<pre><code>use of $m$ on com.acme.OrderService+
group $m$</code></pre>
<p>
  This answers <em>which methods of a class are used, where, and how many times</em> without
  making you enumerate the ways Java spells a use. It is a shortcut for a set of patterns, and the
  panel <strong>shows you the expansion</strong> — so it teaches rather than hides, and you can
  copy it out, edit it and run it as an ordinary query.
</p>
<p>
  Name a member (<code>use of place on …</code>) to pin it; leave <code>$m$</code> to count all of
  them. The <code>+</code> includes uses through a subtype.
</p>
<p>
  <strong>What it covers, and what it does not.</strong> It finds uses <em>through a reference to
  the type</em>: a call on a receiver, and both forms of method reference. It does not find a
  class calling its own member (<code>this.place(o)</code>, a bare <code>place(o)</code>,
  <code>super.place(o)</code>) — those need the enclosing class's hierarchy, which a pattern
  cannot express. That is also usually what you want: “who uses OrderService” is a question about
  its consumers, and counting its own internals among them would inflate every answer.
</p>

<h2>Searching pages</h2>
<p>
  A legacy Struts codebase keeps as much of its logic in JSPs as in classes, and a text search is
  even weaker there than over Java — the same tag is written across four lines as often as one,
  so grepping <code>&lt;s:property value=</code> finds a fraction of them. Switch the picker to
  <strong>JSP</strong> and the pattern is markup:
</p>
<pre><code>&lt;s:property $pre...$ value="$x$" $post...$/&gt;
group $x$</code></pre>
<p>
  That counts every property the pages print, one row per name. A pattern needs no wrapper here —
  any run of tags and text is already a legal page — so what you type is what is parsed.
</p>
<p>
  <strong>Expressions have structure too.</strong> An EL or OGNL body is not one blob: a
  <em>path</em> — a name and what is read off it — is a subtree, and operators, literals and
  spacing are its siblings. So a hole can sit inside one.
</p>
<p>
  The catch is that a pattern still has to match the <em>whole</em> expression.
  <code>%&#123;#session.$prop$&#125;</code> finds only the expressions that are exactly that —
  not <code>%&#123;#session.user != null&#125;</code>, which has three more parts. Put a run on
  each side and it finds them all:
</p>
<pre><code>%&#123;$pre...$ #session.$prop$ $post...$&#125;
group $prop$</code></pre>
<p>
  Same idiom as the one for attributes, and for the same reason — the engine compares children,
  so anything you do not name has to be given somewhere to go.
</p>
<p>
  <code>$&#123;</code> in a pattern is EL and not a hole: a placeholder name cannot begin with a
  brace, so the <code>$</code> is read literally and needs no escaping.
  <code>$$</code> is still a literal <code>$</code> anywhere else.
</p>
<p>
  <strong>The two runs are the idiom, not clutter.</strong> A tag’s attributes are matched in
  order and in full, because the engine compares children and has no notion of a set. So
  <code>&lt;s:property value="$x$"/&gt;</code> alone finds only the tags whose one and only
  attribute is <code>value</code>; <code>$pre...$</code> and <code>$post...$</code> let the rest
  of them be there, in any order, which is how real pages are written. Every JSP template in the
  <strong>Templates</strong> menu is written that way.
</p>
<p>Three more things worth knowing before you discover them:</p>
<ul>
  <li>
    A <strong>self-closing tag and an open/close pair are different shapes</strong>.
    <code>&lt;s:property …/&gt;</code> does not match
    <code>&lt;s:property …&gt;&lt;/s:property&gt;</code>; write the one the page uses.
  </li>
  <li>
    <strong>Expressions are decomposed; scriptlets are not.</strong>
    <code>$&#123;…&#125;</code> / <code>#&#123;…&#125;</code> / <code>%&#123;…&#125;</code> have
    real structure inside, so <code>%&#123;#session.$prop$&#125;</code> is a pattern that works.
    The <code>&lt;% … %&gt;</code> family — scriptlets, directives, declarations — is still a
    <strong>single token</strong> each: <code>&lt;%@ taglib prefix="$p$" %&gt;</code> compiles
    and then matches nothing, because the hole is characters inside a leaf rather than a node.
    For the Java in the pages there is a language of its own — see below.
  </li>
  <li>
    <code>use of</code> and the <code>@type</code> / <code>@value</code> constraints ask about
    Java code. In a page the first is refused with a message rather than run, and the second has
    no resolver behind it, so it reports <em>undecided</em> instead of filtering.
  </li>
</ul>

<h2>The Java inside the pages</h2>
<p>
  The third setting of the picker — <strong>Java in JSP</strong> — is the answer to the limit
  above. The query is <strong>Java</strong>, the files walked are the pages, and what is matched
  is the contents of their <code>&lt;% … %&gt;</code>, <code>&lt;%= … %&gt;</code> and
  <code>&lt;%! … %&gt;</code> blocks.
</p>
<pre><code>session.getAttribute($key$)
group $key$</code></pre>
<p>
  That is every key the pages read out of the session, one row per key — a question a JSP query
  cannot ask at all, because to the page grammar a scriptlet is a single token. Nothing about the
  query language changes: it is an ordinary Java pattern, with the same holes, constraints and
  clauses, including <code>use of</code>.
</p>
<p>
  Each block is lifted out and wrapped in the smallest legal Java that makes its <em>kind</em>
  parse — a scriptlet is statements, a declaration is members, a <code>&lt;%= %&gt;</code> is an
  expression — then matched and mapped back onto the page. So a hit's line, its preview and where
  a click takes you are all the code as it is written, never the scaffolding it was parsed inside.
</p>
<p>
  Two things follow from the lifting and are worth knowing. <strong>Type constraints come back
  undecided</strong>: the resolver is asked about a file, and a scriptlet wrapped in a synthetic
  class is in no file — reporting <em>undecided</em> is this tool's word for "unknown", and it is
  the only honest one here. And <strong><code>group enclosing</code> has nothing to name</strong>:
  the block has no enclosing method, the page is the method. Use <code>group file</code>.
</p>

<h2>What it says when it does not know</h2>
<p>
  A type constraint needs the classpath, and on a legacy project the classpath is often
  incomplete. When Bennu cannot decide whether a receiver is the type you named, the hit is
  <strong>kept and marked undecided</strong> — never quietly dropped.
</p>
<p>
  This matters more than it sounds. A filter that silently excluded what it could not read would
  produce a table that <em>looks</em> complete and is short by however much the project failed to
  resolve: “this API is used 12 times” instead of “12 I could confirm, and 380 I could not read”.
  The table shows both numbers, and a row's undecided count in brackets beside its total.
</p>
<p>
  For the same reason, negating an unknown stays unknown: “not something I could not determine”
  is not a fact, and turning it into one is how an incomplete classpath starts inventing hits.
</p>

<h2>Replacing</h2>
<p>
  Turn on <strong>Replace</strong> and write the template — Java again, with the captures put
  back:
</p>
<pre><code>pattern:      $a$ == null ? null : $a$.$m$()
replacement:  Optional.ofNullable($a$).map(X::$m$)</code></pre>
<p>
  It is always <strong>two steps</strong>: <em>Preview</em> shows the before and after of every
  file it would touch, and <em>Apply</em> writes what the preview showed. Never one step — a
  structural replace rewrites places you did not look at, and the whole reason to prefer it over a
  textual one is that it is precise, which is only worth something if you can check.
</p>
<p>
  A template naming a capture the pattern does not bind is <strong>refused before any file is
  read</strong>, listing what the pattern does bind. Without that check it would render as an
  empty string: valid Java, wrong code, and no error anywhere.
</p>
<p>
  Between the preview and the apply, every file is re-read and compared against what it was.
  Anything that changed in between is <strong>refused by name</strong> rather than overwritten —
  the rewrite in hand was computed from bytes that are no longer there.
</p>

<h2>Why it is fast, and when it is not</h2>
<p>
  A pattern over five thousand files would be five thousand parses. But every useful pattern
  contains <strong>literals that must appear</strong> — <code>log.debug</code>,
  <code>SimpleDateFormat</code>, <code>createStatement</code> — so those are grepped for first and
  only the files that could match are parsed. On a typical query that is a tenfold cut.
</p>
<p>
  A pattern made only of holes (<code>$o$::$m$</code>) has nothing to grep for, and every file
  gets parsed. The field says so as you type — <em>whole-project scan</em> — and the line under
  the results says how many files were actually read. It is the only honest way to explain why one
  query answers instantly and another takes a few seconds.
</p>

<h2>Worked examples</h2>
<p>The panel offers these to start from; each uses a different part of the language.</p>

<h3>The census before a refactor</h3>
<pre><code>use of $m$ on com.acme.OrderService
group $m$</code></pre>
<p>Which methods are used, how often, across how many files.</p>

<h3>Which of my methods touch a deprecated API</h3>
<pre><code>new $x: java.text.SimpleDateFormat$($p...$)
group enclosing</code></pre>

<h3>Logging that was concatenated instead of parameterised</h3>
<pre><code>log.$lvl: ~debug|info|warn$("$s$" + $x$)
group $lvl$</code></pre>

<h3>A call and its method reference, counted together</h3>
<pre><code>$o$.place($a...$)
or $o$::place
group file</code></pre>

<h2>The panel</h2>
<p>
  The query runs across the top, the answer sits under it, and the selected match is shown
  <strong>in context</strong> beside it. A query is two or three lines of code with holes in it —
  never a document — so it gets a band rather than a column, and no line-number gutter: nobody
  refers to part of a query they can see all of by number.
</p>
<p>
  The answer takes one of two shapes and the <em>query</em> decides which: a <strong>table</strong>
  when it groups, a <strong>list of places</strong> when it does not. The context column follows
  the list; under the table it is absent, because a row there is a count and has no single place
  to show. Walking the results with ↑/↓ re-reads the file beside them, and nothing is opened until
  <kbd>Enter</kbd>.
</p>
<p>
  <strong>Templates</strong> is a menu on the query bar — each entry says what it is for, not just
  what it is called. <strong>Export</strong> takes the answer out as CSV, JSON or a Markdown table,
  to the clipboard or to a file; it exports whichever shape is on screen, so a grouped query gives
  you the table rather than the places it summarised. Undecided hits get a column of their own
  rather than being folded into the count.
</p>
<p>
  The line under the results says how long the scan took and how many files it actually parsed.
  Both are worth reading together: this is the one panel whose cost is variable <em>and</em>
  explainable — a query with a literal to grep for reads a tenth of the project, one made only of
  holes reads all of it.
</p>

<h2>The query field</h2>
<p>
  It is a real editor, not a text box. Placeholders, the clause words and a constraint are each
  coloured apart from the code around them — a hole that looks like the code it sits in is the one
  thing a query field must not do — and <kbd>Ctrl</kbd> + <kbd>Space</kbd> completes the five
  things nobody can be expected to remember:
</p>
<ul>
  <li>the <strong>clause words</strong>, at the start of a line;</li>
  <li>what <code>group</code> accepts — <code>file</code>, <code>module</code>,
    <code>enclosing</code>, <strong>and the captures this query binds</strong>, which is the one
    no manual could tell you;</li>
  <li>the <strong>node kinds</strong> after <code>#</code>, the grammar’s own vocabulary;</li>
  <li><code>@type</code> and <code>@value</code>, wherever a constraint can go;</li>
  <li>the <strong>types</strong> after a <code>:</code> or a <code>&amp;</code>, from the
    project’s class index — so <code>: Order</code> offers <code>com.acme.Order</code> rather
    than making you remember a package.</li>
</ul>
<p>
  The replacement field completes the query’s captures, and nothing else — those are the only
  names a template may use.
</p>

<h2>Keyboard</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> — open it.</li>
  <li><kbd>Ctrl</kbd> + <kbd>Enter</kbd> — run the query.</li>
  <li><kbd>Ctrl</kbd> + <kbd>Space</kbd> — completions.</li>
  <li><kbd>Esc</kbd> — close.</li>
  <li><kbd>Enter</kbd> on a result opens that place and closes the dialog.</li>
</ul>
<p>
  A <code>--</code> at the start of a line is a comment, so a query worth keeping can say what it
  is for.
</p>
