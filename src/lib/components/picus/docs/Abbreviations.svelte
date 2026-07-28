<script lang="ts">
  /** Picus docs — the SQL abbreviation shorthand, and why it needs the schema. */
</script>

<h1>SQL abbreviations</h1>
<p class="doc-lead">
  A dense one-liner that stands for a whole statement. Type it in a query, and the SQL it
  means appears greyed out after the caret; <kbd>Tab</kbd> puts the statement in its place.
</p>

<pre><code>s#localstrings(keycode,value)[keycode='ita']
  → SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'</code></pre>

<h2>It is not a snippet</h2>
<p>
  Every editor has snippets, and none of them can do the two things this does, because both
  of them need the database. The <b>column's type</b> decides where the quotes go, and the
  <b>foreign key</b> decides what a join is <code>ON</code>. Those are the reasons the feature
  exists; the saved keystrokes are a side effect.
</p>

<h2>The shape</h2>
<pre><code>&lt;verb&gt;#&lt;table&gt; &gt;&lt;joined table&gt; (columns) [conditions] *rows</code></pre>
<table>
  <thead>
    <tr><th>Verb</th><th>Means</th><th><code>(…)</code></th><th><code>[…]</code></th><th>Also</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><code>s</code></td><td>SELECT</td>
      <td>column names — every column when left out</td>
      <td>optional, any operator</td>
      <td><code>&gt;</code> joins</td>
    </tr>
    <tr>
      <td><code>i</code></td><td>INSERT</td>
      <td>names, each with an optional <code>=value</code></td>
      <td>—</td>
      <td><code>*n</code> repeats the row</td>
    </tr>
    <tr>
      <td><code>u</code></td><td>UPDATE</td>
      <td><code>name=value</code>, required</td>
      <td>required</td>
      <td>—</td>
    </tr>
    <tr>
      <td><code>d</code></td><td>DELETE</td>
      <td>—</td>
      <td>required</td>
      <td>—</td>
    </tr>
  </tbody>
</table>
<p>
  Operators inside <code>[…]</code> are <code>=</code>, <code>&lt;&gt;</code>,
  <code>&lt;</code>, <code>&lt;=</code>, <code>&gt;</code>, <code>&gt;=</code>, and
  <code>~</code> for <code>LIKE</code>. There is deliberately no spelling for an
  <code>UPDATE</code> or a <code>DELETE</code> over every row: four characters is far too few
  to have typed to mean that, and the way to write that statement is to write it.
</p>

<h2><code>&gt;</code> — a join read out of the foreign key</h2>
<p>
  <code>s#ordini&gt;clienti</code> does not guess the condition and does not match names. It
  reads the referential constraint between the two tables and joins on exactly what the
  constraint says, in either direction, generating aliases from the tables' initials.
</p>
<pre><code>s#ordini&gt;clienti(ragione_sociale,totale)[totale&gt;1000]
  → SELECT C.RAGIONE_SOCIALE, O.TOTALE
      FROM ORDINI O JOIN CLIENTI C ON O.ID_CLIENTE = C.ID_CLIENTE
     WHERE O.TOTALE &gt; 1000</code></pre>
<p>
  When two foreign keys connect the same pair of tables, it <b>refuses</b> and names the
  candidates rather than picking one — a query that runs, returns rows and means something
  else is the most expensive kind of wrong. Say which with
  <code>&gt;clienti:id_cliente_fatt</code>. When no foreign key connects them, it refuses and
  says so; there is no <code>1=1</code> anywhere in this feature.
</p>
<p>
  Each link joins the one before it: <code>a&gt;b&gt;c</code> joins <code>c</code> to
  <code>b</code>, never back to <code>a</code>.
</p>

<h2>Quotes come from the column's type</h2>
<p>
  A bare value is quoted, or not, according to what the server said the column is —
  <code>007</code> in a text account code keeps its leading zeros, <code>15</code> in a
  numeric column does not gain quotes, and a column of a type Picus does not recognise is
  quoted, because that is the answer that fails safely.
</p>
<ul>
  <li><b>Quote it yourself and it stays quoted.</b> Quoting is a statement of intent and
    outranks the type in both directions.</li>
  <li><b>Keywords are never quoted</b> — <code>NULL</code>, <code>SYSDATE</code>,
    <code>NOW()</code>, <code>CURRENT_TIMESTAMP</code> and the rest of a closed list. A
    function that is not on that list is treated as text against a text column, which is
    visible in the preview before it is anywhere else.</li>
  <li>A bare value may contain balanced parentheses, so <code>now()</code> survives inside
    <code>(…)</code> without closing the column list.</li>
</ul>
<p>
  <code>i#</code> and <code>u#</code> with a complete set of values are written by the same
  generator that writes the destination scripts, so an abbreviation and a generated statement
  spell an Oracle or a PostgreSQL literal identically — there is one answer to that question
  in the product, not two.
</p>

<h2>Typing one</h2>
<ul>
  <li>The greyed preview appears once the caret settles at the end of the line, and
    <kbd>Tab</kbd> replaces the abbreviation with the statement. <kbd>Esc</kbd> dismisses the
    preview and leaves the line alone.</li>
  <li>Completion works <i>inside</i> the shorthand: tables after <code>#</code> and after
    <code>&gt;</code> — the related ones first — columns inside <code>(…)</code> and
    <code>[…]</code>, the distinguishing column after <code>&gt;table:</code>, and the
    operators. Values are never suggested: what to compare a column to is your data.</li>
  <li>The SQL warnings stand down on a line that is an abbreviation. It is not SQL, and
    measuring it as SQL would bury it in squiggles.</li>
</ul>

<h2>When it refuses</h2>
<p>
  A refusal is an answer, not a failure, and it is reported on the line while you type. The
  language would rather say why than produce something plausible:
</p>
<ul>
  <li>a table or a column that is not on this connection — with the nearest name, when
    exactly one is near enough to be worth naming;</li>
  <li>two foreign keys where a join needs one, or none at all;</li>
  <li>a column that exists in two of the tables in the chain;</li>
  <li>an <code>UPDATE</code> whose conditions are not equalities — Picus writes an update
    keyed by equality, so that one is about this product rather than about SQL.</li>
</ul>

<h2>Where it works</h2>
<p>
  Wherever the editor has a catalogue: a query tab on a connection, and a script file whose
  dialect matches the active connection. Without one there is no type to decide a quote and
  no constraint to decide a join, so the shorthand is not offered at all rather than offered
  as a guess. If the object tree has never been opened for a connection, the abbreviation
  says so instead of expanding.
</p>
