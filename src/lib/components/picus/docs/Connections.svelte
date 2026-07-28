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
</ul>

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
