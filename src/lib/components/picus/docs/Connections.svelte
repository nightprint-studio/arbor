<script lang="ts">
  /** Picus docs — Connections. */
</script>

<h1>Connections</h1>
<p class="doc-lead">
  Picus keeps several database sessions open at once, and makes sure you always know
  which one you are talking to.
</p>

<h2>Colour is the mechanism</h2>
<p>
  Every connection owns a colour from the shared palette. That colour appears on its row
  in the sidebar, on the tab of every document bound to it, and in the status bar. Two
  sessions on two databases are never confusable at a glance — the same idea Arbor uses
  for multi-repository workspaces.
</p>

<h2>A connection carries its scripts</h2>
<p>
  Picus is database-oriented: a repository of SQL scripts is <i>the folder this database is
  installed from</i>, so it is a property of the connection rather than of a separate
  "project". Set it in the connection editor under <b>Scripts</b>, with the folder picker
  or by typing the path; it is saved with the connection, so the repository is back the
  next time that database is opened.
</p>
<p>
  Selecting a connection brings its scripts, its inventory and its consistency report into
  the window; selecting another swaps all three. It is optional — a connection you only run
  queries against never needs one — and detaching it only stops Picus from showing the
  folder, it touches nothing on disk.
</p>
<p>
  A repository does not need a reachable server. An Oracle folder is read, checked and
  generated into with no Oracle session in existence, which is exactly what a folder
  written for an engine Picus has no driver for requires.
</p>

<h2>Read-only connections</h2>
<p>
  A connection marked read-only refuses every statement that is not a read. The refusal
  happens in the <b>backend</b>, not by hiding buttons in the interface: a write typed
  into a query tab comes back rejected with the reason. Use it for production.
</p>

<h2>Passwords</h2>
<p>
  Picus stores no password. Credentials live in Arbor's keychain; Picus asks for a handle
  and receives the secret at the moment of use. Nothing sensitive ends up in a project
  file, a configuration or a log.
</p>

<h2>Which ones are open</h2>
<p>
  Every row in the sidebar carries a dot beside its twisty: <b>filled green</b> when the
  session is open, <b>hollow</b> when it is not, <b>amber</b> while it is opening. Filled or
  hollow, not colour alone — the shape carries the answer on its own.
</p>
<p>
  In Picus green means <i>this session is open</i> and nothing else, which is why the
  connection colour picker does not offer green: a connection whose own colour was green
  would put two meanings in one colour, and colour is what the eye resolves first. The
  identity colour is also drawn as a <b>bar</b> rather than a dot — on the pill, and on
  every tab bound to the connection — so the two are told apart by shape as well.
</p>

<h2>Managing a connection</h2>
<p>
  Right-click a connection in the sidebar — or use the <b>⋯</b> button on its row — for
  everything you can do to the connection itself: connect and disconnect, open a query on
  it, re-read its schema, <b>edit</b> it, see its <b>details</b>, or <b>delete</b> it.
</p>
<p>
  <b>Details</b> is the read-only answer to "what is this connection": engine, address,
  schema, username, whether writes are refused, whether TLS is required, whether a password
  is stored, and the server it is talking to. It shows facts and cannot change them — the
  form is one button away when you want to.
</p>
<p>
  <b>Deleting</b> asks first, and says what goes with it: the open session is closed, and
  the password kept for that connection in Arbor's keychain is deleted too. Configuring the
  same connection again later means typing the password again. Scripts on disk, and anything
  already generated, are untouched.
</p>
<p>
  None of this needs the mouse. Every one of those actions is in the command palette by
  connection name — <i>Edit connection PROD…</i>, <i>Connection details: PROD</i>,
  <i>Delete connection PROD…</i> — and <kbd>F4</kbd> edits the active one directly.
</p>

<h2>What the tree shows</h2>
<p>
  A connection expands into four groups — <b>tables</b>, <b>views</b>, <b>sequences</b> and
  <b>triggers</b> — because "what is in this database" is not answerable from tables alone: a
  missing sequence or a trigger left disabled breaks an installation just as thoroughly, and
  both are things the scripts are supposed to create.
