<script lang="ts">
  /** Picus docs — what the SQL editor knows while you type. */
</script>

<h1>The SQL editor</h1>
<p class="doc-lead">
  Query tabs and script files are not text boxes with colours. They complete, they explain,
  they warn before you run, and they finish what they can finish — using the schema the
  connection already reported, and nothing else.
</p>

<h2>Everything comes from the catalogue</h2>
<p>
  There is no language model anywhere in Picus. Every suggestion is read out of the database
  you are connected to or out of the text in front of you. That is a stronger guarantee than
  a prediction: after <code>INSERT INTO PARAMETRI (</code> the column list is not a guess, it
  is what the server said the table has. A suggestion is either right or absent — and when
  the tool is unsure, it stays quiet.
</p>

<h2>Completion</h2>
<p>
  Completion opens as you type an identifier, and on <kbd>Ctrl</kbd>+<kbd>Space</kbd> anywhere.
  What it offers depends on where the caret is:
</p>
<ul>
  <li>at the start of a statement — the words that can <i>open</i> one, and nothing else.
    No statement in SQL begins with a table name, so no table is offered there;</li>
  <li>where one word decides the next — after <code>INSERT</code>, after <code>GROUP</code>,
    after <code>DROP</code> — only the words that can follow it;</li>
  <li>after a qualifier — <code>c.</code> — the columns of the table that alias stands for,
    <i>only</i> those; <code>FROM CLIENTI c JOIN ORDINI o</code> keeps the two apart;</li>
  <li>in <code>FROM</code> and <code>JOIN</code> — tables, views and the statement's own
    <code>WITH</code> names; once a table reference is complete, the clauses that may follow
    it instead;</li>
  <li>inside <code>INSERT INTO t (…)</code> — that table's columns, minus the ones already
    listed;</li>
  <li>after <code>UPDATE t SET</code> — that table's columns;</li>
  <li>in <code>WHERE</code>, <code>ON</code> and <code>HAVING</code> — the columns in scope,
    the comparison words, and the dialect's functions;</li>
  <li>in <code>ORDER BY</code> — the columns, then <code>ASC</code> / <code>DESC</code>.</li>
</ul>
<p>
  Two entries in the list are worked out rather than looked up, and both are still facts.
  A column name carried by <b>more than one</b> table in scope is offered only in its
  qualified forms — <code>c.ID</code> and <code>o.ID</code>, never a bare <code>ID</code>
  the server would refuse as ambiguous. And in <code>FROM</code> / <code>JOIN</code>, a
  table reachable by a <b>foreign key</b> from what the statement already names is ranked
  above the rest and says which key put it there.
</p>
<p>
  Functions are completed as <code>NAME()</code> with the caret landing between the
  parentheses, and carry their <b>full signature</b> and a sentence saying what they do —
  which for <code>NVL2</code> and <code>MONTHS_BETWEEN</code>, where the argument order is the
  thing people get wrong, is the whole question. A value written without parentheses
  (<code>SYSDATE</code>, <code>CURRENT_DATE</code>) is completed without them, because adding
  them is a syntax error on Oracle. A sequence completes to the form its engine accepts —
  <code>SEQ.NEXTVAL</code> on Oracle, <code>nextval('seq')</code> on PostgreSQL.
</p>
<p>
  The function vocabulary is per engine, and deliberately not merged: <code>SUBSTR</code>
  takes a negative start on one engine and not the other, <code>greatest</code> skips NULLs on
  PostgreSQL and propagates them on Oracle, and <code>TO_CHAR</code> shares a name and not a
  format vocabulary. A merged list would need an exception on most rows.
</p>
<p>
  Keywords are per dialect, taken from the tab: a connection's engine for a query, the
  engine of the folder the file lives in for a script — inherited from wherever it was
  declared. There is no global "current dialect" to get wrong.
</p>

<h2>Pasting into a string</h2>
<p>
  Paste <code>L'Aquila</code> between the quotes of <code>nome = '…'</code> and the
  apostrophe is <b>doubled</b> for you. Without that you get a closed string, a stray word
  and a syntax error — found when you run it rather than when you paste it. The same holds
  for a <code>"</code> pasted inside a delimited identifier.
</p>
<p>
  It happens only where an unescaped quote could not have been meant, so everywhere else a
  paste arrives exactly as it left. Two places that look like strings are deliberately left
  alone: a dollar-quoted body (<code>$$ … $$</code>) and Oracle's
  <code>q'[…]'</code>. Both exist precisely so that what is inside them needs no escaping,
  and doubling a quote there would corrupt the value instead of protecting it.
</p>

<h2>Ghost text</h2>
<p>
  A greyed continuation appears at the caret when the next thing to write is certain.
  <kbd>Tab</kbd> accepts it, <kbd>Esc</kbd> dismisses it, and it never appears while the
  completion popup is open — that popup also wants <kbd>Tab</kbd>, and it is the more
  specific intent.
</p>
<ul>
  <li><code>INSERT INTO t</code> → the parenthesised column list;</li>
  <li><code>INSERT INTO t (</code> → the column names, in schema order;</li>
  <li><code>INSERT INTO t (A, B)</code> → the matching <code>VALUES</code> line, with one
    placeholder per column in the form the engine uses;</li>
  <li><code>JOIN ORDINI o ON</code> → the equality the foreign key between the two tables
    implies — offered only when exactly one foreign key connects them;</li>
  <li>a blank line inside an open block → its <code>END;</code>, <code>END IF;</code>,
    <code>END LOOP;</code> or <code>END CASE;</code>.</li>
</ul>
<p>
  Nothing beyond that is proposed. Which columns you want after <code>SELECT</code>, or which
  rows you meant to delete, is a decision — and a decision dressed up as a completion is
  worse than no completion.
</p>
<p>
  One proposal is a rewrite rather than a continuation: a line written in the abbreviation
  shorthand — <code>s#localstrings(keycode,value)[keycode='ita']</code> — previews the whole
  statement it stands for, and <kbd>Tab</kbd> puts that statement in its place. See
  <b>SQL abbreviations</b>.
</p>

<h2>Hover</h2>
<p>
  Resting the pointer on an identifier shows what the catalogue knows about it: a column's
  type, whether it accepts <code>NULL</code>, its default and the foreign key it points at; a
  table's kind, column count and row estimate; a sequence's last value and increment. An
  identifier that resolves to nothing known shows nothing.
</p>
<p>
  Resting it on one of the <b>engine's own functions</b> shows its signature, what it returns,
  a sentence, sometimes an example — and the trap, where there is one. That
  <code>MONTHS_BETWEEN</code> wants the later date first and returns a negative number
  silently otherwise; that Oracle's <code>ROWNUM</code> is assigned before the sort, which is
  why <code>ROWNUM &lt;= 10</code> with an <code>ORDER BY</code> does not give you the top ten;
  that PostgreSQL's <code>regexp_replace</code> replaces only the first match without the
  <code>'g'</code> flag, the opposite of what <code>replace()</code> does.
</p>
<p>
  This is the one card that does not need a connection — a function's meaning belongs to the
  engine, not to the database — so it works in a script file with nothing open. It is skipped
  for a qualified name: <code>t.COUNT</code> is a column called <code>COUNT</code>, not the
  aggregate.
</p>

<h2>What gets flagged while you type</h2>
<ul>
  <li><b>Whatever the database rejects</b> — an unknown table or view, an unknown column, an
    ambiguous one. A moment after you stop typing, each statement is <i>prepared</i> against
    the connected server — parsed and described, never run — and whatever it refuses is
    underlined where the server says the problem is. It is the server's own verdict, so it is
    exact and never out of date, and a mark on the toolbar says whether the last check passed,
    is running, or could not be made.</li>
  <li><b>A write on a read-only connection</b> — reported at the statement, before you run
    it. This one needs no round trip: the refusal is the server's, but the connection being
    read-only is already known here.</li>
  <li><b>Something that is not SQL</b> — the grammar could not read it. Procedural code
    outside a routine, an unclosed parenthesis, a missing keyword. This comes from the same
    parser the syntax tree is drawn from, so the editor and that panel can no longer disagree
    about whether a statement is readable.</li>
</ul>
<p>
  The first is the database's own answer about <i>meaning</i>; the last is about <i>form</i>,
  which the parser here settles without asking anyone. Between them, the editor no longer
  reimplements the catalogue — what a table has and what a column means is the server's to
  say, and it is asked directly.
</p>

<h2>Abbreviations are marked as abbreviations</h2>
<p>
  A line the backend recognises as a shorthand — <code>s#ordini(id)[stato='EV']</code> — is
  given a tinted band rather than being coloured as SQL, because it is not SQL: read as
  SQL it is a stray identifier, a comment marker and a broken parenthesis, so the one line
  in the buffer the tool understands best would look the most wrong. A shorthand the backend
  <i>refuses</i> gets the band in the warning colour, and the reason on the line.
</p>
<p>
  Which lines those are is the backend's own answer, never a guess made here — so what is
  highlighted and what will expand cannot drift apart. Nothing on such a line is measured as
  SQL: no unknown-table warnings, no parse errors.
</p>

<h2>When the database check says nothing, and why</h2>
<p>
  A warning that is wrong costs more than ten that are right, so the check against the server
  stands down — and says so on the toolbar — wherever it cannot ask:
</p>
<ul>
  <li><b>With no connection open</b>, or on an engine that cannot prepare a statement, there
    is nothing to ask: the toolbar shows the check as unavailable, and the parser and the
    read-only warning are all that remain.</li>
  <li><b>DDL, anonymous blocks, <code>SET</code> and transaction control are not prepared</b>
    — a server cannot prepare them — so they are left to the syntax parser rather than
    checked against the schema. A script whose job is to create a table is never told the
    table does not exist.</li>
  <li><b>A line written as an abbreviation is not measured as SQL.</b> It is a shorthand the
    tool understands; what it reports there is the shorthand's own refusal, when it has one.</li>
</ul>

<h2>Script files</h2>
<p>
  A file on disk has no connection of its own, so it borrows the active one — but only when
  the dialects agree. An Oracle script is never checked against a PostgreSQL database. With
  no match the editor still completes keywords, closes blocks and reports nothing about
  objects, which is the right behaviour for a script opened with no database open.
</p>

<h2>Find and replace</h2>
<p>
  <kbd>Ctrl</kbd>+<kbd>F</kbd> opens the panel — <kbd>Ctrl</kbd>+<kbd>H</kbd> opens the same
  one, because that is where the other half of the world's fingers go. It carries both fields:
  find, replace, replace all, case sensitivity, whole word and a regular-expression toggle.
  <kbd>F3</kbd> and <kbd>Shift</kbd>+<kbd>F3</kbd> walk the matches, <kbd>Enter</kbd> from the
  find field does the same, and <kbd>Esc</kbd> closes it and gives the caret back.
</p>
<p>
  It is the ordinary textual find. To match on the <i>shape</i> of a statement rather than on
  its characters — every <code>INSERT</code> into a table, whatever its values —
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> opens the structural search, which works
  across the whole repository.
</p>

<h2>Editing keys</h2>
<p>
  The editor is aimed at hands trained on an IDE, so the verbs those hands reach for are
  where they expect them:
</p>
<table>
  <thead><tr><th>Key</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd>+<kbd>D</kbd></td><td>Duplicate the selection, or the whole line when there is none</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Y</kbd></td><td>Delete the line</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>/</kbd></td><td>Comment or uncomment the selected lines</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd></td><td>Move the line — <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> does the same</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>J</kbd></td><td>Add the next occurrence of the selection as a second cursor</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Click</kbd></td><td>Put a cursor where you click, keeping the others</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Click</kbd></td><td>Open the structure of the object under the pointer</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd></td><td>Undo · redo</td></tr>
    <tr><td><kbd>Right-click</kbd></td><td>Cut · copy · paste, and <b>Generate</b></td></tr>
  </tbody>
</table>

<h2>Generate</h2>
<p>
  Right-clicking offers a skeleton for each of the six things a script creates — a
  table, a view, a sequence, a trigger, a function, a procedure — written at the caret
  <i>in the dialect of the buffer you are in</i>. That is the part worth having: the two
  engines disagree about nearly all of it. A PostgreSQL trigger cannot exist without a
  <code>RETURNS trigger</code> function, so the skeleton writes both; Oracle carries the
  body inline and ends with a <code>/</code>. A function returns with
  <code>RETURN</code> in one and <code>RETURNS</code> in the other.
</p>
<p>
  The names are deliberately obvious placeholders. A skeleton that came out with
  plausible names would let one survive to review; <code>NOME_TABELLA</code> does not.
</p>

<h2>Saving a script</h2>
<p>
  <kbd>Ctrl</kbd>+<kbd>S</kbd>, or <b>Save</b> on the toolbar, writes the file back
  <b>in its own encoding and line endings</b> — and then re-reads the repository and
  re-runs the checks, because every rule Picus has is about what is in the scripts.
</p>
<p>
  A character the file's declared encoding cannot represent <b>stops the save</b> and
  says so. It is never written as <code>?</code>. Silently mangling a
  <code>windows-1252</code> script is the failure this whole product exists to catch in
  other people's editors, and it would be indefensible here.
</p>
<p>
  Duplicating with a selection puts the copy immediately after it <i>and selects the copy</i>,
  so pressing it again duplicates again and typing replaces what you just made — which is what
  makes it the fastest way to build a list of similar values.
</p>

<h2>The syntax tree</h2>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Y</kbd>, or the braces icon on the right rail. It shows
  how the parser actually read the document in front of you, and it answers one question:
  <b>why did it read it that way?</b>
</p>
<ul>
  <li><b>Clicking a node selects its text</b> in the editor, and moving the caret opens the tree
    down to the node holding it. Both directions matter: one is "show me what this is", the
    other is "show me where that is".</li>
  <li><b>The punctuation is shown by default.</b> The commas and the keywords are noisy, and
    they are very often the answer — a statement that ends earlier than you expected usually
    ends at a character you did not notice. The filter button hides them for the reading where
    they are not the point.</li>
  <li><b>A field name is on the row where the grammar gives one</b>: it is the difference
    between "an identifier" and "the table being written to".</li>
  <li><b>It descends into a routine's body.</b> PostgreSQL hands a <code>$$ … $$</code> body back
    as a single string, which is exactly where an update script does its work — so the body is
    read separately and its statements appear in the tree, at their real positions in the file.
    A <code>$$ … $$</code> anywhere else is left as the string literal it is.</li>
  <li><b>A file that will not parse still has a tree</b>, with its error nodes and the tokens
    the parser had to invent to keep going — which is the reading this panel is most useful
    for, so it is not the one it refuses.</li>
  <li>A very large file is walked up to a budget and the panel says <b>truncated</b> rather
    than implying the file ends there.</li>
</ul>
<p>
  It follows the <i>buffer</i>, not the file on disk: the moment you want the tree is usually
  the moment you have just typed something that reads differently than you meant.
</p>
