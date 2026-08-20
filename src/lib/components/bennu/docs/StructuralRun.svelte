<!-- Bennu docs — running a structural search: the panel, the rewrite, the limits. -->
<h1>Searching &amp; replacing</h1>
<p class="doc-lead">
  Where the results land, how a rewrite is applied, and what the panel says when the pattern is
  beyond what it can answer.
</p>

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
