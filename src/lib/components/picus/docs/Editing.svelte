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
  <li>after a qualifier — <code>c.</code> — the columns of the table that alias stands for,
    <i>only</i> those; <code>FROM CLIENTI c JOIN ORDINI o</code> keeps the two apart;</li>
  <li>in <code>FROM</code> and <code>JOIN</code> — tables, views and the statement's own
    <code>WITH</code> names;</li>
  <li>inside <code>INSERT INTO t (…)</code> — that table's columns, minus the ones already
    listed;</li>
  <li>after <code>UPDATE t SET</code> — that table's columns;</li>
  <li>anywhere else — the columns in scope first, then tables, views, sequences and the
    dialect's keywords.</li>
</ul>
<p>
  Keywords are per dialect, taken from the tab: a connection's engine for a query, the
  engine of the folder the file lives in for a script — inherited from wherever it was
  declared. There is no global "current dialect" to get wrong.
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

<h2>Hover</h2>
<p>
  Resting the pointer on an identifier shows what the catalogue knows about it: a column's
  type, whether it accepts <code>NULL</code>, its default and the foreign key it points at; a
  table's kind, column count and row estimate; a sequence's last value and increment. An
  identifier that resolves to nothing known shows nothing.
</p>

<h2>What gets flagged while you type</h2>
<ul>
  <li><b>Unknown table or view</b> — the name is not in this connection's schema.</li>
  <li><b>Unknown column</b> — a qualified reference such as <code>c.NOEM</code> where the
    table has no such column.</li>
  <li><b>Ambiguous column</b> — a bare name that exists in two of the joined tables, which
    the server would refuse to resolve.</li>
  <li><b>A write on a read-only connection</b> — reported at the statement, before you run
    it. The refusal itself is still the server's.</li>
</ul>

<h2>When it says nothing, and why</h2>
<p>
  A warning that is wrong costs more than ten that are right, so the analysis stands down
  wherever it cannot be sure:
</p>
<ul>
  <li><b>Before the schema has been read</b>, nothing about objects is reported at all. An
    unread catalogue is not an empty one.</li>
  <li><b>A different schema</b> — <code>ALTRO.CLIENTI</code> when the session is pinned
    elsewhere — is skipped: there is no catalogue for it.</li>
  <li><b>DDL is never measured against the live schema</b>, and anything the file creates
    earlier counts as existing. A script whose job is to create a table must not be told the
    table does not exist.</li>
  <li><b>Inside a procedural block</b>, and inside a derived table or a <code>WITH</code>
    body, columns are not checked — their shape needs a full parse.</li>
  <li><b>Bare names are never called unknown</b>, only ambiguous: a lone word can be an
    output alias, a function or a variable.</li>
</ul>

<h2>Script files</h2>
<p>
  A file on disk has no connection of its own, so it borrows the active one — but only when
  the dialects agree. An Oracle script is never checked against a PostgreSQL database. With
  no match the editor still completes keywords, closes blocks and reports nothing about
  objects, which is the right behaviour for a script opened with no database open.
</p>