</p>
<p>
  Each group carries its count and starts <b>closed</b>, and every object is a single line
  carrying its name. A schema of several hundred tables is the normal case, so the name is
  what the list is for; the object's metadata — columns, foreign keys, row estimate, a
  sequence's step, a trigger's timing — appears at the end of the row you are pointing at,
  where it answers a question about one object instead of crowding all the others.
</p>
<p>
  The <b>filter</b> above the list is the way in at that scale: it searches connections and
  objects at once, and opens the groups that contain a match. A group with more matches than
  fit on screen stops and says so rather than drawing thousands of rows nobody scrolls
  through — narrow the filter and it comes back.
</p>
<p>
  Opening a connection reads its catalogue — the tables, views, sequences and triggers that
  completion, abbreviation expansion and the live checks all work from. A tab bound to a
  connection gets <i>that</i> connection's catalogue, so two tabs on two databases are each
  as clever as the other.
</p>
<p>
  A few catalogues are kept at a time, not all of them: they are large, and a database
  nobody has looked at since this morning is not worth the memory. A connection whose
  catalogue has been dropped says so instead of showing an empty tree — select it and it is
  read again. Nothing is ever re-read on a timer: a schema that reloads under you while you
  are writing DML from it is worse than a stale one you know is stale, so <b>Refresh</b> is
  yours to press.
</p>
<p>
  Opening a table shows its rows, its structure — columns, primary key, foreign keys in
  <i>both</i> directions, indexes, triggers — and its DDL. Views carry their defining query
  instead of constraints.
</p>
<p>
  A <b>sequence</b> is its properties and nothing else, because there is nothing else true
  about one. A <b>trigger</b> is its properties <i>and what it actually does</i>: the
  <code>CREATE TRIGGER</code> the server writes itself, and below it the source of the
  routine it fires. When it fires is the easy half of the question; the routine is the
  half you opened it for, and finding it in the scripts means first working out which of
  them installed this version. A routine written in C, or one this session may not read,
  says so rather than showing an empty box.
</p>
<p>
  Neither has a toolbar. Everything one would carry — sub-views, a new row, DML from the
  columns, a CSV export — is about something these two do not have.
</p>

<h2>Queries</h2>
<p>
  Each query tab is bound to one connection, shown in the bar above the editor with its
  schema and host. Rebinding a tab to another connection re-runs it there — the binding
  is explicit and visible, never a hidden global mode.
</p>
<ul>
  <li>A tab whose connection is <b>not open</b> says so before you press anything: the pill
    carries the same state dot the sidebar does, and <b>Connect</b> appears beside it. Nothing
    that needs a session is offered meanwhile — Run, Run all, the transaction controls — each
    saying why rather than merely greying out, and the live validation indicator falls back to
    "nothing to check against" instead of leaving its last verdict on screen.</li>
  <li>On the toolbar, colour means what the button does: <b>green</b> starts something
    (Run, Run all, Generate, Write), <b>red</b> stops it (Cancel), <b>blue</b> writes to
    disk (Save). Everything that only reads or exports stays grey, and a coloured button
    turns grey the moment it is unavailable.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Enter</kbd> runs the statement under the cursor, or the selection.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> runs the whole script.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> cancels a running query, or the row count
    running behind one. Press it <b>again</b> to stop waiting for the server: the connection is
    dropped and a new one opened, so the tab is usable even when the database will not answer.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> cycles the active connection.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Click</kbd> on a name in the SQL opens that table, view, sequence
    or trigger's structure — the schema is the authority on whether the word names one, so it
    works on a half-typed statement and on a fragment pasted out of a log.</li>
</ul>
<p>
  The time each statement took is beside the row count, and beside the “rows affected” of a
  write — a question asked about an <code>UPDATE</code> at least as often as about a
  <code>SELECT</code>. Per-statement times are in <b>Messages</b>; the number in the header is
  the whole run, so <b>Run all</b> reports one figure for the one thing you asked for.
</p>

