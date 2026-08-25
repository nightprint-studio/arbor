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
<pre><code>&lt;verb&gt;#&lt;table&gt; &gt;&lt;joined&gt; +add:type ~retype:type (columns) [conditions] *n &lbrace;row template&rbrace;</code></pre>
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
      <td><code>*n</code> rows, <code>&lbrace;…&rbrace;</code> per-row values</td>
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
    <tr>
      <td><code>m</code></td><td>upsert</td>
      <td>columns to write — all of them when left out</td>
      <td><b>the key columns</b>, no operators</td>
      <td>—</td>
    </tr>
    <tr>
      <td><code>a</code></td><td>ALTER TABLE</td>
      <td>—</td><td>—</td>
      <td><code>+name:type</code> adds, <code>~name:type</code> retypes</td>
    </tr>
    <tr>
      <td><code>fc</code></td><td>cursor loop</td>
      <td>as <code>s</code></td>
      <td>as <code>s</code></td>
      <td><code>&gt;</code> joins</td>
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
<p>
  The verb also takes its whole word — <code>select#…</code>, <code>merge#…</code>,
  <code>upsert#…</code> — because nobody trusts a one-letter language on the first day. The
  letter is what the language is; the word is there so you never have to look it up.
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

<h2><code>*n</code> and <code>&lbrace;…&rbrace;</code> — rows that differ</h2>
<p>
  <code>*3</code> on its own writes the same row three times, which is what somebody seeding
  data types before editing the three apart. A <code>&lbrace;…&rbrace;</code> template is what makes them
  differ: one value per column, with <code>$</code> standing for the row's number.
</p>
<pre><code>i#ordini(id,codice)*3&lbrace;$, 'COD_$'&rbrace;
  → INSERT INTO ORDINI (ID, CODICE)
    VALUES (1, 'COD_1'), (2, 'COD_2'), (3, 'COD_3');</code></pre>
<p>
  The numbering is Emmet's, because that is where the <code>*3&lbrace;…&rbrace;</code> shape comes from:
</p>
<table>
  <thead><tr><th>Written</th><th>Row 1 of 3</th><th>Row 2</th><th>Row 3</th></tr></thead>
  <tbody>
    <tr><td><code>$</code></td><td>1</td><td>2</td><td>3</td></tr>
    <tr><td><code>$$$</code></td><td>001</td><td>002</td><td>003</td></tr>
    <tr><td><code>$@5</code></td><td>5</td><td>6</td><td>7</td></tr>
    <tr><td><code>$@-</code></td><td>3</td><td>2</td><td>1</td></tr>
    <tr><td><code>\$</code></td><td colspan="3">a literal <code>$</code> — which Oracle-era
      names and ordinary text both contain</td></tr>
  </tbody>
</table>
<p>
  A template whose length does not match the column list is <b>refused</b>, naming the columns.
  The alternative is valid SQL with every value one column to the left, which is the worst
  thing this language could produce.
</p>
<p>
  On PostgreSQL the rows come out as one statement with a tuple each; on Oracle as one
  statement per row, because Oracle has no multi-row <code>VALUES</code>. Same abbreviation,
  two correct spellings.
</p>

<h2><code>m#</code> — insert it, or update it if it is there</h2>
<p>
  Nine characters for the statement nobody enjoys writing. The brackets are the one place in
  this language where <code>[…]</code> is <b>not</b> a <code>WHERE</code>: they name the
  columns that decide whether the row already exists.
</p>
<pre><code>m#ordini[id]
  → -- PostgreSQL
    INSERT INTO ORDINI (ID, STATO, IMPORTO)
    VALUES (:ID, :STATO, :IMPORTO)
    ON CONFLICT (ID) DO UPDATE SET
          STATO = EXCLUDED.STATO,
          IMPORTO = EXCLUDED.IMPORTO;

    -- Oracle
    MERGE INTO ORDINI d
    USING (SELECT :ID AS ID, :STATO AS STATO, :IMPORTO AS IMPORTO FROM dual) s
       ON (d.ID = s.ID)
     WHEN MATCHED THEN UPDATE SET
          d.STATO = s.STATO,
          d.IMPORTO = s.IMPORTO
     WHEN NOT MATCHED THEN INSERT (ID, STATO, IMPORTO)
          VALUES (s.ID, s.STATO, s.IMPORTO);</code></pre>
