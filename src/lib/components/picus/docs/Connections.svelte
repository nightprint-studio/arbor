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
  A repository does not need a reachable server. An Oracle branch is read, checked and
  generated into with no Oracle session in existence, which is exactly what a branch of a
  dialect Picus has no driver for requires.
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
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> cancels a running query.</li>
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
<p>
  Table data is <b>paged</b>, and each page is rendered through the same virtualised
  viewport: paging bounds what is fetched, virtualisation bounds what is drawn. That is why
  the page sizes go up to ten thousand — a large page costs no more to display than a small
  one, and "1–500 of 4,210" is an answer where an endless scrollbar is not.
</p>
<p>
  The row count beside a table is the server's <b>estimate</b>, marked with a <code>~</code>.
  Picus never runs a <code>count(*)</code> to label a page: on the tables it is pointed at,
  counting them would cost more than reading them.
</p>
<p>
  A page is an <code>OFFSET</code> on the server, and the server reaches a given offset by
  walking the rows before it — so pages deep into a large table keep getting slower. Picus
  says so when you get there. To land on a distant row directly, query it with a
  <code>WHERE</code> on an indexed column instead of paging to it.
</p>

<h2>The row limit on query results</h2>
<p>
  A query tab fetches at most <b>Settings → Queries → row limit</b> rows. Wherever the
  statement allows it, that limit is applied by the <b>server</b>: a
  <code>SELECT * FROM orders</code> on a few million rows stops at the limit there, instead
  of crossing the network in full to be cut down afterwards.
</p>
<p>
  When a result hits the limit, the grid says so — a <b>capped</b> marker beside the row
  count and a banner above the rows. This matters more than it looks: a result cut at the
  limit and a result that genuinely ended are otherwise identical on screen, and the
  sorting and per-column filters in the grid apply only to the rows that were fetched, not
  to the rest of the table. Raise the limit, or narrow the statement.
</p>