<h2>Query tabs survive the window closing</h2>
<p>
  A scratchpad is where the work happens — a <code>SELECT</code> refined eight times, the
  <code>UPDATE</code> it turned into — and none of it is a file. Picus remembers the text,
  the title and the connection of every query tab, writes them shortly after you stop typing
  and once more as the window closes, and re-opens them next time.
</p>
<p>
  What comes back is the <b>buffer</b>, never the rows: a result is a cursor on a server that
  closed with the process, and showing a grid of rows that no longer exist would be a lie told
  at startup. Empty tabs are not restored — an untouched tab is not work.
</p>

<h2>The data grid</h2>
<p>
  Results and table contents use the same grid, so they always read the same way:
  <b>NULL</b> is muted italics, an <b>empty string</b> is a small dashed box, and numbers
  are right-aligned with tabular figures so magnitudes line up down the column. Confusing
  a null with an empty string is expensive when you are about to write DML from what you
  see.
</p>
<h2>Which table a column came from</h2>
<p>
  When a result draws on <b>more than one table</b>, each column gets a small coloured bar
  under its name and a legend appears above the grid saying which colour is which table.
  Clicking a table in the legend dims every column that is not its own, so a join forty
  columns wide can be read one table at a time; clicking it again brings everything back.
</p>
<p>
  A result from a single table shows neither — there would be nothing to tell apart.
  Hovering any column header still names its origin as
  <code>table.column</code>, which is how you see what a column aliased to something
  else really is. Columns that are computed rather than read — a <code>count(*)</code>,
  an expression — have no origin and carry no bar.
</p>
<p>
  The origins come from the server's own description of the result, not from reading the
  statement, so they are right about <code>*</code>, aliases and subqueries.
</p>
<p>
  A <b>view counts as one table</b> — its own. The server names the relation the statement
  asked for, and that is the view, not the tables inside it; so selecting from a single
  view shows no bars and no legend however many tables it joins. Joining two views shows
  two. A statement the server cannot describe — a multi-statement paste, a
  <code>SET</code> — shows no origins at all rather than guessing.
</p>
<h2>Tracing a column through the views</h2>
<p>
  When the answer you want is <i>which table is this really in</i>, and the query reads a
  view that reads another view, the <b>Lineage</b> pane follows it. Press <b>Trace
  columns</b> and each column of the result is walked back through the views to the table
  it comes from:
</p>
<pre><code>CODSA  ←  V_TIPI.CENINT  ←  TAB_TIPI.CENINT</code></pre>
<p>
  The chain matters as much as its end — it is what shows <i>which view renamed it</i>,
  and a column whose name changed on the way is marked.
</p>
<p>
  Not every column has one table behind it, and the pane distinguishes the reasons rather
  than lumping them together:
</p>
<ul>
  <li><b>Computed</b> — an expression, a function, a concatenation. It names what the value
    is made of, and there is nothing to write back through.</li>
  <li><b>One of several</b> — a <code>UNION</code> whose arms read different tables. The
    value <i>is</i> a real column, of one table for some rows and another for the rest;
    which one a given row came from is not in the result. Deliberately not called
    computed: there are two writable tables here, not none.</li>
  <li><b>Not followed</b> — the walk stopped, and says where and why: an ambiguous bare
    name, a table in another schema, a construct Picus does not read.</li>
</ul>
<p>
  This is a <b>deduction</b>, not the server's own answer — it is read out of the views'
  SQL, so it can be wrong where the reported origins cannot. That is why it is asked for
  and never computed behind a query, and why, once traced, the grid's colour bars turn
  <b>dashed</b> and the legend says <i>traced to</i>: two colourings that looked identical
  would be the one way to misread this badly. Running a new statement drops the trace
  rather than quietly redoing it.
</p>
<h2>Scrolling a result</h2>
<p>
  A query result and a table's data behave identically: one <b>continuous scroll</b> over
  the whole thing. The scrollbar is the length of the result from the first frame, and rows
  arrive in windows as you approach them — you scroll, and the next stretch is already being
  fetched before you reach the edge of what is loaded. Rows that have not arrived yet draw
  as quiet placeholder bars, so the scrollbar never lies about how much there is.
