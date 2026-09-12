<script lang="ts">
  /**
   * Running a structural search: the panel, the query field and its completion, the keyboard, replacing in two steps,
   * undecided hits, why it is fast, and worked examples.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Structural search</span>
<h1>Searching &amp; replacing</h1>

<p class="doc-lead">
  Where the results land, how a rewrite is applied, and what the panel says when the pattern is beyond what it can answer.
</p>

<h2>The panel</h2>
<p>
  The query runs across the top, the answer sits under it, and the selected match is shown <strong>in context</strong> beside it. A query is two or three
  lines with holes — never a document — so it gets a band rather than a column, and no line numbers.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">A query with <code>group</code></div>
    <div class="fc-title">A table</div>
    <div class="fc-desc">A row is a count, with no single place to show, so there is no context column. Undecided hits get a column of their own.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A query without</div>
    <div class="fc-title">A list of places</div>
    <div class="fc-desc"><kbd>↑</kbd>/<kbd>↓</kbd> read the file beside them; nothing opens until <kbd>Enter</kbd>.</div>
  </div>
</div>
<dl class="meta-grid">
  <dt>Templates</dt>
  <dd>A menu on the query bar; each entry says what it is for, not just what it is called.</dd>
  <dt>Export</dt>
  <dd>CSV, JSON or a Markdown table, to the clipboard or a file — whichever shape is on screen, so a grouped query exports the table, not the places it summarised.</dd>
  <dt>The line under the results</dt>
  <dd>How long the scan took and how many files it parsed. Read them together: a query with a literal to grep for reads a tenth of the project, one made only of holes reads all of it.</dd>
</dl>

<h2>The query field</h2>
<p>
  A real editor, not a text box. Placeholders, clause words and constraints are each coloured apart from the code around them — a hole looking like its code
  is the one thing a query field must not do — and <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd> completes the five things nobody can remember:
</p>
<ul>
  <li>the <strong>clause words</strong>, at the start of a line;</li>
  <li>what <code>group</code> accepts — <code>file</code>, <code>module</code>, <code>enclosing</code>, <strong>and the captures this query binds</strong>,
    which no manual could tell you;</li>
  <li>the <strong>node kinds</strong> after <code>#</code>, the grammar's own vocabulary;</li>
  <li><code>@type</code> and <code>@value</code>, wherever a constraint can go;</li>
  <li>the <strong>types</strong> after <code>:</code> or <code>&amp;</code>, from the class index — <code>: Order</code> offers <code>com.acme.Order</code>.</li>
</ul>
<p>The replacement field completes the query's captures and nothing else — the only names a template may use.</p>
<p>A <code>--</code> at the start of a line is a comment, so a query worth keeping can say what it is for.</p>

<h2>Keyboard</h2>
<table>
  <thead><tr><th>Key</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd></td><td>Opens it</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Enter</kbd></td><td>Runs the query</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd>, or <kbd>Ctrl</kbd> + <kbd>Space</kbd></td><td>Completions</td></tr>
    <tr><td><kbd>Enter</kbd> on a result</td><td>Opens that place and closes the dialog</td></tr>
    <tr><td><kbd>Esc</kbd></td><td>Closes</td></tr>
  </tbody>
</table>

<h2>Replacing</h2>
<p>Turn on <strong>Replace</strong> and write the template — Java again, with the captures put back:</p>
<pre><code>pattern:      $a$ == null ? null : $a$.$m$()
replacement:  Optional.ofNullable($a$).map(X::$m$)</code></pre>
<ol class="step-list">
  <li><em>Preview</em> shows the before and after of every file the rewrite would touch.</li>
  <li><em>Apply</em> writes what the preview showed.</li>
</ol>
<Callout variant="info" title="Always two steps">
  A structural replace rewrites places you did not look at, and its whole advantage over a textual one is precision — worth something only if you can check.
</Callout>
<dl class="meta-grid">
  <dt>A capture the pattern does not bind</dt>
  <dd>Refused <strong>before any file is read</strong>, listing what the pattern does bind. Otherwise it would render as an empty string: valid Java, wrong code, no error anywhere.</dd>
  <dt>A file changed since the preview</dt>
  <dd>Every file is read again and compared before writing; one that changed is <strong>refused by name</strong>, since the rewrite in hand was computed from bytes that are no longer there.</dd>
</dl>

<h2>What it says when it does not know</h2>
<p>
  A type constraint needs the classpath, and a legacy project's is often incomplete. When Bennu cannot decide whether a receiver is the type you named, the hit is
  <strong>kept and marked undecided</strong> — never quietly dropped.
</p>
<p>
  A filter silently excluding what it could not read would produce a table that <em>looks</em> complete and is short by however much failed to resolve: "this API
  is used 12 times" instead of "12 I could confirm, and 380 I could not read". The table shows both — a row's undecided count in brackets beside its total — and
  negating an unknown stays unknown.
</p>

<h2>Why it is fast, and when it is not</h2>
<p>
  A pattern over five thousand files would be five thousand parses. But every useful pattern holds <strong>literals that must appear</strong> —
  <code>log.debug</code>, <code>SimpleDateFormat</code>, <code>createStatement</code> — so those are grepped for first and only files that could match are parsed:
  on a typical query, a tenfold cut.
</p>
<Callout variant="warning" title="A pattern made only of holes">
  <code>$o$::$m$</code> has nothing to grep for, so every file is parsed. The field says so as you type — <em>whole-project scan</em> — and the line under the
  results says how many files were read: the honest way to explain why one query is instant and another takes seconds.
</Callout>

<h2>Worked examples</h2>
<p>The panel offers these to start from; each uses a different part of the language.</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow"><code>use of</code> · <code>group</code></div>
    <div class="fc-title">The census before a refactor</div>
    <div class="fc-desc"><code>use of $m$ on com.acme.OrderService</code><br /><code>group $m$</code><br />Which methods are used, how often, across how many files.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A type constraint · <code>group enclosing</code></div>
    <div class="fc-title">Which of my methods touch a deprecated API</div>
    <div class="fc-desc"><code>new $x: java.text.SimpleDateFormat$($p...$)</code><br /><code>group enclosing</code></div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A text glob</div>
    <div class="fc-title">Logging concatenated instead of parameterised</div>
    <div class="fc-desc"><code>log.$lvl: ~debug|info|warn$("$s$" + $x$)</code><br /><code>group $lvl$</code></div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow"><code>or</code> · <code>group file</code></div>
    <div class="fc-title">A call and its method reference, together</div>
    <div class="fc-desc"><code>$o$.place($a...$)</code><br /><code>or $o$::place</code><br /><code>group file</code></div>
  </div>
</div>
