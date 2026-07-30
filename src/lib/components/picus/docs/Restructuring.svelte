<h1>Structural search and replace</h1>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd>. A pattern is <b>the statement itself with holes in
  it</b> — there is no second syntax to learn, and somebody who can write the statement can write
  the pattern for it. It is matched against the <i>parsed</i> repository, not against the text, so
  line breaks, extra whitespace and a comment in the middle of a statement change nothing.
</p>

<h2>The two holes</h2>
<table>
  <thead><tr><th>Written</th><th>Matches</th></tr></thead>
  <tbody>
    <tr><td><code>$name$</code></td><td>exactly one node</td></tr>
    <tr><td><code>$name...$</code></td><td>a run of consecutive siblings, possibly none</td></tr>
    <tr><td><code>$$</code></td><td>a literal dollar sign — needed for a <code>$$ … $$</code> body</td></tr>
  </tbody>
</table>
<p>
  A list capture comes back as <b>the original bytes</b>, separators included: <code>COD, VAL</code>
  is what the file said, not what Picus guessed a separator should be.
</p>
<p>
  A placeholder stands in for a name while the pattern is parsed, so it can go anywhere a name can
  — a table, a column, a value, an argument. It cannot stand for a whole statement, because no
  grammar accepts a name there.
</p>

<h2>Half of it is a query</h2>
<p>
  <b>Leave the replacement empty.</b> The results are a table with one row per match and
  <b>one column per placeholder</b>, so
</p>
<pre><code>INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)</code></pre>
<p>
  run over four hundred scripts gives back every row those scripts install, with its columns and its
  values in their own columns. That is a use of its own, and it is why the table exports — as CSV
  for a spreadsheet, JSON for a script, or a Markdown table for a ticket, to the clipboard or to a
  file.
</p>

<h2>Finding the odd ones out</h2>
<p>
  <b>Compare</b>, above the results, groups the matches by one placeholder and lists the distinct
  values it caught, <b>commonest first</b>. That ordering is the answer: the top row is what the
  repository does, everything under it is what somebody did once.
</p>
<p>
  The reading it exists for. Every translation row is written by an <code>INSERT</code> whose column
  list ought to be the same everywhere; search
</p>
<pre><code>insert into localstrings ($chiavi...$) values ($valori...$)</code></pre>
<p>
  then <b>Compare</b> on <code>$chiavi$</code>. What comes back is every column order in use:
</p>
<pre><code>keycode, langcode, stringvalue     9 812   in 27 files
langcode, keycode, stringvalue        11   in 2 files    ← the deviation</code></pre>
<p>
  Eleven statements out of ten thousand put the language before the key, which means their values
  are in that order too — and every one of them installs the string under the wrong key. Clicking
  the group narrows the table to exactly those eleven, so they can be read, opened at their line,
  and exported before anybody decides what to do about them.
</p>
<p>
  Deciding is the point. <b>Seeing the conflict is not the same as rewriting it</b>, and for a
  repository this is usually the more valuable half: eleven deviations may be eleven mistakes, or
  they may be the one folder that has always been written that way for a reason. Picus shows you
  which rows disagree and leaves the judgement where it belongs. Whitespace and casing are ignored
  when comparing, so a list that differs only in formatting is not reported as a conflict.
</p>

<h2>Addressing a value by its column</h2>
<p>
  A position is a poor address exactly where it matters. If some statements list their columns in
  one order and some in another, <code>$vals.0$</code> means a different thing in each — which is
  the bug, not the fix. So a list can be addressed <b>through a parallel one</b>:
</p>
<pre><code>$vals[cols=keycode]$</code></pre>
<p>
  reads as <b>"the element of <code>vals</code> at the index where <code>cols</code> is
  <code>keycode</code>"</b>. Which turns the whole problem of column order into one template that
  does not care about it:
</p>
<pre><code>pattern:      insert into localstrings ($cols...$) values ($vals...$)
replacement:  INSERT INTO LOCALSTRINGS (KEYCODE, LANGCODE, STRINGVALUE)
              VALUES ($vals[cols=keycode]$, $vals[cols=langcode]$, $vals[cols=stringvalue]$)</code></pre>