</p>
<p>
  This works because a read leaves a <b>cursor open on the server</b> and Picus reads
  forward through it, rather than re-running the statement with a new
  <code>OFFSET</code> each time. Scrolling therefore never repeats a row and never skips
  one, even on a table being written to while you read it. The cursor is released when you
  close the tab, when you run another statement in it, and when the connection is closed.
</p>
<p>
  <b>Settings → Queries → row limit</b> is how many rows come back per window. It is not a
  ceiling any more: a bigger number means fewer, larger trips, a smaller one means more,
  smaller trips.
</p>

<h2>How long is it, really</h2>
<p>
  The total appears in the <b>status bar</b>, and starts as the server's <b>estimate</b>.
  An estimate is marked with a <code>~</code> everywhere it is shown — beside the result,
  in the status bar, in the query history. Meanwhile Picus counts the result exactly, in the
  background; when that count lands the <code>~</code> disappears and the number is the real
  one. Nothing waits for it, and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> stops it if
  you would rather not pay for a count on a very large table.
</p>
<p>
  The number is never quietly promoted from guess to fact: for as long as it carries the
  <code>~</code>, it can be out — sometimes considerably, on a table that has just been
  written to.
</p>

<h2>Filtering by what is actually there</h2>
<p>
  Each column's filter box has a small funnel beside it that opens <b>the values that
  column holds</b>, each with the number of rows it accounts for. Tick the ones you want
  and the grid narrows as you go; the box then shows what is picked, and the <b>×</b>
  beside it clears the column.
</p>
<p>
  The list is exact, and it is not the same thing as typing: picking <code>ROMA</code>
  selects the rows whose value <i>is</i> <code>ROMA</code>, where typing <code>ROMA</code>
  would also bring back <code>ROMANO</code>. A column is filtered one way or the other,
  never both.
</p>
<p>
  Each column's list is narrowed by the filters on the <b>other</b> columns, so after
  choosing a region the province list is that region's provinces. It is taken when the
  list opens and does not move while it is open — otherwise ticking a value would delete
  every other value from the list you are picking from. Columns holding thousands of
  distinct values list the first few hundred and say so: at that point the text box is
  the better tool.
</p>
<p>
  A grid narrowed to nothing says so, and offers to clear the filters — an empty result
  and a result you filtered away look identical otherwise.
</p>

<h2>Sorting and filtering a result you only partly hold</h2>
<p>
  While a result is still filling, the column sort and the per-column filters are
  <b>visible and unavailable</b>, and say why when you point at them. Sorting a tenth of a
  result looks exactly like sorting the whole one, and the row you were looking for is
  simply not in the part that was in memory.
</p>
<p>
  As soon as the whole result is loaded — which for most queries is immediately, on the
  first window — both come back and behave as they always have. Until then the row counter
  in the footer reads <b>loaded of total</b> rather than just the total, and pointing
  at it says why the two controls are standing down.
</p>
<p>
  You do not have to scroll to the end to get them back: the button at the <b>head of the
  filter row</b> fetches the rest in one act, and turns into a stop while it runs. What has
  already arrived is kept if you stop it.
</p>
<p>
  On a result too large to hold in memory at once the button is <b>not offered</b> — past
  that size the oldest windows are dropped as new ones arrive, so it would fetch forever
  and never hand the controls back. Narrow the query with a <code>WHERE</code> instead;
  the row counter's tooltip says which of the two cases you are in.
</p>
<p>
  To reach a specific distant row, query it with a <code>WHERE</code> on an indexed column
  rather than scrolling to it: that lands in one step, where scrolling to row four million
  makes the server walk there.
</p>

<h2>Taking a result out</h2>
<p>
  The download button above the grid offers four renditions, to the clipboard or to a file:
</p>
<ul>
  <li><b>CSV</b> — RFC 4180, for a spreadsheet;</li>
  <li><b>INSERT statements</b> — the rows as the SQL that would recreate them. Written by the
    same emitter that writes the destination scripts, so quoting comes from each column's
    declared type and the result is one statement with a tuple per row on PostgreSQL, one per
    row on Oracle;</li>
  <li><b>JSON</b> — one object per row;</li>
  <li><b>Markdown</b> — a pipe table, for a ticket or a message.</li>
