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
  Only the connection currently in use has a catalogue loaded; the others say so rather than
  showing another database's tables under their name.
</p>
<p>
  Opening a table shows its rows, its structure — columns, primary key, foreign keys in
  <i>both</i> directions, indexes, triggers — and its DDL. Views carry their defining query
  instead of constraints; sequences and triggers show their properties, because there is
  nothing else true about them.
</p>

<h2>Queries</h2>
<p>
  Each query tab is bound to one connection, shown in the bar above the editor with its
  schema and host. Rebinding a tab to another connection re-runs it there — the binding
  is explicit and visible, never a hidden global mode.
</p>
<ul>
  <li><kbd>Ctrl</kbd>+<kbd>Enter</kbd> runs the statement under the cursor, or the selection.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> runs the whole script.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> cancels a running query, or the row count
    running behind one.</li>
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

<h2>Sorting and filtering a result you only partly hold</h2>
<p>
  While a result is still filling, the column sort and the per-column filters are
  <b>visible and unavailable</b>, and say why when you point at them. Sorting a tenth of a
  result looks exactly like sorting the whole one, and the row you were looking for is
  simply not in the part that was in memory.
</p>
<p>
  As soon as the whole result is loaded — which for most queries is immediately, on the
  first window — both come back and behave as they always have. A note above the rows says
  how many of how many are loaded while that is not yet the case, and leaves when it is.
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
  Opening a cell is a separate matter: that needs something that identifies the row, so a
  result with no key column in it shows the sizes but cannot fetch any of them. Select the key
  column too and they open. Sizes you cannot open are a smaller problem than a read you cannot
  wait out, which is why the masking happens either way.
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
