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

<h2>What the tree shows</h2>
<p>
  A connection expands into four groups — <b>tables</b>, <b>views</b>, <b>sequences</b> and
  <b>triggers</b> — because "what is in this database" is not answerable from tables alone: a
  missing sequence or a trigger left disabled breaks an installation just as thoroughly, and
  both are things the scripts are supposed to create.
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