</ul>
<p>
  <code>INSERT</code> needs to know which table, so it is offered when the rows come from one:
  a relation tab knows, and a query tab has it read off a statement that selects from exactly
  one table. A join or a computed result exports as a table and not as SQL.
</p>
<p>
  What is exported is <b>what is loaded</b>, and the button says so — <i>1,200 of ~40,000
  rows</i> — rather than leaving you to discover it after sending the file.
</p>

<h2>Editing rows in the grid</h2>
<p>
  Double-click a cell to change it. <kbd>Enter</kbd> writes what is in the box —
  including nothing, which is the empty string — <kbd>Ctrl</kbd>+<kbd>Enter</kbd> writes
  <code>NULL</code>, and <kbd>Esc</kbd> leaves the cell alone. Changed cells are marked, and
  the tooltip on each says what it was.
</p>
<p>
  <b>Right-click a cell</b> for the same things without the shortcuts: copy the value, copy
  the whole row (tab-separated, so it pastes into a spreadsheet as columns), copy the column
  name, and <b>Set</b> — <code>NULL</code> or empty text, named side by side because they are
  different values and the difference is what this grid is careful about. A cell with a
  pending change also offers to restore just that one. On a large object the menu is how you
  open the value, since it was never fetched with the rest of the row — and how you
  <b>replace it from a file</b>.
</p>
<p>
  A <b>text</b> column takes a file too — <i>Load from file…</i> — and there the file's
  contents become an ordinary pending change: marked in the grid, written by <b>Store</b>,
  undone by <b>Restore</b>, like anything else you type. The file is read as UTF-8 where it
  is valid UTF-8 and as windows-1252 where it is not, and the dialog <b>says which</b>; a
  byte-order mark is a declaration and never becomes the first character of the value. If the
  text is longer than the length the column declares, that is said before you accept it
  rather than discovered when the server refuses it.
</p>
<p>
  A <b>large object</b> is the exception: it is written <b>straight away</b>, and it is the
  one write in the grid that does not wait for Store. Bytes cannot travel as a pending
  change — those carry text, so a file would be stored as its own base64 and the cell would
  look written while the document was broken. With nothing to review afterwards, the review
  happens first: the dialog names the column, the file and its size. <b>Restore does not undo
  it.</b> What comes back is re-read from the server rather than assumed, so the size shown
  is the stored value's and not the file's.
</p>
<p>
  Nothing appears in that menu that cannot happen: on a read-only connection, or a result
  whose rows cannot be addressed, the editing entries are absent rather than present and
  refusing.
</p>
<p>
  <b>Nothing is written until you press Store.</b> There is no autosave and no write when you
  leave a cell: a bar above the grid counts what is pending, <b>Store</b>
  (<kbd>Ctrl</kbd>+<kbd>S</kbd>) writes it and <b>Restore</b> puts every cell back. After a
  write the query is re-run, so what you see is what the server has — a trigger or a default
  may have done more than you asked.
</p>
<p>
  The batch goes out as one call, so on PostgreSQL it lands or it does not. The SQL that ran is
  shown afterwards: it is the one write in this product you did not read beforehand, so you
  get to read it after — and it is ready to paste into the script this change probably also
  belongs in.
</p>
<p>
  Editing is offered only when a row can be <b>addressed</b>: the rows come from one table,
  that table has a primary key, and the result includes it. Otherwise the header says
  <i>read-only rows</i> and the tooltip says which of the three is missing. An update matching
  on the values themselves would quietly change nothing the moment one of them differed, and
  doing nothing quietly is worse than refusing. The <code>WHERE</code> is built from the values
  the row was <b>read</b> with, which is why a key column can itself be edited.
</p>

<h2>Large objects are not fetched</h2>
<p>
  Reading a table with a <code>bytea</code> or a <code>blob</code> column in it pulls every
  byte of every row across the connection to draw a grid that cannot show any of it. On a
  table of scanned documents that is the difference between a read that returns and one that
  looks exactly like the application having hung.