<p>
  Every statement comes out in one shape whatever shape it went in as — the eleven deviations and
  the nine thousand others alike, in a single pass.
</p>
<ul>
  <li><b>Which list holds the name is named, never guessed.</b> A shorthand that picked a list would
    be a rewriting tool guessing which column a value belongs to, and this one writes into a
    database.</li>
  <li><b>Lists of different lengths are refused.</b> Three columns addressed and two present is a
    statement reported on its own row, not a value quietly written into the wrong column.</li>
  <li><b>A statement that does not have the column is reported</b>, naming the columns it does have
    — which is usually the more interesting finding.</li>
  <li>Names are matched ignoring case, because SQL folds them.</li>
</ul>

<h2>Rewriting</h2>
<p>
  The replacement is source text again, read the other way. <code>$name$</code> writes back what
  that placeholder captured, byte for byte, so values keep their quoting and their casing.
  <code>$name.0$</code> addresses one element of a list — which is what makes reordering
  expressible:
</p>
<pre><code>pattern:      INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)
replacement:  INSERT INTO CATALOGO_WIDGET_V2 (ETICHETTA, CHIAVE) VALUES ($vals.1$, $vals.0$)</code></pre>
<p>
  Indices count <b>elements</b>, not separators, so you count what you can see. Every match carries
  what it would become <i>before</i> a preview is asked for: a template that addresses more elements
  than a particular match caught says so on that row, and a rewrite is refused until every row can
  be written.
</p>

<h2>Scope, and the guarantees</h2>
<p>
  A transformation is narrowed by folder, engine and role before it is run. A <b>portable</b> script
  belongs to both engines, so it stays in scope when one is named.
</p>
<ul>
  <li><b>Nothing is written that has not been seen.</b> Compute the preview, read the diffs, then
    rewrite — the same two steps as a generation, over the same scripts.</li>
  <li><b>A file that moved stops the write.</b> Each previewed file carries a digest of what was on
    disk when it was computed, and the write refuses by name if any of them changed.</li>
  <li><b>Encodings and line endings survive.</b> The rewrite goes through the same writer the
    generator uses: a file that cannot be written back byte for byte is refused rather than
    converted.</li>
  <li><b>Matches never nest.</b> A replacement rewrites the matched range whole, so an inner match
    inside an outer one would be an edit inside an edit.</li>
  <li><b>Keyword casing does not have to agree.</b> These repositories are not consistent about it,
    and a pattern that only matched the casing it was typed in would miss half of them. Names are
    compared as written.</li>
</ul>
<p>
  A file that matched but could not be prepared is listed with its reason rather than dropped: a
  migration missing a file is worse than one that says which file it cannot do.
</p>

<h2>The same patterns, in the tab you have open</h2>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> opens the same thing beside the editor, scoped
  to the document in front of you. It is the half you reach for while writing the statement —
  forty inserts pasted out of a ticket that all need one more column — rather than the one you
  refine over an afternoon.
</p>
<ul>
  <li><b>It finds as you type</b>, in the pattern and in the document alike. The count in the
    header is always about the text on screen.</li>
  <li><b>Replace all is one edit</b>, so <kbd>Ctrl</kbd>+<kbd>Z</kbd> takes the whole thing
    back. That is why it needs no preview and no confirmation: it is an ordinary edit in your
    own buffer, and nothing is on disk until you save.</li>
  <li><b>A row rewrites on its own</b>, for the pattern that is right for thirty-nine of forty.
    Click a row to select it in the editor.</li>
  <li><b>A match the template cannot be applied to is marked and left alone</b>, and the rest
    are still rewritten — the failing rows are right there next to the button.</li>
  <li><b>Nothing is applied to a buffer that moved.</b> If the document changed since the
    matches were found, they are looked for again and nothing is replaced. The offsets would
    otherwise splice into the wrong statements.</li>
</ul>
<p>
  <kbd>Tab</kbd> moves between the two boxes, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> replaces
  everything and <kbd>Esc</kbd> leaves the panel — the whole flow without the mouse.
</p>