<ul>
  <li>The columns are the whole table unless you name some: <code>m#ordini(stato)[id]</code>.
    A key column you left out is added back — a merge that did not insert its own key would
    write a row it could never match again.</li>
  <li>The key is never in the <code>SET</code>: it is what the statement matched on.</li>
  <li>A key covering every column is refused. There would be nothing to update, which makes
    it an <code>INSERT</code> with extra words.</li>
  <li>Writing an operator (<code>m#ordini[id=1]</code>) is refused rather than reinterpreted.</li>
  <li>A <b>portable</b> folder is refused: the two engines share no spelling for this, and
    the refusal names both so you can pick one.</li>
</ul>

<h2><code>a#</code> — add a column, or change its type</h2>
<p>
  <code>+</code> adds, <code>~</code> retypes, and as many of each as you like. The type is
  written once, portably, and comes out in the engine's own spelling.
</p>
<pre><code>a#ordini+nota:varchar(200)
  → -- PostgreSQL
    ALTER TABLE ORDINI ADD COLUMN NOTA varchar(200);
    -- Oracle
    ALTER TABLE ORDINI ADD (NOTA VARCHAR2(200));

a#ordini~importo:number(12,2)
  → -- PostgreSQL
    ALTER TABLE ORDINI ALTER COLUMN IMPORTO TYPE numeric(12,2);
    -- Oracle
    ALTER TABLE ORDINI MODIFY (IMPORTO NUMBER(12,2));</code></pre>
<p>
  <code>varchar</code>/<code>varchar2</code>, <code>number</code>/<code>numeric</code>,
  <code>int</code>, <code>bigint</code>, <code>boolean</code>, <code>text</code>/<code>clob</code>,
  <code>blob</code>/<code>bytea</code>, <code>date</code>, <code>timestamp</code> and a few
  more are translated; anything else is written through as typed, because a user who spelled
  it the engine's way was already right. Two of them gain arguments they were not written
  with — <code>int</code> is <code>NUMBER(10)</code> on Oracle and <code>boolean</code> is
  <code>NUMBER(1)</code>, because the unsized forms mean something else there. A type may be
  several words: <code>+creato:timestamp with time zone</code>.
</p>
<p>
  The schema is consulted in <b>opposite directions</b> for the two, which is the whole value
  of doing this against a live connection: adding a column that is already there is refused
  and points at <code>~</code>, and retyping one that does not exist is refused with the
  nearest name.
</p>

<h2><code>fc#</code> — a loop over a query</h2>
<p>
  The block that starts every procedural script, with the query already written.
</p>
<pre><code>fc#ordini[stato='EV']
  → FOR r IN SELECT * FROM ORDINI WHERE STATO = 'EV' LOOP
      NULL; -- TODO
    END LOOP;</code></pre>
<p>
  It takes everything <code>s#</code> takes, joins included — <code>fc#ordini&gt;clienti(nome)</code>
  loops over the join. Oracle gets the query in parentheses and PostgreSQL does not, because
  PL/pgSQL rejects them; that is the only difference between the two.
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
  <li>While the completion list is open it owns <kbd>Tab</kbd>, so the preview waits.
    The list closes on its own once what you have typed is a complete name with nothing
    longer to reach — so typing a name to the end hands <kbd>Tab</kbd> straight back to
    the expansion.</li>
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
    keyed by equality, so that one is about this product rather than about SQL;</li>
  <li>a <code>&lbrace;…&rbrace;</code> that does not line up with the columns, or that gives a value a
    second time after an <code>=</code> already did;</li>
  <li>a merge with no key, or one whose key covers everything;</li>
  <li>a column added that is already there, or retyped when it is not.</li>
</ul>
<p>
  Every one of those names the thing that is wrong and, where there is one, the way out. A
  refusal you cannot act on would be worse than no feature.
</p>

<h2>A dozen to try</h2>
<pre><code>s#ordini                          every column, no conditions
s#ordini(codice,totale)[id=7]     two columns, one row
s#ordini&gt;clienti(nome)            joined on the foreign key
s#ordini[codice~'AB%']            LIKE
i#ordini                          skeleton with a placeholder per column
i#ordini(codice='AB',totale=15)   values, quoted by their types
i#ordini(id,codice)*5&lbrace;$, 'C_$$'&rbrace;  five numbered rows
u#ordini(stato='EV')[id=7]        set one column of one row
d#ordini[id=7]                    delete one row
m#ordini[id]                      insert-or-update, keyed on the id
a#ordini+nota:varchar(200)        add a column
a#ordini~totale:number(12,2)      change a type
fc#ordini[stato='EV']             loop over the matching rows</code></pre>

<h2>Where it works</h2>
<p>
  Wherever the editor has a catalogue: a query tab on a connection, and a script file whose
  dialect matches the active connection. Without one there is no type to decide a quote and
  no constraint to decide a join, so the shorthand is not offered at all rather than offered
  as a guess. If the object tree has never been opened for a connection, the abbreviation
  says so instead of expanding.
</p>