</p>
<p>
  The window you scroll is bounded, and so is what the server does to fill it: the first
  window is read on its own, and the scrollable snapshot behind it is only taken when you
  scroll past that window. The consequence is worth knowing — those are two reads, so a
  statement with no <code>ORDER BY</code> may show a row twice, or skip one, exactly at that
  boundary. Add an ordering and it cannot.
</p>
<p>
  So Picus asks for the <b>size</b> of those columns instead, and the cell shows it as a chip.
  Click it — or reach it with <kbd>Tab</kbd> and press <kbd>Enter</kbd> — and Picus reads that
  one value: text as text, bytes as a hex dump with its ASCII column, and a
  <b>Save to a file</b> that writes the real bytes rather than their encoding. Values past
  4&nbsp;MB are truncated for reading and say so; saving still writes what was read.
</p>

<h3>However you asked for it</h3>
<p>
  It applies to a relation tab, to <code>SELECT * FROM ordini</code>, to
  <code>SELECT allegato FROM ordini</code> and to a join or a union that happens to carry the
  column. Picus decides from what the <i>result</i> contains — the server describes the columns
  before any row is sent — rather than from reading your SQL, so there is no way of writing the
  statement that drags the bytes across by accident.
</p>
<p>
  Opening a cell needs something that identifies the row — and Picus arranges that for you.
  When the statement reads from one table but did not select its key, the key is added to the
  read <b>invisibly</b>: the primary key spliced in as a hidden column, or, for a table that
  has no primary key, the engine's own internal row address. The grid never shows it; the
  cell just opens. Only a result with no single row behind it — a join, a view, a computed
  result — cannot be addressed at all, and there the value is shown in full instead of a size
  you could never expand.
</p>
<p>
  <b>One exception, and it is deliberate.</b> A statement with its own <code>ORDER BY</code> is
  never rewritten, so it carries its large objects. Masking means wrapping the statement, and
  PostgreSQL is free to hand a wrapped statement's rows back in another order — a grid in the
  wrong order is a wrong answer, where a slow one is only slow. The read is still bounded to
  the window you are looking at. If it matters, name the columns you want instead of
  <code>*</code>, or drop the ordering while you browse.
</p>
<p>
  The size in the chip is the value's <b>stored</b> size. Getting the exact length would mean
  fetching and decompressing every value, which is most of the cost this avoids. For scans,
  images and PDFs the two agree to within a few bytes; for something highly compressible the
  chip reads smaller than the value is. Open the cell and the exact length is on the value
  itself.
</p>

<h2>Explicit transactions</h2>
<p>
  Statements normally commit as they run. <b>Begin</b> stops that: everything after it is
  held until you decide. The toolbar says so while one is open — it changes the meaning of
  every statement after it, so it is not something to have to remember — and
  <b>Commit</b> and <b>Rollback</b> are beside it. Closing the connection or the window
  with one open asks first, and says that the answer is a rollback.
</p>
<p>
  A statement that fails inside a transaction leaves it in a state where the server accepts
  nothing further. Picus shows that state as its own thing, and offers only the rollback,
  because that is the only thing that can succeed.
</p>
<p>
  <b>What a rollback undoes depends on the engine, and Picus says which.</b> PostgreSQL's
  DDL is transactional: a rolled-back transaction really does undo a
  <code>CREATE TABLE</code>. Oracle commits implicitly before and after every DDL statement,
  so the first <code>ALTER</code> closes the transaction whatever the client asked for — no
  driver can prevent that. The connection's engine declares which of the two it is, and the
  controls state it rather than promising a rollback the server will not honour.
</p>

<h2>Bind variables</h2>
<p>
  A statement with placeholders — <code>:CODICE</code> on Oracle, <code>$1</code> on
  PostgreSQL — asks for their values before it runs, one field each, and remembers them for
  that tab. Every field has an explicit <b>NULL</b> toggle: NULL and the empty string are
  different values, and confusing them is how a wrong <code>UPDATE</code> gets written.
</p>
<p>
  The values are <b>bound</b>, never spliced into the text. That is the point: a value pasted
  into SQL has to be quoted by whoever pastes it, which is how both injection and mangled
  apostrophes happen. What is sent to the server is the statement and the values, separately,
  and the server — which knows the column's type — does the conversion.
</p>
<p>
  Placeholders are found by the same scanner that colours the buffer, so
  <code>::</code> (a PostgreSQL cast), <code>:=</code> (a PL/SQL assignment),
  <code>:NEW</code> inside a trigger body, and anything inside a string or a comment are not
  mistaken for one. A bound read is not scrollable — the engine's cursors do not take
  parameters — so it returns one window and says so.
</p>

<h2>The plan</h2>
<p>
  Beside the rows a statement returned, a <b>Plan</b> pane, in three readings of the same
  answer. <b>Steps</b> is the indented tree, in execution order, with the cost and rows on
  each one. <b>Text</b> is the engine's own output, for pasting somewhere else.
</p>
<p>
  <b>Diagram</b> is the shape: the root on top, its inputs below it, and every edge drawn
  <i>as thick as the number of rows travelling along it</i> — so the place where a thin line
  becomes a rope is where the query went wrong, found without reading a number. Under each box
  a bar shows how much of the work happens at <i>that</i> node rather than in its subtree,
  which is the difference between a plan that blames its root for everything and one that
  points at the step you can do something about. Colour is spent on one thing only: a node
  whose row estimate was wrong by the same factor the Steps list badges. Click a box for the
  server's own filter, sort and index conditions.
</p>
<p>
  <b>Analyze</b> is a separate action, and it is separate because it <i>runs the statement</i>.
  On a <code>SELECT</code> that is only slow; on a <code>DELETE</code> it is the delete. It is
  refused for anything that is not a read, refused on a read-only connection, and the pane
  always says whether the numbers you are reading are an estimate or a measurement — which is
  the most important distinction on that screen.
</p>
<p>
  Worth knowing when reading a plan here: rows are streamed through a held cursor, and
  PostgreSQL picks fast-start plans for cursors that it would not pick for the same statement
  run whole. That is a real difference and not a display artefact — it is why the plan is
  shown beside the rows rather than left to another tool.
</p>

<h2>Sessions</h2>
<p>
  <b>Sessions</b> — on the right-hand rail — is what the server is doing right now: every
  connected backend, what it is running, how long it has been running it, and what it is
  waiting on. Refreshed every few seconds while the panel is open and not at all while it is
  closed, because a poll against a production server should cost nothing when nobody is
  looking.
</p>
<p>
  Blocking is shown as a chain rather than a flag. "This session is blocked" is not
  actionable; "this is blocked by pid 4412, which is idle in a transaction" is — so the chain
  is drawn to the session at the root of it, which is the only one worth doing anything about.
  Picus's own session is labelled.
</p>
<p>
  Two verbs, and they are genuinely different. <b>Cancel</b> asks the running statement to
  stop and leaves the connection alive; it is almost always the right one. <b>Terminate</b>
  drops the connection and rolls its transaction back, and is the answer only for a session
  that is not running anything — the abandoned <code>idle in transaction</code> holding a
  lock. Both confirm first, and the confirmation says what will happen rather than asking
  whether you are sure.
</p>

<h2>Dependencies</h2>
<p>
  What needs what: foreign keys, the tables a view reads, the table a trigger is installed on
  and the routine it fires, the sequence a column's default draws from.
</p>
<p>
  Shown from the object you select, as two trees — <b>depends on</b> and <b>used by</b> — each
  expandable, each edge saying <i>why</i> it is there. Not as a free-floating graph: on a
  schema with several hundred tables that is a cloud nobody can read. Cycles are reported
  rather than followed forever, and anything the catalogue could not resolve is listed rather
  than quietly dropped — a graph that omits what it did not understand cannot be trusted to
  order anything.
</p>
<p>
  Which is the point of it beyond looking: <b>creation order</b> sorts the objects
  topologically, and that is the order anything emitting them has to use.
</p>
